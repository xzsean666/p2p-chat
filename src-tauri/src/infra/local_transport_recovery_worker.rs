use crate::domain::chat::ChatDomainSeed;
use crate::domain::transport::{
    TransportCircleAction, TransportCircleActionInput, TransportRuntimeAdapterKind,
    TransportRuntimeDesiredState, TransportRuntimeLaunchStatus, TransportRuntimeRecoveryPolicy,
    TransportRuntimeRegistryEntry, TransportRuntimeState,
};
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_repository::TransportCache;
use crate::domain::transport_runtime_registry::runtime_registry_entry_from_session;
use crate::domain::transport_runtime_registry::TransportRuntimeProfile;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct LocalTransportRecoveryAction {
    pub circle_id: String,
    pub action: TransportCircleAction,
}

pub fn collect_local_transport_recovery_actions(
    seed: &ChatDomainSeed,
    previous_cache: &TransportCache,
    runtime_profiles: &[TransportRuntimeProfile],
    experimental_transport: bool,
) -> Vec<LocalTransportRecoveryAction> {
    if !experimental_transport {
        return Vec::new();
    }

    let now_ms = current_time_millis();
    let runtime_registry = if previous_cache.runtime_registry.is_empty() {
        previous_cache
            .runtime_sessions
            .iter()
            .map(runtime_registry_entry_from_session)
            .collect::<Vec<_>>()
    } else {
        previous_cache.runtime_registry.clone()
    };

    seed.circles
        .iter()
        .filter_map(|circle| {
            let runtime = runtime_registry
                .iter()
                .find(|entry| entry.circle_id == circle.id)?;
            let runtime_profile = runtime_profiles
                .iter()
                .find(|entry| entry.circle_id == circle.id);

            if !matches!(runtime.desired_state, TransportRuntimeDesiredState::Running)
                || !matches!(
                    runtime.recovery_policy,
                    TransportRuntimeRecoveryPolicy::Auto
                )
            {
                return None;
            }

            if runtime_profile
                .map(|profile| {
                    matches!(profile.launch_status, TransportRuntimeLaunchStatus::Missing)
                })
                .unwrap_or(false)
            {
                return None;
            }

            let action = if needs_bootstrap_connect(runtime) {
                Some(TransportCircleAction::Connect)
            } else if matches!(runtime.state, TransportRuntimeState::Starting) {
                Some(TransportCircleAction::Sync)
            } else if matches!(runtime.state, TransportRuntimeState::Inactive)
                && runtime
                    .next_retry_at_ms
                    .map(|next_retry_at_ms| next_retry_at_ms <= now_ms)
                    .unwrap_or(false)
            {
                Some(TransportCircleAction::Connect)
            } else {
                None
            }?;

            Some(LocalTransportRecoveryAction {
                circle_id: circle.id.clone(),
                action,
            })
        })
        .collect()
}

fn needs_bootstrap_connect(runtime: &TransportRuntimeRegistryEntry) -> bool {
    matches!(runtime.adapter_kind, TransportRuntimeAdapterKind::LocalCommand)
        && matches!(runtime.launch_status, TransportRuntimeLaunchStatus::Ready)
        && runtime.last_launch_result.is_none()
        && runtime.last_launch_pid.is_none()
        && runtime.last_launch_at.is_none()
}

fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn recovery_action_input(
    action: &LocalTransportRecoveryAction,
    active_circle_id: Option<String>,
    runtime: TransportRuntimeOptions,
) -> TransportCircleActionInput {
    TransportCircleActionInput {
        circle_id: action.circle_id.clone(),
        action: action.action.clone(),
        active_circle_id,
        use_tor_network: runtime.use_tor_network,
        experimental_transport: runtime.experimental_transport,
        sync_since_created_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{ChatDomainSeed, CircleItem, CircleStatus, CircleType};
    use crate::domain::transport::{
        TransportRuntimeAdapterKind, TransportRuntimeDesiredState, TransportRuntimeLaunchResult,
        TransportRuntimeLaunchStatus, TransportRuntimeQueueState,
        TransportRuntimeRecoveryPolicy, TransportRuntimeRegistryEntry, TransportRuntimeSession,
        TransportRuntimeState,
    };
    use crate::domain::transport_repository::TransportCache;
    use crate::domain::transport_runtime_registry::TransportRuntimeLabels;
    use crate::domain::transport_runtime_registry::TransportRuntimeProfile;
    use std::collections::HashMap;

    fn seed(status: CircleStatus) -> ChatDomainSeed {
        ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "wss://relay.example.com".into(),
                circle_type: CircleType::Default,
                status,
                latency: "--".into(),
                description: "test".into(),
            }],
            contacts: Vec::new(),
            sessions: Vec::new(),
            groups: Vec::new(),
            message_store: HashMap::new(),
        }
    }

    fn auto_runtime(
        state: TransportRuntimeState,
        next_retry_in: Option<&str>,
        next_retry_at_ms: Option<u64>,
    ) -> TransportRuntimeRegistryEntry {
        TransportRuntimeRegistryEntry {
            circle_id: "circle-1".into(),
            driver: "native-preview-relay-runtime".into(),
            adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
            launch_status: TransportRuntimeLaunchStatus::Ready,
            launch_command: Some("p2p-chat-runtime".into()),
            launch_arguments: vec!["preview-relay".into(), "--circle".into(), "circle-1".into()],
            resolved_launch_command: Some("/usr/local/bin/p2p-chat-runtime".into()),
            launch_error: None,
            last_launch_result: None,
            last_launch_pid: None,
            last_launch_at: None,
            desired_state: TransportRuntimeDesiredState::Running,
            recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
            queue_state: if next_retry_in.is_some() {
                TransportRuntimeQueueState::Backoff
            } else {
                TransportRuntimeQueueState::Idle
            },
            restart_attempts: u32::from(next_retry_in.is_some()),
            next_retry_in: next_retry_in.map(str::to_owned),
            next_retry_at_ms,
            last_failure_reason: None,
            last_failure_at: None,
            state,
            generation: 1,
            state_since: "now".into(),
            session_label: "native::ws::circle-1".into(),
            endpoint: "native://relay/circle-1".into(),
            last_event: "native runtime active".into(),
            last_event_at: "now".into(),
        }
    }

    #[test]
    fn closed_auto_runtime_waits_until_retry_deadline() {
        let mut runtime = auto_runtime(
            TransportRuntimeState::Inactive,
            Some("in 3s"),
            Some(u64::MAX),
        );
        runtime.last_launch_result = Some(TransportRuntimeLaunchResult::Spawned);
        runtime.last_launch_pid = Some(7);
        runtime.last_launch_at = Some("now".into());

        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Closed),
            &TransportCache {
                runtime_registry: vec![runtime],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Inactive)],
            true,
        );

        assert!(actions.is_empty());
    }

    #[test]
    fn connecting_auto_runtime_requests_sync() {
        let mut runtime = auto_runtime(TransportRuntimeState::Starting, None, None);
        runtime.last_launch_result = Some(TransportRuntimeLaunchResult::Spawned);
        runtime.last_launch_pid = Some(42);
        runtime.last_launch_at = Some("now".into());

        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Connecting),
            &TransportCache {
                runtime_registry: vec![runtime],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Starting)],
            true,
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].action, TransportCircleAction::Sync));
    }

    #[test]
    fn connecting_auto_runtime_without_launch_markers_requests_connect() {
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Connecting),
            &TransportCache {
                runtime_registry: vec![auto_runtime(TransportRuntimeState::Starting, None, None)],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Starting)],
            true,
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].action, TransportCircleAction::Connect));
    }

    #[test]
    fn manual_runtime_does_not_auto_recover() {
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Closed),
            &TransportCache {
                runtime_sessions: vec![TransportRuntimeSession {
                    circle_id: "circle-1".into(),
                    driver: "local-mock-relay-daemon".into(),
                    adapter_kind: TransportRuntimeAdapterKind::Embedded,
                    launch_status: TransportRuntimeLaunchStatus::Embedded,
                    launch_command: None,
                    launch_arguments: Vec::new(),
                    resolved_launch_command: None,
                    launch_error: None,
                    last_launch_result: None,
                    last_launch_pid: None,
                    last_launch_at: None,
                    desired_state: TransportRuntimeDesiredState::Running,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
                    queue_state: TransportRuntimeQueueState::Backoff,
                    restart_attempts: 1,
                    next_retry_in: Some("in 3s".into()),
                    next_retry_at_ms: Some(100),
                    last_failure_reason: Some("local runtime heartbeat expired".into()),
                    last_failure_at: Some("now".into()),
                    state: TransportRuntimeState::Inactive,
                    generation: 1,
                    state_since: "now".into(),
                    session_label: "mock::ws::circle-1".into(),
                    endpoint: "loopback://relay/circle-1".into(),
                    last_event: "mock runtime idle".into(),
                    last_event_at: "now".into(),
                }],
                ..TransportCache::default()
            },
            &[embedded_profile(TransportRuntimeState::Inactive)],
            true,
        );

        assert!(actions.is_empty());
    }

    #[test]
    fn disabled_experimental_transport_skips_recovery_worker() {
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Closed),
            &TransportCache {
                runtime_registry: vec![auto_runtime(
                    TransportRuntimeState::Inactive,
                    Some("in 3s"),
                    Some(100),
                )],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Inactive)],
            false,
        );

        assert!(actions.is_empty());
    }

    #[test]
    fn closed_auto_runtime_only_recovers_after_due_time() {
        let now_ms = current_time_millis();
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Closed),
            &TransportCache {
                runtime_registry: vec![auto_runtime(
                    TransportRuntimeState::Inactive,
                    Some("when local runtime worker is ready"),
                    Some(now_ms.saturating_sub(1)),
                )],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Inactive)],
            true,
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].action, TransportCircleAction::Connect));
    }

    #[test]
    fn missing_launch_command_skips_auto_recovery() {
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Closed),
            &TransportCache {
                runtime_registry: vec![auto_runtime(
                    TransportRuntimeState::Inactive,
                    Some("when local runtime worker is ready"),
                    Some(0),
                )],
                ..TransportCache::default()
            },
            &[missing_profile(TransportRuntimeState::Inactive)],
            true,
        );

        assert!(actions.is_empty());
    }

    #[test]
    fn open_circle_with_inactive_auto_runtime_requests_connect_when_due() {
        let actions = collect_local_transport_recovery_actions(
            &seed(CircleStatus::Open),
            &TransportCache {
                runtime_registry: vec![auto_runtime(
                    TransportRuntimeState::Inactive,
                    Some("when local runtime worker is ready"),
                    Some(0),
                )],
                ..TransportCache::default()
            },
            &[ready_profile(TransportRuntimeState::Inactive)],
            true,
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0].action, TransportCircleAction::Connect));
    }

    fn native_labels() -> TransportRuntimeLabels {
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

    fn ready_profile(state: TransportRuntimeState) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            circle_id: "circle-1".into(),
            driver: "native-preview-relay-runtime".into(),
            adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
            launch_status: TransportRuntimeLaunchStatus::Ready,
            launch_command: Some("p2p-chat-runtime".into()),
            launch_arguments: vec!["preview-relay".into(), "--circle".into(), "circle-1".into()],
            resolved_launch_command: Some("/usr/local/bin/p2p-chat-runtime".into()),
            launch_error: None,
            recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
            state,
            session_label: "native::ws::circle-1".into(),
            endpoint: "native://relay/circle-1".into(),
            labels: native_labels(),
        }
    }

    fn missing_profile(state: TransportRuntimeState) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            launch_status: TransportRuntimeLaunchStatus::Missing,
            resolved_launch_command: None,
            launch_error: Some("command `p2p-chat-runtime` is not available on PATH".into()),
            ..ready_profile(state)
        }
    }

    fn embedded_profile(state: TransportRuntimeState) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            circle_id: "circle-1".into(),
            driver: "local-mock-relay-daemon".into(),
            adapter_kind: TransportRuntimeAdapterKind::Embedded,
            launch_status: TransportRuntimeLaunchStatus::Embedded,
            launch_command: None,
            launch_arguments: Vec::new(),
            resolved_launch_command: None,
            launch_error: None,
            recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
            state,
            session_label: "mock::ws::circle-1".into(),
            endpoint: "loopback://relay/circle-1".into(),
            labels: TransportRuntimeLabels {
                inactive_event: "mock runtime idle",
                starting_event: "mock runtime booting",
                active_event: "mock runtime active",
                connect_event: "mock runtime handshake enqueued",
                disconnect_event: "mock runtime released",
                sync_event: "mock relay checkpoint synced",
                discover_event: "mock peer sweep queued",
                sync_sessions_event: "mock session merge queued",
            },
        }
    }
}
