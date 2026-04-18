use crate::domain::transport::{
    TransportCircleAction, TransportRuntimeAdapterKind, TransportRuntimeDesiredState,
    TransportRuntimeLaunchResult, TransportRuntimeLaunchStatus, TransportRuntimeQueueState,
    TransportRuntimeRecoveryPolicy, TransportRuntimeRegistryEntry, TransportRuntimeSession,
    TransportRuntimeState,
};

#[derive(Debug, Clone, Copy)]
pub struct TransportRuntimeLabels {
    pub inactive_event: &'static str,
    pub starting_event: &'static str,
    pub active_event: &'static str,
    pub connect_event: &'static str,
    pub disconnect_event: &'static str,
    pub sync_event: &'static str,
    pub discover_event: &'static str,
    pub sync_sessions_event: &'static str,
}

#[derive(Debug, Clone)]
pub struct TransportRuntimeProfile {
    pub circle_id: String,
    pub driver: String,
    pub adapter_kind: TransportRuntimeAdapterKind,
    pub launch_status: TransportRuntimeLaunchStatus,
    pub launch_command: Option<String>,
    pub launch_arguments: Vec<String>,
    pub resolved_launch_command: Option<String>,
    pub launch_error: Option<String>,
    pub recovery_policy: TransportRuntimeRecoveryPolicy,
    pub state: TransportRuntimeState,
    pub session_label: String,
    pub endpoint: String,
    pub labels: TransportRuntimeLabels,
}

pub enum TransportRuntimeTrigger<'a> {
    Hydrate,
    Action(&'a TransportCircleAction),
}

#[derive(Debug, Clone)]
pub struct TransportRuntimeLaunchAttempt {
    pub result: TransportRuntimeLaunchResult,
    pub pid: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRuntimeProcessProbe {
    pub detail: String,
}

struct TransportRuntimeRecoveryQueue {
    queue_state: TransportRuntimeQueueState,
    restart_attempts: u32,
    next_retry_in: Option<String>,
    next_retry_at_ms: Option<u64>,
}

struct TransportRuntimeFailureStatus {
    last_failure_reason: Option<String>,
    last_failure_at: Option<String>,
}

pub fn apply_runtime_registry_transition(
    previous: Option<&TransportRuntimeRegistryEntry>,
    profile: TransportRuntimeProfile,
    trigger: TransportRuntimeTrigger<'_>,
    launch_attempt: Option<&TransportRuntimeLaunchAttempt>,
    process_probe: Option<&TransportRuntimeProcessProbe>,
    now_ms: u64,
) -> TransportRuntimeRegistryEntry {
    let effective_state = effective_runtime_state(&profile.state, launch_attempt, process_probe);
    let identity_changed = previous
        .map(|item| {
            item.driver != profile.driver
                || item.session_label != profile.session_label
                || item.endpoint != profile.endpoint
        })
        .unwrap_or(false);
    let state_changed = previous
        .map(|item| item.state != effective_state)
        .unwrap_or(false);
    let boot_started = matches!(
        trigger,
        TransportRuntimeTrigger::Action(TransportCircleAction::Connect)
    ) || previous
        .map(|item| !runtime_is_live(&item.state) && runtime_is_live(&effective_state))
        .unwrap_or(runtime_is_live(&effective_state))
        || (identity_changed && runtime_is_live(&effective_state));
    let generation = runtime_generation(previous, &effective_state, boot_started);
    let desired_state = desired_state(previous, &effective_state, &trigger);
    let recovery_policy = previous
        .map(|item| item.recovery_policy.clone())
        .filter(|_| !identity_changed)
        .unwrap_or(profile.recovery_policy.clone());
    let recovery_queue = recovery_queue(
        previous,
        &effective_state,
        &desired_state,
        &recovery_policy,
        identity_changed,
        launch_attempt,
        process_probe,
        now_ms,
    );
    let failure_status = runtime_failure_status(
        previous,
        &profile,
        &effective_state,
        &desired_state,
        identity_changed,
        launch_attempt,
        process_probe,
    );
    let launch_error = launch_error(&profile, launch_attempt);
    let stable_previous = previous.filter(|_| !identity_changed);
    let (last_launch_result, last_launch_pid, last_launch_at) = match launch_attempt {
        Some(launch_attempt) => (
            Some(launch_attempt.result.clone()),
            launch_attempt.pid,
            Some("now".into()),
        ),
        None => (
            stable_previous.and_then(|item| item.last_launch_result.clone()),
            stable_previous.and_then(|item| item.last_launch_pid),
            stable_previous.and_then(|item| item.last_launch_at.clone()),
        ),
    };
    let state_since = if identity_changed || state_changed {
        "now".into()
    } else if let Some(previous) = previous {
        previous.state_since.clone()
    } else {
        initial_state_since(&effective_state).into()
    };
    let (last_event, last_event_at) = if let Some(launch_attempt) = launch_attempt {
        (
            launch_event_label(&profile.driver, &launch_attempt.result),
            "now".into(),
        )
    } else if let Some(process_probe) = process_probe {
        (
            process_event_label(&profile.driver, process_probe).into(),
            "now".into(),
        )
    } else {
        match trigger {
            TransportRuntimeTrigger::Action(action) => (
                action_event_label(action, &profile.labels).into(),
                "now".into(),
            ),
            TransportRuntimeTrigger::Hydrate if identity_changed || state_changed => (
                state_event_label(&effective_state, &profile.labels).into(),
                "now".into(),
            ),
            TransportRuntimeTrigger::Hydrate => {
                if let Some(previous) = previous {
                    (previous.last_event.clone(), previous.last_event_at.clone())
                } else {
                    (
                        state_event_label(&effective_state, &profile.labels).into(),
                        initial_state_since(&effective_state).into(),
                    )
                }
            }
        }
    };

    TransportRuntimeRegistryEntry {
        circle_id: profile.circle_id,
        driver: profile.driver,
        adapter_kind: profile.adapter_kind,
        launch_status: profile.launch_status,
        launch_command: profile.launch_command,
        launch_arguments: profile.launch_arguments,
        resolved_launch_command: profile.resolved_launch_command,
        launch_error,
        last_launch_result,
        last_launch_pid,
        last_launch_at,
        desired_state,
        recovery_policy,
        queue_state: recovery_queue.queue_state,
        restart_attempts: recovery_queue.restart_attempts,
        next_retry_in: recovery_queue.next_retry_in,
        next_retry_at_ms: recovery_queue.next_retry_at_ms,
        last_failure_reason: failure_status.last_failure_reason,
        last_failure_at: failure_status.last_failure_at,
        state: effective_state,
        generation,
        state_since,
        session_label: profile.session_label,
        endpoint: profile.endpoint,
        last_event,
        last_event_at,
    }
}

pub fn project_runtime_session(entry: &TransportRuntimeRegistryEntry) -> TransportRuntimeSession {
    TransportRuntimeSession {
        circle_id: entry.circle_id.clone(),
        driver: entry.driver.clone(),
        adapter_kind: entry.adapter_kind.clone(),
        launch_status: entry.launch_status.clone(),
        launch_command: entry.launch_command.clone(),
        launch_arguments: entry.launch_arguments.clone(),
        resolved_launch_command: entry.resolved_launch_command.clone(),
        launch_error: entry.launch_error.clone(),
        last_launch_result: entry.last_launch_result.clone(),
        last_launch_pid: entry.last_launch_pid,
        last_launch_at: entry.last_launch_at.clone(),
        desired_state: entry.desired_state.clone(),
        recovery_policy: entry.recovery_policy.clone(),
        queue_state: entry.queue_state.clone(),
        restart_attempts: entry.restart_attempts,
        next_retry_in: entry.next_retry_in.clone(),
        next_retry_at_ms: entry.next_retry_at_ms,
        last_failure_reason: entry.last_failure_reason.clone(),
        last_failure_at: entry.last_failure_at.clone(),
        state: entry.state.clone(),
        generation: entry.generation,
        state_since: entry.state_since.clone(),
        session_label: entry.session_label.clone(),
        endpoint: entry.endpoint.clone(),
        last_event: entry.last_event.clone(),
        last_event_at: entry.last_event_at.clone(),
    }
}

pub fn runtime_registry_entry_from_session(
    session: &TransportRuntimeSession,
) -> TransportRuntimeRegistryEntry {
    TransportRuntimeRegistryEntry {
        circle_id: session.circle_id.clone(),
        driver: session.driver.clone(),
        adapter_kind: session.adapter_kind.clone(),
        launch_status: session.launch_status.clone(),
        launch_command: session.launch_command.clone(),
        launch_arguments: session.launch_arguments.clone(),
        resolved_launch_command: session.resolved_launch_command.clone(),
        launch_error: session.launch_error.clone(),
        last_launch_result: session.last_launch_result.clone(),
        last_launch_pid: session.last_launch_pid,
        last_launch_at: session.last_launch_at.clone(),
        desired_state: session.desired_state.clone(),
        recovery_policy: session.recovery_policy.clone(),
        queue_state: session.queue_state.clone(),
        restart_attempts: session.restart_attempts,
        next_retry_in: session.next_retry_in.clone(),
        next_retry_at_ms: session.next_retry_at_ms,
        last_failure_reason: session.last_failure_reason.clone(),
        last_failure_at: session.last_failure_at.clone(),
        state: session.state.clone(),
        generation: session.generation,
        state_since: session.state_since.clone(),
        session_label: session.session_label.clone(),
        endpoint: session.endpoint.clone(),
        last_event: session.last_event.clone(),
        last_event_at: session.last_event_at.clone(),
    }
}

pub fn infer_runtime_registry_entry_from_legacy_session(
    session: TransportRuntimeSession,
) -> TransportRuntimeRegistryEntry {
    let desired_state = if runtime_is_live(&session.state) {
        TransportRuntimeDesiredState::Running
    } else {
        TransportRuntimeDesiredState::Stopped
    };
    let adapter_kind = if session.driver.contains("native-preview") {
        TransportRuntimeAdapterKind::LocalCommand
    } else {
        TransportRuntimeAdapterKind::Embedded
    };
    let launch_status = match adapter_kind {
        TransportRuntimeAdapterKind::Embedded => TransportRuntimeLaunchStatus::Embedded,
        TransportRuntimeAdapterKind::LocalCommand => TransportRuntimeLaunchStatus::Unknown,
    };
    let recovery_policy = if session.driver.contains("native") {
        TransportRuntimeRecoveryPolicy::Auto
    } else {
        TransportRuntimeRecoveryPolicy::Manual
    };
    let recovery_queue = recovery_queue(
        None,
        &session.state,
        &desired_state,
        &recovery_policy,
        false,
        None,
        None,
        0,
    );

    TransportRuntimeRegistryEntry {
        circle_id: session.circle_id,
        driver: session.driver.clone(),
        adapter_kind,
        launch_status,
        launch_command: session.launch_command,
        launch_arguments: session.launch_arguments,
        resolved_launch_command: session.resolved_launch_command,
        launch_error: session.launch_error,
        last_launch_result: session.last_launch_result,
        last_launch_pid: session.last_launch_pid,
        last_launch_at: session.last_launch_at,
        desired_state,
        recovery_policy,
        queue_state: recovery_queue.queue_state,
        restart_attempts: recovery_queue.restart_attempts,
        next_retry_in: recovery_queue.next_retry_in,
        next_retry_at_ms: recovery_queue.next_retry_at_ms,
        last_failure_reason: session.last_failure_reason,
        last_failure_at: session.last_failure_at,
        state: session.state,
        generation: session.generation,
        state_since: session.state_since,
        session_label: session.session_label,
        endpoint: session.endpoint,
        last_event: session.last_event,
        last_event_at: session.last_event_at,
    }
}

fn effective_runtime_state(
    state: &TransportRuntimeState,
    launch_attempt: Option<&TransportRuntimeLaunchAttempt>,
    process_probe: Option<&TransportRuntimeProcessProbe>,
) -> TransportRuntimeState {
    if launch_attempt_failed(launch_attempt) || process_probe.is_some() {
        TransportRuntimeState::Inactive
    } else {
        state.clone()
    }
}

fn launch_attempt_failed(launch_attempt: Option<&TransportRuntimeLaunchAttempt>) -> bool {
    matches!(
        launch_attempt.map(|attempt| &attempt.result),
        Some(TransportRuntimeLaunchResult::Failed)
    )
}

fn launch_error(
    profile: &TransportRuntimeProfile,
    launch_attempt: Option<&TransportRuntimeLaunchAttempt>,
) -> Option<String> {
    if launch_attempt_failed(launch_attempt) {
        launch_attempt.and_then(|attempt| attempt.detail.clone())
    } else {
        profile.launch_error.clone()
    }
}

fn runtime_is_live(state: &TransportRuntimeState) -> bool {
    matches!(
        state,
        TransportRuntimeState::Starting | TransportRuntimeState::Active
    )
}

fn runtime_generation(
    previous: Option<&TransportRuntimeRegistryEntry>,
    state: &TransportRuntimeState,
    boot_started: bool,
) -> u32 {
    match previous {
        Some(previous) if boot_started => {
            if previous.generation == 0 {
                1
            } else {
                previous.generation.saturating_add(1)
            }
        }
        Some(previous) if runtime_is_live(state) => previous.generation.max(1),
        Some(previous) => previous.generation,
        None if runtime_is_live(state) => 1,
        None => 0,
    }
}

fn desired_state(
    previous: Option<&TransportRuntimeRegistryEntry>,
    state: &TransportRuntimeState,
    trigger: &TransportRuntimeTrigger<'_>,
) -> TransportRuntimeDesiredState {
    match trigger {
        TransportRuntimeTrigger::Action(TransportCircleAction::Disconnect) => {
            TransportRuntimeDesiredState::Stopped
        }
        TransportRuntimeTrigger::Action(_) => TransportRuntimeDesiredState::Running,
        TransportRuntimeTrigger::Hydrate => {
            if runtime_is_live(state) {
                return TransportRuntimeDesiredState::Running;
            }

            previous
                .map(|item| item.desired_state.clone())
                .unwrap_or(TransportRuntimeDesiredState::Stopped)
        }
    }
}

fn recovery_queue(
    previous: Option<&TransportRuntimeRegistryEntry>,
    state: &TransportRuntimeState,
    desired_state: &TransportRuntimeDesiredState,
    recovery_policy: &TransportRuntimeRecoveryPolicy,
    identity_changed: bool,
    launch_attempt: Option<&TransportRuntimeLaunchAttempt>,
    process_probe: Option<&TransportRuntimeProcessProbe>,
    now_ms: u64,
) -> TransportRuntimeRecoveryQueue {
    if matches!(desired_state, TransportRuntimeDesiredState::Stopped)
        || runtime_is_live(state)
        || matches!(recovery_policy, TransportRuntimeRecoveryPolicy::Manual)
    {
        return TransportRuntimeRecoveryQueue {
            queue_state: TransportRuntimeQueueState::Idle,
            restart_attempts: 0,
            next_retry_in: None,
            next_retry_at_ms: None,
        };
    }

    let previous = previous.filter(|_| !identity_changed);
    let previous_attempts = previous.map(|item| item.restart_attempts).unwrap_or(0);
    let failure_detected = launch_attempt_failed(launch_attempt)
        || process_probe.is_some()
        || previous
            .map(|item| runtime_is_live(&item.state))
            .unwrap_or(false);
    let restart_attempts = if failure_detected {
        previous_attempts.saturating_add(1).max(1)
    } else {
        previous_attempts
    };
    let next_retry_at_ms = if failure_detected {
        Some(now_ms.saturating_add(retry_backoff_delay_ms(restart_attempts)))
    } else if let Some(next_retry_at_ms) = previous.and_then(|item| item.next_retry_at_ms) {
        Some(next_retry_at_ms)
    } else if restart_attempts > 0 {
        Some(now_ms.saturating_add(retry_backoff_delay_ms(restart_attempts)))
    } else {
        Some(now_ms)
    };
    let (queue_state, next_retry_in) =
        retry_queue_state_and_label(now_ms, next_retry_at_ms, restart_attempts);

    TransportRuntimeRecoveryQueue {
        queue_state,
        restart_attempts,
        next_retry_in,
        next_retry_at_ms,
    }
}

fn runtime_failure_status(
    previous: Option<&TransportRuntimeRegistryEntry>,
    profile: &TransportRuntimeProfile,
    state: &TransportRuntimeState,
    desired_state: &TransportRuntimeDesiredState,
    identity_changed: bool,
    launch_attempt: Option<&TransportRuntimeLaunchAttempt>,
    process_probe: Option<&TransportRuntimeProcessProbe>,
) -> TransportRuntimeFailureStatus {
    let previous = previous.filter(|_| !identity_changed);

    if matches!(desired_state, TransportRuntimeDesiredState::Stopped) || runtime_is_live(state) {
        return TransportRuntimeFailureStatus {
            last_failure_reason: None,
            last_failure_at: None,
        };
    }

    if launch_attempt_failed(launch_attempt) {
        return TransportRuntimeFailureStatus {
            last_failure_reason: launch_attempt.and_then(|attempt| attempt.detail.clone()),
            last_failure_at: Some("now".into()),
        };
    }

    if let Some(process_probe) = process_probe {
        return TransportRuntimeFailureStatus {
            last_failure_reason: Some(process_probe.detail.clone()),
            last_failure_at: Some("now".into()),
        };
    }

    let failure_detected = previous
        .map(|item| runtime_is_live(&item.state))
        .unwrap_or(false);
    if failure_detected {
        return TransportRuntimeFailureStatus {
            last_failure_reason: previous
                .map(|item| runtime_failure_reason(&profile.driver, &item.state)),
            last_failure_at: Some("now".into()),
        };
    }

    TransportRuntimeFailureStatus {
        last_failure_reason: previous.and_then(|item| item.last_failure_reason.clone()),
        last_failure_at: previous.and_then(|item| item.last_failure_at.clone()),
    }
}

fn process_event_label(
    driver: &str,
    _process_probe: &TransportRuntimeProcessProbe,
) -> &'static str {
    if driver.contains("native") {
        "native runtime process exited"
    } else {
        "local runtime process exited"
    }
}

fn runtime_failure_reason(driver: &str, previous_state: &TransportRuntimeState) -> String {
    let native_driver = driver.contains("native");
    match (native_driver, previous_state) {
        (true, TransportRuntimeState::Starting) => {
            "native preview runtime dropped during startup".into()
        }
        (true, _) => "native preview runtime heartbeat expired".into(),
        (false, TransportRuntimeState::Starting) => "local runtime dropped during startup".into(),
        (false, _) => "local runtime heartbeat expired".into(),
    }
}

fn launch_event_label(driver: &str, result: &TransportRuntimeLaunchResult) -> String {
    let native_driver = driver.contains("native");
    match (native_driver, result) {
        (true, TransportRuntimeLaunchResult::Spawned) => "native runtime launch spawned".into(),
        (true, TransportRuntimeLaunchResult::Reused) => "native runtime launch reused".into(),
        (true, TransportRuntimeLaunchResult::Failed) => "native runtime launch failed".into(),
        (false, TransportRuntimeLaunchResult::Spawned) => "local runtime launch spawned".into(),
        (false, TransportRuntimeLaunchResult::Reused) => "local runtime launch reused".into(),
        (false, TransportRuntimeLaunchResult::Failed) => "local runtime launch failed".into(),
    }
}

fn retry_backoff_delay_ms(restart_attempts: u32) -> u64 {
    match restart_attempts {
        0 | 1 => 3_000,
        2 => 10_000,
        3 => 30_000,
        _ => 60_000,
    }
}

fn retry_queue_state_and_label(
    now_ms: u64,
    next_retry_at_ms: Option<u64>,
    restart_attempts: u32,
) -> (TransportRuntimeQueueState, Option<String>) {
    match next_retry_at_ms {
        Some(next_retry_at_ms) if next_retry_at_ms > now_ms => (
            TransportRuntimeQueueState::Backoff,
            Some(retry_backoff_label(next_retry_at_ms.saturating_sub(now_ms))),
        ),
        Some(_) => (
            TransportRuntimeQueueState::Queued,
            Some("when local runtime worker is ready".into()),
        ),
        None if restart_attempts == 0 => (TransportRuntimeQueueState::Queued, None),
        None => (
            TransportRuntimeQueueState::Backoff,
            Some(retry_backoff_label(retry_backoff_delay_ms(
                restart_attempts,
            ))),
        ),
    }
}

fn retry_backoff_label(remaining_ms: u64) -> String {
    let remaining_seconds = remaining_ms.div_ceil(1_000);
    match remaining_seconds {
        0 | 1 => "in 1s".into(),
        seconds if seconds < 60 => format!("in {seconds}s"),
        seconds => format!("in {}m", seconds.div_ceil(60)),
    }
}

fn state_event_label(
    state: &TransportRuntimeState,
    labels: &TransportRuntimeLabels,
) -> &'static str {
    match state {
        TransportRuntimeState::Inactive => labels.inactive_event,
        TransportRuntimeState::Starting => labels.starting_event,
        TransportRuntimeState::Active => labels.active_event,
    }
}

fn action_event_label(
    action: &TransportCircleAction,
    labels: &TransportRuntimeLabels,
) -> &'static str {
    match action {
        TransportCircleAction::Connect => labels.connect_event,
        TransportCircleAction::Disconnect => labels.disconnect_event,
        TransportCircleAction::Sync => labels.sync_event,
        TransportCircleAction::DiscoverPeers => labels.discover_event,
        TransportCircleAction::SyncSessions => labels.sync_sessions_event,
    }
}

fn initial_state_since(state: &TransportRuntimeState) -> &'static str {
    if runtime_is_live(state) {
        "this launch"
    } else {
        "not started"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_labels() -> TransportRuntimeLabels {
        TransportRuntimeLabels {
            inactive_event: "mock runtime idle",
            starting_event: "mock runtime booting",
            active_event: "mock runtime active",
            connect_event: "mock runtime handshake enqueued",
            disconnect_event: "mock runtime released",
            sync_event: "mock relay checkpoint synced",
            discover_event: "mock peer sweep queued",
            sync_sessions_event: "mock session merge queued",
        }
    }

    fn profile(state: TransportRuntimeState) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            circle_id: "circle-1".into(),
            driver: "local-mock-mesh-daemon".into(),
            adapter_kind: TransportRuntimeAdapterKind::Embedded,
            launch_status: TransportRuntimeLaunchStatus::Embedded,
            launch_command: None,
            launch_arguments: Vec::new(),
            resolved_launch_command: None,
            launch_error: None,
            recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
            state,
            session_label: "mock::mesh::circle-1".into(),
            endpoint: "loopback://mesh/circle-1".into(),
            labels: mock_labels(),
        }
    }

    fn previous_entry(
        state: TransportRuntimeState,
        generation: u32,
        desired_state: TransportRuntimeDesiredState,
        recovery_policy: TransportRuntimeRecoveryPolicy,
        queue_state: TransportRuntimeQueueState,
        restart_attempts: u32,
        next_retry_in: Option<&str>,
        next_retry_at_ms: Option<u64>,
        last_failure_reason: Option<&str>,
        last_failure_at: Option<&str>,
    ) -> TransportRuntimeRegistryEntry {
        TransportRuntimeRegistryEntry {
            circle_id: "circle-1".into(),
            driver: "local-mock-mesh-daemon".into(),
            adapter_kind: TransportRuntimeAdapterKind::Embedded,
            launch_status: TransportRuntimeLaunchStatus::Embedded,
            launch_command: None,
            launch_arguments: Vec::new(),
            resolved_launch_command: None,
            launch_error: None,
            last_launch_result: None,
            last_launch_pid: None,
            last_launch_at: None,
            desired_state,
            recovery_policy,
            queue_state,
            restart_attempts,
            next_retry_in: next_retry_in.map(str::to_owned),
            next_retry_at_ms,
            last_failure_reason: last_failure_reason.map(str::to_owned),
            last_failure_at: last_failure_at.map(str::to_owned),
            state,
            generation,
            state_since: "now".into(),
            session_label: "mock::mesh::circle-1".into(),
            endpoint: "loopback://mesh/circle-1".into(),
            last_event: "mock relay checkpoint synced".into(),
            last_event_at: "now".into(),
        }
    }

    #[test]
    fn preserves_lifecycle_for_stable_active_session() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Active,
                3,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Manual,
                TransportRuntimeQueueState::Idle,
                0,
                None,
                None,
                None,
                None,
            )),
            profile(TransportRuntimeState::Active),
            TransportRuntimeTrigger::Hydrate,
            None,
            None,
            10_000,
        );

        assert_eq!(session.generation, 3);
        assert_eq!(session.state_since, "now");
        assert_eq!(session.last_event, "mock relay checkpoint synced");
        assert_eq!(session.last_event_at, "now");
        assert!(matches!(
            session.desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(session.restart_attempts, 0);
        assert_eq!(session.next_retry_in, None);
        assert_eq!(session.next_retry_at_ms, None);
        assert_eq!(session.last_failure_reason, None);
        assert_eq!(session.last_failure_at, None);
    }

    #[test]
    fn increments_generation_when_connect_boots_again() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Inactive,
                1,
                TransportRuntimeDesiredState::Stopped,
                TransportRuntimeRecoveryPolicy::Manual,
                TransportRuntimeQueueState::Idle,
                0,
                None,
                None,
                None,
                None,
            )),
            profile(TransportRuntimeState::Starting),
            TransportRuntimeTrigger::Action(&TransportCircleAction::Connect),
            None,
            None,
            10_000,
        );

        assert_eq!(session.generation, 2);
        assert!(matches!(session.state, TransportRuntimeState::Starting));
        assert_eq!(session.state_since, "now");
        assert_eq!(session.last_event, "mock runtime handshake enqueued");
        assert_eq!(session.last_event_at, "now");
        assert!(matches!(
            session.desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(session.restart_attempts, 0);
        assert_eq!(session.next_retry_in, None);
        assert_eq!(session.next_retry_at_ms, None);
        assert_eq!(session.last_failure_reason, None);
        assert_eq!(session.last_failure_at, None);
    }

    #[test]
    fn disconnect_keeps_generation_but_resets_state_window() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Active,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Manual,
                TransportRuntimeQueueState::Idle,
                0,
                None,
                None,
                Some("local runtime heartbeat expired"),
                Some("now"),
            )),
            profile(TransportRuntimeState::Inactive),
            TransportRuntimeTrigger::Action(&TransportCircleAction::Disconnect),
            None,
            None,
            10_000,
        );

        assert_eq!(session.generation, 4);
        assert!(matches!(session.state, TransportRuntimeState::Inactive));
        assert_eq!(session.state_since, "now");
        assert_eq!(session.last_event, "mock runtime released");
        assert_eq!(session.last_event_at, "now");
        assert!(matches!(
            session.desired_state,
            TransportRuntimeDesiredState::Stopped
        ));
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(session.restart_attempts, 0);
        assert_eq!(session.next_retry_in, None);
        assert_eq!(session.next_retry_at_ms, None);
        assert_eq!(session.last_failure_reason, None);
        assert_eq!(session.last_failure_at, None);
    }

    #[test]
    fn hydrate_keeps_running_desire_for_inactive_runtime() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Active,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Manual,
                TransportRuntimeQueueState::Idle,
                0,
                None,
                None,
                None,
                None,
            )),
            profile(TransportRuntimeState::Inactive),
            TransportRuntimeTrigger::Hydrate,
            None,
            None,
            10_000,
        );

        assert!(matches!(session.state, TransportRuntimeState::Inactive));
        assert!(matches!(
            session.desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(session.restart_attempts, 0);
        assert_eq!(session.next_retry_in, None);
        assert_eq!(session.next_retry_at_ms, None);
        assert_eq!(
            session.last_failure_reason.as_deref(),
            Some("local runtime heartbeat expired")
        );
        assert_eq!(session.last_failure_at.as_deref(), Some("now"));
    }

    #[test]
    fn hydrate_marks_auto_recovery_as_backoff_after_runtime_drop() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Active,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Auto,
                TransportRuntimeQueueState::Idle,
                0,
                None,
                None,
                None,
                None,
            )),
            TransportRuntimeProfile {
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                ..profile(TransportRuntimeState::Inactive)
            },
            TransportRuntimeTrigger::Hydrate,
            None,
            None,
            10_000,
        );

        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Backoff
        ));
        assert_eq!(session.restart_attempts, 1);
        assert_eq!(session.next_retry_in.as_deref(), Some("in 3s"));
        assert_eq!(session.next_retry_at_ms, Some(13_000));
        assert_eq!(
            session.last_failure_reason.as_deref(),
            Some("local runtime heartbeat expired")
        );
        assert_eq!(session.last_failure_at.as_deref(), Some("now"));
    }

    #[test]
    fn hydrate_keeps_auto_recovery_queue_stable_while_waiting() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Inactive,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Auto,
                TransportRuntimeQueueState::Backoff,
                2,
                Some("in 10s"),
                Some(20_000),
                Some("local runtime heartbeat expired"),
                Some("now"),
            )),
            TransportRuntimeProfile {
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                ..profile(TransportRuntimeState::Inactive)
            },
            TransportRuntimeTrigger::Hydrate,
            None,
            None,
            12_000,
        );

        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Backoff
        ));
        assert_eq!(session.restart_attempts, 2);
        assert_eq!(session.next_retry_in.as_deref(), Some("in 8s"));
        assert_eq!(session.next_retry_at_ms, Some(20_000));
        assert_eq!(
            session.last_failure_reason.as_deref(),
            Some("local runtime heartbeat expired")
        );
        assert_eq!(session.last_failure_at.as_deref(), Some("now"));
    }

    #[test]
    fn hydrate_promotes_expired_backoff_to_queued() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Inactive,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Auto,
                TransportRuntimeQueueState::Backoff,
                2,
                Some("in 10s"),
                Some(20_000),
                Some("local runtime heartbeat expired"),
                Some("now"),
            )),
            TransportRuntimeProfile {
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                ..profile(TransportRuntimeState::Inactive)
            },
            TransportRuntimeTrigger::Hydrate,
            None,
            None,
            20_001,
        );

        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Queued
        ));
        assert_eq!(session.restart_attempts, 2);
        assert_eq!(
            session.next_retry_in.as_deref(),
            Some("when local runtime worker is ready")
        );
        assert_eq!(session.next_retry_at_ms, Some(20_000));
        assert_eq!(
            session.last_failure_reason.as_deref(),
            Some("local runtime heartbeat expired")
        );
        assert_eq!(session.last_failure_at.as_deref(), Some("now"));
    }

    #[test]
    fn legacy_inactive_native_runtime_keeps_auto_policy_without_queue() {
        let session = infer_runtime_registry_entry_from_legacy_session(TransportRuntimeSession {
            circle_id: "circle-1".into(),
            driver: "native-preview-mesh-runtime".into(),
            adapter_kind: TransportRuntimeAdapterKind::Embedded,
            launch_status: TransportRuntimeLaunchStatus::Unknown,
            launch_command: None,
            launch_arguments: Vec::new(),
            resolved_launch_command: None,
            launch_error: None,
            last_launch_result: None,
            last_launch_pid: None,
            last_launch_at: None,
            desired_state: TransportRuntimeDesiredState::Stopped,
            recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
            queue_state: TransportRuntimeQueueState::Idle,
            restart_attempts: 0,
            next_retry_in: None,
            next_retry_at_ms: None,
            last_failure_reason: None,
            last_failure_at: None,
            state: TransportRuntimeState::Inactive,
            generation: 1,
            state_since: "not started".into(),
            session_label: "native::mesh::circle-1".into(),
            endpoint: "native://mesh/circle-1".into(),
            last_event: "native runtime idle".into(),
            last_event_at: "not started".into(),
        });

        assert!(matches!(
            session.desired_state,
            TransportRuntimeDesiredState::Stopped
        ));
        assert!(matches!(
            session.recovery_policy,
            TransportRuntimeRecoveryPolicy::Auto
        ));
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(session.restart_attempts, 0);
        assert_eq!(session.next_retry_in, None);
        assert_eq!(session.next_retry_at_ms, None);
        assert_eq!(session.last_failure_reason, None);
        assert_eq!(session.last_failure_at, None);
    }

    #[test]
    fn reconnect_clears_failure_details_after_runtime_starts_again() {
        let session = apply_runtime_registry_transition(
            Some(&previous_entry(
                TransportRuntimeState::Inactive,
                4,
                TransportRuntimeDesiredState::Running,
                TransportRuntimeRecoveryPolicy::Auto,
                TransportRuntimeQueueState::Backoff,
                2,
                Some("when local runtime worker is ready"),
                Some(20_000),
                Some("local runtime heartbeat expired"),
                Some("now"),
            )),
            TransportRuntimeProfile {
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                ..profile(TransportRuntimeState::Starting)
            },
            TransportRuntimeTrigger::Action(&TransportCircleAction::Connect),
            None,
            None,
            20_001,
        );

        assert!(matches!(session.state, TransportRuntimeState::Starting));
        assert_eq!(session.last_failure_reason, None);
        assert_eq!(session.last_failure_at, None);
        assert!(matches!(
            session.queue_state,
            TransportRuntimeQueueState::Idle
        ));
    }
}
