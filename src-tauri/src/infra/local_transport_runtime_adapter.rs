use crate::domain::transport::{
    RelayProtocol, TransportRuntimeAdapterKind, TransportRuntimeLaunchStatus,
};
use crate::domain::transport_adapter::TransportRuntimeOptions;
use std::collections::HashSet;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub(crate) enum TransportRuntimeFlavor {
    Mock,
    Preview,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedLocalTransportRuntimeAdapter {
    pub driver: String,
    pub adapter_kind: TransportRuntimeAdapterKind,
    pub launch_status: TransportRuntimeLaunchStatus,
    pub launch_command: Option<String>,
    pub launch_arguments: Vec<String>,
    pub resolved_launch_command: Option<String>,
    pub launch_error: Option<String>,
    pub session_prefix: &'static str,
    pub protocol_token: &'static str,
    pub endpoint_scheme: &'static str,
}

pub(crate) fn resolve_local_transport_runtime_adapter(
    flavor: TransportRuntimeFlavor,
    protocol: &RelayProtocol,
    options: TransportRuntimeOptions,
    circle_id: &str,
    relay_url: Option<&str>,
    preferred_session_id: Option<&str>,
) -> ResolvedLocalTransportRuntimeAdapter {
    match (flavor, protocol) {
        (TransportRuntimeFlavor::Mock, RelayProtocol::Websocket) => {
            embedded_adapter("local-mock-relay-daemon", "mock", "ws", "loopback")
        }
        (TransportRuntimeFlavor::Mock, RelayProtocol::Mesh) => {
            embedded_adapter("local-mock-mesh-daemon", "mock", "mesh", "loopback")
        }
        (TransportRuntimeFlavor::Mock, RelayProtocol::Invite) => {
            embedded_adapter("local-mock-invite-daemon", "mock", "invite", "loopback")
        }
        (TransportRuntimeFlavor::Preview, RelayProtocol::Websocket) if options.use_tor_network => {
            preview_adapter(
                "native-preview-tor-runtime",
                "native",
                "tor-ws",
                "native+tor",
                "p2p-chat-runtime",
                preview_runtime_arguments(
                    "preview-relay",
                    circle_id,
                    relay_url,
                    Some("--tor"),
                    preferred_session_id,
                ),
            )
        }
        (TransportRuntimeFlavor::Preview, RelayProtocol::Websocket) => preview_adapter(
            "native-preview-relay-runtime",
            "native",
            "ws",
            "native",
            "p2p-chat-runtime",
            preview_runtime_arguments(
                "preview-relay",
                circle_id,
                relay_url,
                None,
                preferred_session_id,
            ),
        ),
        (TransportRuntimeFlavor::Preview, RelayProtocol::Mesh) => preview_adapter(
            "native-preview-mesh-runtime",
            "native",
            "mesh",
            "native",
            "p2p-chat-runtime",
            preview_runtime_arguments("preview-mesh", circle_id, None, None, preferred_session_id),
        ),
        (TransportRuntimeFlavor::Preview, RelayProtocol::Invite) => preview_adapter(
            "native-preview-invite-runtime",
            "native",
            "invite",
            "native",
            "p2p-chat-runtime",
            preview_runtime_arguments(
                "preview-invite",
                circle_id,
                None,
                None,
                preferred_session_id,
            ),
        ),
    }
}

fn preview_adapter(
    driver: &'static str,
    session_prefix: &'static str,
    protocol_token: &'static str,
    endpoint_scheme: &'static str,
    launch_command: &'static str,
    launch_arguments: Vec<String>,
) -> ResolvedLocalTransportRuntimeAdapter {
    if preview_transport_uses_embedded_adapter() {
        return embedded_adapter(driver, session_prefix, protocol_token, endpoint_scheme);
    }

    local_command_adapter(
        driver,
        session_prefix,
        protocol_token,
        endpoint_scheme,
        launch_command,
        launch_arguments,
    )
}

fn preview_transport_uses_embedded_adapter() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

fn preview_runtime_arguments(
    command: &str,
    circle_id: &str,
    relay_url: Option<&str>,
    extra_flag: Option<&str>,
    preferred_session_id: Option<&str>,
) -> Vec<String> {
    let mut arguments = vec![command.into()];
    if let Some(extra_flag) = extra_flag {
        arguments.push(extra_flag.into());
    }
    if let Some(relay_url) = relay_url {
        arguments.push("--relay-url".into());
        arguments.push(relay_url.into());
    }
    arguments.push("--circle".into());
    arguments.push(circle_id.into());
    if let Some(session_id) = preferred_session_id {
        arguments.push("--session".into());
        arguments.push(session_id.into());
    }
    arguments
}

fn embedded_adapter(
    driver: &'static str,
    session_prefix: &'static str,
    protocol_token: &'static str,
    endpoint_scheme: &'static str,
) -> ResolvedLocalTransportRuntimeAdapter {
    ResolvedLocalTransportRuntimeAdapter {
        driver: driver.into(),
        adapter_kind: TransportRuntimeAdapterKind::Embedded,
        launch_status: TransportRuntimeLaunchStatus::Embedded,
        launch_command: None,
        launch_arguments: Vec::new(),
        resolved_launch_command: None,
        launch_error: None,
        session_prefix,
        protocol_token,
        endpoint_scheme,
    }
}

fn local_command_adapter(
    driver: &'static str,
    session_prefix: &'static str,
    protocol_token: &'static str,
    endpoint_scheme: &'static str,
    launch_command: &'static str,
    launch_arguments: Vec<String>,
) -> ResolvedLocalTransportRuntimeAdapter {
    let (launch_status, resolved_launch_command, launch_error) =
        probe_local_command(launch_command);

    ResolvedLocalTransportRuntimeAdapter {
        driver: driver.into(),
        adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
        launch_status,
        launch_command: Some(launch_command.into()),
        launch_arguments,
        resolved_launch_command,
        launch_error,
        session_prefix,
        protocol_token,
        endpoint_scheme,
    }
}

fn probe_local_command(
    command: &str,
) -> (TransportRuntimeLaunchStatus, Option<String>, Option<String>) {
    if let Some(path) = resolve_command_path(command) {
        return (
            TransportRuntimeLaunchStatus::Ready,
            Some(path.to_string_lossy().into_owned()),
            None,
        );
    }

    (
        TransportRuntimeLaunchStatus::Missing,
        None,
        Some(format!(
            "command `{command}` is not available on PATH or local build outputs"
        )),
    )
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return command_path_exists(command_path).then(|| command_path.to_path_buf());
    }

    resolve_command_path_on_path(command).or_else(|| {
        resolve_repo_local_command_path(
            command,
            env::current_exe().ok().as_deref(),
            Path::new(env!("CARGO_MANIFEST_DIR")),
        )
    })
}

fn resolve_command_path_on_path(command: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    resolve_command_in_directories(command, env::split_paths(&path))
}

fn resolve_repo_local_command_path(
    command: &str,
    current_executable: Option<&Path>,
    manifest_dir: &Path,
) -> Option<PathBuf> {
    resolve_command_in_directories(
        command,
        repo_local_command_directories(current_executable, manifest_dir),
    )
}

fn resolve_command_in_directories<I>(command: &str, directories: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    directories
        .into_iter()
        .flat_map(|directory| candidate_command_paths(&directory, command))
        .find(|candidate| command_path_exists(candidate))
}

fn repo_local_command_directories(
    current_executable: Option<&Path>,
    manifest_dir: &Path,
) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    let mut seen = HashSet::new();

    if let Some(current_executable) = current_executable {
        if let Some(parent) = current_executable.parent() {
            push_unique_directory(&mut directories, &mut seen, parent.to_path_buf());
            if parent.file_name().is_some_and(|name| name == "deps") {
                if let Some(profile_dir) = parent.parent() {
                    push_unique_directory(&mut directories, &mut seen, profile_dir.to_path_buf());
                }
            }
        }
    }

    push_unique_directory(
        &mut directories,
        &mut seen,
        manifest_dir.join("target").join("debug"),
    );
    push_unique_directory(
        &mut directories,
        &mut seen,
        manifest_dir.join("target").join("release"),
    );

    directories
}

fn push_unique_directory(
    directories: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
    directory: PathBuf,
) {
    if seen.insert(directory.clone()) {
        directories.push(directory);
    }
}

fn candidate_command_paths(directory: &Path, command: &str) -> Vec<PathBuf> {
    let base = directory.join(command);
    if cfg!(windows) {
        if base.extension().is_some() {
            return vec![base];
        }

        let path_ext =
            env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));

        env::split_paths(&path_ext)
            .map(|extension| {
                extension
                    .to_string_lossy()
                    .trim_start_matches('.')
                    .to_string()
            })
            .filter(|extension| !extension.is_empty())
            .map(|extension| base.with_extension(extension))
            .collect()
    } else {
        vec![base]
    }
}

fn command_path_exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_adapter_is_marked_embedded() {
        let adapter = resolve_local_transport_runtime_adapter(
            TransportRuntimeFlavor::Mock,
            &RelayProtocol::Mesh,
            TransportRuntimeOptions {
                use_tor_network: false,
                experimental_transport: false,
            },
            "circle-1",
            None,
            None,
        );

        assert!(matches!(
            adapter.launch_status,
            TransportRuntimeLaunchStatus::Embedded
        ));
        assert_eq!(adapter.resolved_launch_command, None);
        assert_eq!(adapter.launch_error, None);
    }

    #[test]
    fn absolute_command_path_is_marked_ready() {
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let (status, resolved_path, launch_error) =
            probe_local_command(current_executable.to_string_lossy().as_ref());

        assert!(matches!(status, TransportRuntimeLaunchStatus::Ready));
        assert_eq!(
            resolved_path,
            Some(current_executable.to_string_lossy().into_owned())
        );
        assert_eq!(launch_error, None);
    }

    #[test]
    fn missing_command_is_marked_missing() {
        let (status, resolved_path, launch_error) =
            probe_local_command("p2p-chat-runtime-command-that-should-not-exist-anywhere");

        assert!(matches!(status, TransportRuntimeLaunchStatus::Missing));
        assert_eq!(resolved_path, None);
        assert!(launch_error.as_deref().is_some_and(
            |message| message.contains("not available on PATH or local build outputs")
        ));
    }

    #[test]
    fn preview_runtime_arguments_include_session_when_available() {
        let adapter = resolve_local_transport_runtime_adapter(
            TransportRuntimeFlavor::Preview,
            &RelayProtocol::Websocket,
            TransportRuntimeOptions {
                use_tor_network: false,
                experimental_transport: true,
            },
            "main-circle",
            Some("wss://relay.example.com"),
            Some("alice"),
        );

        assert_eq!(
            adapter.launch_arguments,
            vec![
                "preview-relay".to_string(),
                "--relay-url".to_string(),
                "wss://relay.example.com".to_string(),
                "--circle".to_string(),
                "main-circle".to_string(),
                "--session".to_string(),
                "alice".to_string(),
            ]
        );
    }

    #[test]
    fn preview_transport_adapter_uses_platform_safe_launch_strategy() {
        let adapter = resolve_local_transport_runtime_adapter(
            TransportRuntimeFlavor::Preview,
            &RelayProtocol::Websocket,
            TransportRuntimeOptions {
                use_tor_network: false,
                experimental_transport: true,
            },
            "main-circle",
            Some("wss://relay.example.com"),
            Some("alice"),
        );

        if preview_transport_uses_embedded_adapter() {
            assert!(matches!(
                adapter.adapter_kind,
                TransportRuntimeAdapterKind::Embedded
            ));
            assert!(matches!(
                adapter.launch_status,
                TransportRuntimeLaunchStatus::Embedded
            ));
            assert_eq!(adapter.launch_command, None);
            assert_eq!(adapter.resolved_launch_command, None);
            assert_eq!(adapter.launch_error, None);
        } else {
            assert!(matches!(
                adapter.adapter_kind,
                TransportRuntimeAdapterKind::LocalCommand
            ));
            assert_eq!(adapter.launch_command.as_deref(), Some("p2p-chat-runtime"));
        }
    }

    #[test]
    fn repo_local_resolution_checks_profile_directory_above_deps() {
        let temp_root = std::env::temp_dir().join(format!(
            "p2p-chat-runtime-adapter-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let profile_dir = temp_root.join("target").join("debug");
        let deps_dir = profile_dir.join("deps");
        let runtime_path = profile_dir.join(if cfg!(windows) {
            "p2p-chat-runtime.exe"
        } else {
            "p2p-chat-runtime"
        });
        std::fs::create_dir_all(&deps_dir).expect("deps directory should be created");
        std::fs::write(&runtime_path, b"runtime").expect("runtime file should be created");

        let resolved = resolve_repo_local_command_path(
            "p2p-chat-runtime",
            Some(&deps_dir.join("p2p_chat_lib-tests")),
            &temp_root,
        );

        assert_eq!(resolved, Some(runtime_path));
        let _ = std::fs::remove_dir_all(&temp_root);
    }
}
