use crate::domain::transport::{
    TransportRuntimeEffects, TransportRuntimeInputEvent, TransportRuntimeLaunchResult,
    TransportRuntimeOutputEvent,
};
use crate::domain::transport_runtime_registry::{
    TransportRuntimeLaunchAttempt, TransportRuntimeProcessProbe, TransportRuntimeProfile,
};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

struct ManagedLocalCommandRuntimeProcess {
    pid: u32,
    child: Child,
    runtime_effects: Arc<Mutex<TransportRuntimeEffects>>,
}

pub fn launch_local_command_runtime(
    profile: &TransportRuntimeProfile,
) -> TransportRuntimeLaunchAttempt {
    match local_command_runtime_registry()
        .lock()
        .map_err(|_| "local command runtime registry lock poisoned".to_string())
    {
        Ok(mut registry) => {
            if let Some(reused_pid) = managed_runtime_pid(&mut registry, &profile.circle_id) {
                return TransportRuntimeLaunchAttempt {
                    result: TransportRuntimeLaunchResult::Reused,
                    pid: Some(reused_pid),
                    detail: None,
                };
            }
        }
        Err(detail) => {
            return TransportRuntimeLaunchAttempt {
                result: TransportRuntimeLaunchResult::Failed,
                pid: None,
                detail: Some(detail),
            };
        }
    }

    let command = profile
        .resolved_launch_command
        .as_deref()
        .or(profile.launch_command.as_deref());

    let Some(command) = command else {
        return TransportRuntimeLaunchAttempt {
            result: TransportRuntimeLaunchResult::Failed,
            pid: None,
            detail: Some("runtime launch command is not configured".into()),
        };
    };

    if let Err(error) = reset_local_command_runtime_input_queue(&profile.circle_id) {
        return TransportRuntimeLaunchAttempt {
            result: TransportRuntimeLaunchResult::Failed,
            pid: None,
            detail: Some(error),
        };
    }

    let mut child = Command::new(command);
    child
        .args(&profile.launch_arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    match child.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            let runtime_effects = Arc::new(Mutex::new(TransportRuntimeEffects::default()));
            if let Some(stdout) = child.stdout.take() {
                spawn_runtime_output_reader(stdout, Arc::clone(&runtime_effects));
            }
            match local_command_runtime_registry()
                .lock()
                .map_err(|_| "local command runtime registry lock poisoned".to_string())
            {
                Ok(mut registry) => {
                    registry.insert(
                        profile.circle_id.clone(),
                        ManagedLocalCommandRuntimeProcess {
                            pid,
                            child,
                            runtime_effects,
                        },
                    );
                    TransportRuntimeLaunchAttempt {
                        result: TransportRuntimeLaunchResult::Spawned,
                        pid: Some(pid),
                        detail: None,
                    }
                }
                Err(detail) => TransportRuntimeLaunchAttempt {
                    result: TransportRuntimeLaunchResult::Failed,
                    pid: None,
                    detail: Some(detail),
                },
            }
        }
        Err(error) => TransportRuntimeLaunchAttempt {
            result: TransportRuntimeLaunchResult::Failed,
            pid: None,
            detail: Some(format!("failed to launch `{command}`: {error}")),
        },
    }
}

pub fn stop_local_command_runtime(circle_id: &str) -> Result<(), String> {
    let mut registry = local_command_runtime_registry()
        .lock()
        .map_err(|_| "local command runtime registry lock poisoned".to_string())?;
    let Some(mut process) = registry.remove(circle_id) else {
        let _ = reset_local_command_runtime_input_queue(circle_id);
        return Ok(());
    };

    match process.child.try_wait() {
        Ok(Some(_)) => {
            let _ = reset_local_command_runtime_input_queue(circle_id);
            Ok(())
        }
        Ok(None) => {
            process.child.kill().map_err(|error| {
                format!(
                    "failed to stop local runtime pid {} for circle `{circle_id}`: {error}",
                    process.pid
                )
            })?;
            let _ = process.child.wait();
            let _ = reset_local_command_runtime_input_queue(circle_id);
            Ok(())
        }
        Err(error) => Err(format!(
            "failed to inspect local runtime pid {} for circle `{circle_id}`: {error}",
            process.pid
        )),
    }
}

pub fn probe_local_command_runtime(
    circle_id: &str,
) -> Result<Option<TransportRuntimeProcessProbe>, String> {
    let mut registry = local_command_runtime_registry()
        .lock()
        .map_err(|_| "local command runtime registry lock poisoned".to_string())?;
    let Some(process) = registry.get_mut(circle_id) else {
        return Ok(Some(TransportRuntimeProcessProbe {
            detail: format!(
                "local runtime handle for circle `{circle_id}` is not registered in this app session"
            ),
        }));
    };

    let pid = process.pid;
    let exit_detail = match process.child.try_wait() {
        Ok(Some(status)) => Some(format!(
            "local runtime pid {pid} exited with status {status}"
        )),
        Ok(None) => None,
        Err(error) => Some(format!(
            "failed to inspect local runtime pid {pid}: {error}"
        )),
    };

    if exit_detail.is_none() {
        return Ok(None);
    }

    registry.remove(circle_id);
    let _ = reset_local_command_runtime_input_queue(circle_id);
    Ok(exit_detail.map(|detail| TransportRuntimeProcessProbe { detail }))
}

pub fn drain_local_command_runtime_effects(
    circle_id: &str,
) -> Result<TransportRuntimeEffects, String> {
    let runtime_effects = {
        let registry = local_command_runtime_registry()
            .lock()
            .map_err(|_| "local command runtime registry lock poisoned".to_string())?;
        registry
            .get(circle_id)
            .map(|process| Arc::clone(&process.runtime_effects))
    };

    let Some(runtime_effects) = runtime_effects else {
        return Ok(TransportRuntimeEffects::default());
    };

    let mut queued_runtime_effects = runtime_effects
        .lock()
        .map_err(|_| "local command runtime effects lock poisoned".to_string())?;
    let mut drained = TransportRuntimeEffects::default();
    std::mem::swap(&mut *queued_runtime_effects, &mut drained);
    Ok(drained)
}

pub fn enqueue_local_command_runtime_input(
    circle_id: &str,
    event: &TransportRuntimeInputEvent,
) -> Result<(), String> {
    let path = local_command_runtime_input_path(circle_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!("failed to prepare local runtime input queue for circle `{circle_id}`: {error}")
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| {
            format!("failed to open local runtime input queue for circle `{circle_id}`: {error}")
        })?;
    serde_json::to_writer(&mut file, event).map_err(|error| {
        format!(
            "failed to encode local runtime input queue event for circle `{circle_id}`: {error}"
        )
    })?;
    file.write_all(b"\n").map_err(|error| {
        format!(
            "failed to append local runtime input queue event for circle `{circle_id}`: {error}"
        )
    })?;
    file.flush().map_err(|error| {
        format!("failed to flush local runtime input queue event for circle `{circle_id}`: {error}")
    })?;
    Ok(())
}

fn local_command_runtime_registry(
) -> &'static Mutex<HashMap<String, ManagedLocalCommandRuntimeProcess>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, ManagedLocalCommandRuntimeProcess>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn managed_runtime_pid(
    registry: &mut HashMap<String, ManagedLocalCommandRuntimeProcess>,
    circle_id: &str,
) -> Option<u32> {
    let mut remove_stale = false;
    let pid = registry
        .get_mut(circle_id)
        .and_then(|process| match process.child.try_wait() {
            Ok(Some(_)) => {
                remove_stale = true;
                None
            }
            Ok(None) => Some(process.pid),
            Err(_) => {
                remove_stale = true;
                None
            }
        });

    if remove_stale {
        registry.remove(circle_id);
    }

    pid
}

fn spawn_runtime_output_reader(
    stdout: std::process::ChildStdout,
    runtime_effects: Arc<Mutex<TransportRuntimeEffects>>,
) {
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let Ok(event) = serde_json::from_str::<TransportRuntimeOutputEvent>(trimmed) else {
                continue;
            };
            let Ok(mut queued_runtime_effects) = runtime_effects.lock() else {
                break;
            };
            queued_runtime_effects.push_runtime_output_event(event);
        }
    });
}

fn reset_local_command_runtime_input_queue(circle_id: &str) -> Result<(), String> {
    let path = local_command_runtime_input_path(circle_id);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "failed to reset local runtime input queue for circle `{circle_id}`: {error}"
        )),
    }
}

fn local_command_runtime_input_path(circle_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("p2p-chat-runtime")
        .join("requests")
        .join(format!("{}.jsonl", sanitize_runtime_identifier(circle_id)))
}

fn sanitize_runtime_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::transport::{
        TransportCircleAction, TransportRuntimeActionRequest, TransportRuntimeAdapterKind,
        TransportRuntimeLaunchStatus, TransportRuntimeRecoveryPolicy, TransportRuntimeState,
    };
    use crate::domain::transport_runtime_registry::{
        TransportRuntimeLabels, TransportRuntimeProcessProbe, TransportRuntimeProfile,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unix_sleep_command() -> (String, Vec<String>) {
        ("sh".into(), vec!["-c".into(), "sleep 30".into()])
    }

    fn windows_sleep_command() -> (String, Vec<String>) {
        (
            "cmd".into(),
            vec!["/C".into(), "ping -n 30 127.0.0.1 > NUL".into()],
        )
    }

    fn sleeping_command() -> (String, Vec<String>) {
        if cfg!(windows) {
            windows_sleep_command()
        } else {
            unix_sleep_command()
        }
    }

    fn labels() -> TransportRuntimeLabels {
        TransportRuntimeLabels {
            inactive_event: "native runtime idle",
            starting_event: "native runtime booting",
            active_event: "native runtime active",
            connect_event: "native runtime booted",
            disconnect_event: "native runtime released",
            sync_event: "native relay checkpoint committed",
            discover_event: "native discovery sweep committed",
            sync_sessions_event: "native session merge committed",
        }
    }

    fn profile(
        circle_id: &str,
        command: Option<String>,
        args: Vec<String>,
    ) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            circle_id: circle_id.into(),
            driver: "native-preview-relay-runtime".into(),
            adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
            launch_status: TransportRuntimeLaunchStatus::Ready,
            launch_command: command.clone(),
            launch_arguments: args,
            resolved_launch_command: command,
            launch_error: None,
            recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
            state: TransportRuntimeState::Starting,
            session_label: "native::ws::circle-1".into(),
            endpoint: "native://relay/circle-1".into(),
            labels: labels(),
        }
    }

    fn unique_circle_id(prefix: &str) -> String {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        format!("{prefix}-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn launcher_returns_spawned_with_pid_for_valid_command() {
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let circle_id = unique_circle_id("spawned");
        let launch_attempt = launch_local_command_runtime(&profile(
            &circle_id,
            Some(current_executable.to_string_lossy().into_owned()),
            vec!["--help".into()],
        ));

        assert!(matches!(
            launch_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));
        assert!(launch_attempt.pid.is_some());
        assert_eq!(launch_attempt.detail, None);
        stop_local_command_runtime(&circle_id).expect("launcher cleanup should succeed");
    }

    #[test]
    fn launcher_returns_failed_when_command_cannot_spawn() {
        let invalid_command = std::env::temp_dir();
        let circle_id = unique_circle_id("failed");
        let launch_attempt = launch_local_command_runtime(&profile(
            &circle_id,
            Some(invalid_command.to_string_lossy().into_owned()),
            Vec::new(),
        ));

        assert!(matches!(
            launch_attempt.result,
            TransportRuntimeLaunchResult::Failed
        ));
        assert_eq!(launch_attempt.pid, None);
        assert!(launch_attempt
            .detail
            .as_deref()
            .is_some_and(|message| message.contains("failed to launch")));
    }

    #[test]
    fn launcher_reuses_existing_managed_process_for_same_circle() {
        let circle_id = unique_circle_id("reused");
        let (command, args) = sleeping_command();
        let profile = profile(&circle_id, Some(command), args);
        let first_attempt = launch_local_command_runtime(&profile);
        let second_attempt = launch_local_command_runtime(&profile);

        assert!(matches!(
            first_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));
        assert!(matches!(
            second_attempt.result,
            TransportRuntimeLaunchResult::Reused
        ));
        assert_eq!(first_attempt.pid, second_attempt.pid);

        stop_local_command_runtime(&circle_id).expect("launcher cleanup should succeed");
    }

    #[test]
    fn launcher_stop_releases_managed_process() {
        let circle_id = unique_circle_id("stop");
        let (command, args) = sleeping_command();
        let profile = profile(&circle_id, Some(command), args);
        let first_attempt = launch_local_command_runtime(&profile);

        assert!(matches!(
            first_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));

        stop_local_command_runtime(&circle_id).expect("launcher stop should succeed");

        let second_attempt = launch_local_command_runtime(&profile);
        assert!(matches!(
            second_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));

        stop_local_command_runtime(&circle_id).expect("launcher cleanup should succeed");
    }

    #[test]
    fn probe_returns_none_while_managed_process_is_running() {
        let circle_id = unique_circle_id("probe-running");
        let (command, args) = sleeping_command();
        let profile = profile(&circle_id, Some(command), args);
        let launch_attempt = launch_local_command_runtime(&profile);

        assert!(matches!(
            launch_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));
        assert_eq!(
            probe_local_command_runtime(&circle_id).expect("probe should succeed"),
            None
        );

        stop_local_command_runtime(&circle_id).expect("launcher cleanup should succeed");
    }

    #[test]
    fn probe_reports_missing_handle_when_circle_is_unmanaged() {
        let circle_id = unique_circle_id("probe-missing");
        let probe = probe_local_command_runtime(&circle_id).expect("probe should succeed");

        let TransportRuntimeProcessProbe { detail } =
            probe.expect("missing handle should surface a process probe");
        assert!(detail.contains("is not registered in this app session"));
    }

    #[test]
    fn probe_returns_exit_detail_after_process_finishes() {
        let circle_id = unique_circle_id("probe-exit");
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let profile = profile(
            &circle_id,
            Some(current_executable.to_string_lossy().into_owned()),
            vec!["--help".into()],
        );
        let launch_attempt = launch_local_command_runtime(&profile);

        assert!(matches!(
            launch_attempt.result,
            TransportRuntimeLaunchResult::Spawned
        ));

        let probe = loop {
            if let Some(probe) =
                probe_local_command_runtime(&circle_id).expect("probe should succeed")
            {
                break probe;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };

        let TransportRuntimeProcessProbe { detail } = probe;
        assert!(detail.contains("exited with status"));
        let followup_probe = probe_local_command_runtime(&circle_id).expect("probe should succeed");
        let TransportRuntimeProcessProbe { detail } =
            followup_probe.expect("released handle should report missing registration");
        assert!(detail.contains("is not registered in this app session"));
    }

    #[test]
    fn enqueue_runtime_input_writes_json_line_queue() {
        let circle_id = unique_circle_id("queue");
        enqueue_local_command_runtime_input(
            &circle_id,
            &TransportRuntimeInputEvent::ApplyCircleAction(TransportRuntimeActionRequest {
                request_id: "sync-sessions:circle-1:1".into(),
                circle_id: circle_id.clone(),
                action: TransportCircleAction::SyncSessions,
                background: false,
                primary_session_id: Some("session-1".into()),
                session_ids: vec!["session-1".into()],
                unread_session_ids: vec!["session-1".into()],
                peer_count: 2,
                session_sync_count: 1,
                sync_since_created_at: None,
                relay_sync_filters: Vec::new(),
                outbound_messages: Vec::new(),
                outbound_media_messages: Vec::new(),
            }),
        )
        .expect("queue event should be written");

        let queue_path = local_command_runtime_input_path(&circle_id);
        let contents = std::fs::read_to_string(&queue_path).expect("queue file should be readable");

        assert!(contents.contains("\"kind\":\"applyCircleAction\""));
        assert!(contents.contains("\"action\":\"syncSessions\""));
        assert!(contents.contains("\"requestId\":\"sync-sessions:circle-1:1\""));
        assert!(contents.contains("\"primarySessionId\":\"session-1\""));
        assert!(contents.contains("\"peerCount\":2"));

        reset_local_command_runtime_input_queue(&circle_id)
            .expect("queue file should be cleaned up");
    }
}
