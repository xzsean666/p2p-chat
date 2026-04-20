use crate::domain::transport::{
    SessionSyncState, TransportActivityItem, TransportChatEffects, TransportCircleAction,
    TransportCircleActionInput, TransportOutboundDispatch, TransportRelaySyncFilter,
    TransportRuntimeActionRequest, TransportRuntimeEffects, TransportRuntimeInputEvent,
    TransportRuntimeOutboundMessage, TransportRuntimePublishRequest,
};
use crate::domain::transport_repository::TransportCache;
use crate::domain::transport_runtime_manager::TransportRuntimeManager;
use crate::domain::transport_runtime_registry::{
    apply_runtime_registry_transition, project_runtime_session,
    runtime_registry_entry_from_session, TransportRuntimeProcessProbe, TransportRuntimeProfile,
    TransportRuntimeTrigger,
};
use crate::infra::local_command_transport_runtime_launcher::{
    drain_local_command_runtime_effects, enqueue_local_command_runtime_input,
    launch_local_command_runtime, probe_local_command_runtime, stop_local_command_runtime,
};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct LocalTransportRuntimeManager;

impl TransportRuntimeManager for LocalTransportRuntimeManager {
    fn sync_cache(
        &self,
        previous_cache: &TransportCache,
        cache: &mut TransportCache,
        profiles: Vec<TransportRuntimeProfile>,
        action: Option<&TransportCircleActionInput>,
        outbound_messages: &[TransportRuntimeOutboundMessage],
        relay_sync_filters: &[TransportRelaySyncFilter],
    ) -> Result<TransportChatEffects, String> {
        let now_ms = current_time_millis();
        let mut chat_effects = TransportChatEffects::default();
        let mut runtime_registry = Vec::with_capacity(profiles.len());
        for profile in profiles {
            let previous = previous_cache
                .runtime_registry
                .iter()
                .find(|entry| entry.circle_id == profile.circle_id)
                .cloned()
                .or_else(|| {
                    previous_cache
                        .runtime_sessions
                        .iter()
                        .find(|session| session.circle_id == profile.circle_id)
                        .map(runtime_registry_entry_from_session)
                });
            let previous = previous.as_ref();
            merge_runtime_effects(cache, &mut chat_effects, drain_runtime_effects(&profile)?);
            let trigger = action
                .filter(|action| action.circle_id == profile.circle_id)
                .map(|action| TransportRuntimeTrigger::Action(&action.action))
                .unwrap_or(TransportRuntimeTrigger::Hydrate);
            let process_probe = match action.filter(|action| action.circle_id == profile.circle_id)
            {
                Some(action) if !runtime_process_action_managed_locally(action) => {
                    probe_runtime_process(previous, &profile)?
                }
                Some(_) => None,
                None => probe_runtime_process(previous, &profile)?,
            };
            let launch_attempt = action
                .filter(|action| action.circle_id == profile.circle_id)
                .map(|action| {
                    apply_runtime_process_action(
                        previous,
                        &profile,
                        cache,
                        action,
                        outbound_messages,
                        relay_sync_filters,
                    )
                })
                .transpose()?
                .flatten();
            merge_runtime_effects(cache, &mut chat_effects, drain_runtime_effects(&profile)?);
            if action.is_none() {
                Self::enqueue_background_outbound_messages(
                    previous,
                    previous_cache,
                    cache,
                    &profile,
                    &process_probe,
                    outbound_messages,
                )?;
            }

            runtime_registry.push(apply_runtime_registry_transition(
                previous,
                profile,
                trigger,
                launch_attempt.as_ref(),
                process_probe.as_ref(),
                now_ms,
            ));
        }
        cache.runtime_registry = runtime_registry;
        cache.runtime_sessions = cache
            .runtime_registry
            .iter()
            .map(project_runtime_session)
            .collect();

        Ok(chat_effects)
    }
}

impl LocalTransportRuntimeManager {
    fn enqueue_background_outbound_messages(
        previous: Option<&crate::domain::transport::TransportRuntimeRegistryEntry>,
        previous_cache: &TransportCache,
        cache: &mut TransportCache,
        profile: &TransportRuntimeProfile,
        process_probe: &Option<TransportRuntimeProcessProbe>,
        outbound_messages: &[TransportRuntimeOutboundMessage],
    ) -> Result<(), String> {
        if outbound_messages.is_empty()
            || !matches!(
                profile.adapter_kind,
                crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
            )
            || !matches!(
                profile.launch_status,
                crate::domain::transport::TransportRuntimeLaunchStatus::Ready
            )
            || !previous
                .map(|entry| runtime_is_live(&entry.state))
                .unwrap_or(false)
            || process_probe.is_some()
        {
            return Ok(());
        }

        let profile_outbound_messages = outbound_messages_for_circle(
            previous_cache,
            cache,
            &profile.circle_id,
            outbound_messages,
        );
        if profile_outbound_messages.is_empty() {
            return Ok(());
        }

        let request = build_runtime_publish_request(&profile.circle_id, &profile_outbound_messages);
        let event = TransportRuntimeInputEvent::PublishOutboundMessages(request.clone());
        enqueue_local_command_runtime_input(&profile.circle_id, &event)?;
        record_outbound_dispatches(
            cache,
            profile.circle_id.as_str(),
            previous.map(|entry| entry.generation).unwrap_or_default(),
            request.request_id.as_str(),
            &request.outbound_messages,
        );
        Ok(())
    }
}

fn drain_runtime_effects(
    profile: &TransportRuntimeProfile,
) -> Result<TransportRuntimeEffects, String> {
    if !matches!(
        profile.adapter_kind,
        crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
    ) {
        return Ok(TransportRuntimeEffects::default());
    }

    drain_local_command_runtime_effects(&profile.circle_id)
}

fn merge_runtime_effects(
    cache: &mut TransportCache,
    chat_effects: &mut TransportChatEffects,
    runtime_effects: TransportRuntimeEffects,
) {
    if runtime_effects.is_empty() {
        return;
    }

    chat_effects.append(runtime_effects.chat_effects);
    apply_runtime_cache_effects(cache, runtime_effects.cache_effects);
}

fn apply_runtime_cache_effects(
    cache: &mut TransportCache,
    cache_effects: crate::domain::transport::TransportRuntimeCacheEffects,
) {
    for update in cache_effects.peer_presence_updates {
        for peer in cache
            .peers
            .iter_mut()
            .filter(|peer| peer.circle_id == update.circle_id)
        {
            if peer.blocked {
                continue;
            }
            peer.presence = update.presence.clone();
            peer.last_seen = update.last_seen.clone();
        }
    }

    for update in cache_effects.session_sync_updates {
        for item in cache
            .session_sync
            .iter_mut()
            .filter(|item| item.circle_id == update.circle_id)
        {
            if matches!(
                item.state,
                SessionSyncState::Pending | SessionSyncState::Conflict
            ) && !matches!(
                update.state,
                SessionSyncState::Pending | SessionSyncState::Conflict
            ) {
                item.state = update.state.clone();
            } else {
                item.state = update.state.clone();
            }
            item.last_merge = update.last_merge.clone();
        }
    }

    if !cache_effects.activities_append.is_empty() {
        let mut activities = cache_effects.activities_append;
        activities.extend(cache.activities.clone());
        cache.activities = trim_transport_activities(activities);
    }
}

fn trim_transport_activities(activities: Vec<TransportActivityItem>) -> Vec<TransportActivityItem> {
    let mut per_circle_counts = std::collections::HashMap::<String, u32>::new();

    activities
        .into_iter()
        .filter(|item| {
            let count = per_circle_counts.entry(item.circle_id.clone()).or_insert(0);
            if *count >= 6 {
                return false;
            }

            *count += 1;
            true
        })
        .collect()
}

fn apply_runtime_process_action(
    previous: Option<&crate::domain::transport::TransportRuntimeRegistryEntry>,
    profile: &TransportRuntimeProfile,
    cache: &mut TransportCache,
    action: &TransportCircleActionInput,
    outbound_messages: &[TransportRuntimeOutboundMessage],
    relay_sync_filters: &[TransportRelaySyncFilter],
) -> Result<Option<crate::domain::transport_runtime_registry::TransportRuntimeLaunchAttempt>, String>
{
    if matches!(
        action.action,
        crate::domain::transport::TransportCircleAction::Disconnect
    ) {
        if matches!(
            profile.adapter_kind,
            crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
        ) {
            stop_local_command_runtime(&profile.circle_id)?;
        }
        return Ok(None);
    }

    if !matches!(
        action.action,
        crate::domain::transport::TransportCircleAction::Connect
    ) {
        if matches!(
            action.action,
            TransportCircleAction::Sync
                | TransportCircleAction::DiscoverPeers
                | TransportCircleAction::SyncSessions
        ) && matches!(
            profile.adapter_kind,
            crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
        ) && previous
            .map(|entry| runtime_is_live(&entry.state))
            .unwrap_or(false)
        {
            let request = build_runtime_action_request(
                profile,
                cache,
                action,
                outbound_messages,
                relay_sync_filters,
            );
            let event = TransportRuntimeInputEvent::ApplyCircleAction(request.clone());
            enqueue_local_command_runtime_input(&profile.circle_id, &event)?;
            record_outbound_dispatches(
                cache,
                action.circle_id.as_str(),
                previous.map(|entry| entry.generation).unwrap_or_default(),
                request.request_id.as_str(),
                &request.outbound_messages,
            );
        }
        return Ok(None);
    }

    if !matches!(
        profile.adapter_kind,
        crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
    ) {
        return Ok(None);
    }

    if !matches!(
        profile.launch_status,
        crate::domain::transport::TransportRuntimeLaunchStatus::Ready
    ) {
        return Ok(None);
    }

    if previous
        .map(|entry| runtime_is_live(&entry.state))
        .unwrap_or(false)
    {
        return Ok(None);
    }

    Ok(Some(launch_local_command_runtime(profile)))
}

fn build_runtime_action_request(
    profile: &TransportRuntimeProfile,
    cache: &TransportCache,
    action: &TransportCircleActionInput,
    outbound_messages: &[TransportRuntimeOutboundMessage],
    relay_sync_filters: &[TransportRelaySyncFilter],
) -> TransportRuntimeActionRequest {
    let session_ids = cache
        .session_sync
        .iter()
        .filter(|item| item.circle_id == action.circle_id)
        .map(|item| item.session_id.clone())
        .collect::<Vec<_>>();
    let unread_session_ids = cache
        .session_sync
        .iter()
        .filter(|item| item.circle_id == action.circle_id && item.pending_messages > 0)
        .map(|item| item.session_id.clone())
        .collect::<Vec<_>>();
    let peer_count = cache
        .peers
        .iter()
        .filter(|peer| peer.circle_id == action.circle_id && !peer.blocked)
        .count() as u32;
    let primary_session_id = unread_session_ids
        .first()
        .cloned()
        .or_else(|| session_ids.first().cloned())
        .or_else(|| profile_launch_session_id(profile));

    TransportRuntimeActionRequest {
        request_id: format!(
            "{}:{}:{}",
            runtime_action_token(&action.action),
            action.circle_id,
            current_time_millis()
        ),
        circle_id: action.circle_id.clone(),
        action: action.action.clone(),
        primary_session_id,
        session_ids: session_ids.clone(),
        unread_session_ids,
        peer_count,
        session_sync_count: session_ids.len() as u32,
        sync_since_created_at: action.sync_since_created_at,
        relay_sync_filters: relay_sync_filters.to_vec(),
        outbound_messages: outbound_messages.to_vec(),
    }
}

fn build_runtime_publish_request(
    circle_id: &str,
    outbound_messages: &[TransportRuntimeOutboundMessage],
) -> TransportRuntimePublishRequest {
    TransportRuntimePublishRequest {
        request_id: format!("publish:{circle_id}:{}", current_time_millis()),
        circle_id: circle_id.to_string(),
        outbound_messages: outbound_messages.to_vec(),
    }
}

fn outbound_messages_for_circle(
    previous_cache: &TransportCache,
    cache: &TransportCache,
    circle_id: &str,
    outbound_messages: &[TransportRuntimeOutboundMessage],
) -> Vec<TransportRuntimeOutboundMessage> {
    let session_ids = cache
        .session_sync
        .iter()
        .chain(previous_cache.session_sync.iter())
        .filter(|item| item.circle_id == circle_id)
        .map(|item| item.session_id.as_str())
        .collect::<HashSet<_>>();
    if session_ids.is_empty() {
        return Vec::new();
    }

    outbound_messages
        .iter()
        .filter(|message| session_ids.contains(message.session_id.as_str()))
        .cloned()
        .collect()
}

fn record_outbound_dispatches(
    cache: &mut TransportCache,
    circle_id: &str,
    runtime_generation: u32,
    request_id: &str,
    outbound_messages: &[TransportRuntimeOutboundMessage],
) {
    if outbound_messages.is_empty() {
        return;
    }

    for outbound in outbound_messages {
        let already_recorded = cache.outbound_dispatches.iter().any(|dispatch| {
            dispatch.circle_id == circle_id
                && dispatch.session_id == outbound.session_id
                && dispatch.message_id == outbound.message_id
                && dispatch.event_id == outbound.signed_nostr_event.event_id
                && dispatch.runtime_generation == runtime_generation
        });
        if already_recorded {
            continue;
        }

        cache.outbound_dispatches.push(TransportOutboundDispatch {
            circle_id: circle_id.to_string(),
            session_id: outbound.session_id.clone(),
            message_id: outbound.message_id.clone(),
            remote_id: outbound.remote_id.clone(),
            event_id: outbound.signed_nostr_event.event_id.clone(),
            runtime_generation,
            request_id: request_id.to_string(),
            dispatched_at: "now".into(),
        });
    }
}

fn profile_launch_session_id(profile: &TransportRuntimeProfile) -> Option<String> {
    profile
        .launch_arguments
        .windows(2)
        .find(|pair| pair[0] == "--session")
        .map(|pair| pair[1].clone())
}

fn runtime_action_token(action: &TransportCircleAction) -> &'static str {
    match action {
        TransportCircleAction::Connect => "connect",
        TransportCircleAction::Disconnect => "disconnect",
        TransportCircleAction::Sync => "sync",
        TransportCircleAction::DiscoverPeers => "discover-peers",
        TransportCircleAction::SyncSessions => "sync-sessions",
    }
}

fn probe_runtime_process(
    previous: Option<&crate::domain::transport::TransportRuntimeRegistryEntry>,
    profile: &TransportRuntimeProfile,
) -> Result<Option<TransportRuntimeProcessProbe>, String> {
    if !matches!(
        profile.adapter_kind,
        crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand
    ) {
        return Ok(None);
    }

    if !previous
        .map(|entry| runtime_is_live(&entry.state))
        .unwrap_or(false)
    {
        return Ok(None);
    }

    probe_local_command_runtime(&profile.circle_id)
}

fn runtime_process_action_managed_locally(action: &TransportCircleActionInput) -> bool {
    matches!(
        action.action,
        crate::domain::transport::TransportCircleAction::Connect
            | crate::domain::transport::TransportCircleAction::Disconnect
    )
}

fn runtime_is_live(state: &crate::domain::transport::TransportRuntimeState) -> bool {
    matches!(
        state,
        crate::domain::transport::TransportRuntimeState::Starting
            | crate::domain::transport::TransportRuntimeState::Active
    )
}

fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        MergeRemoteMessagesInput, MessageAuthor, MessageItem, MessageKind, MessageSyncSource,
        SignedNostrEvent,
    };
    use crate::domain::transport::{
        CirclePeerPresenceUpdate, CircleSessionSyncUpdate, DiscoveredPeer, PeerPresence,
        SessionSyncItem, SessionSyncState, TransportActivityItem, TransportActivityKind,
        TransportActivityLevel, TransportCircleAction, TransportCircleActionInput,
        TransportRuntimeAdapterKind, TransportRuntimeDesiredState, TransportRuntimeLaunchResult,
        TransportRuntimeLaunchStatus, TransportRuntimeOutboundMessage, TransportRuntimeOutputEvent,
        TransportRuntimeQueueState, TransportRuntimeRecoveryPolicy, TransportRuntimeSession,
        TransportRuntimeState,
    };
    use crate::domain::transport_runtime_registry::TransportRuntimeLabels;
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

    fn unique_circle_id(prefix: &str) -> String {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
        format!("{prefix}-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn local_command_profile(
        circle_id: &str,
        command: String,
        args: Vec<String>,
        state: TransportRuntimeState,
    ) -> TransportRuntimeProfile {
        TransportRuntimeProfile {
            circle_id: circle_id.into(),
            driver: "native-preview-relay-runtime".into(),
            adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
            launch_status: TransportRuntimeLaunchStatus::Ready,
            launch_command: Some(command.clone()),
            launch_arguments: args,
            resolved_launch_command: Some(command),
            launch_error: None,
            recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
            state,
            session_label: format!("native::ws::{circle_id}"),
            endpoint: format!("native://relay/{circle_id}"),
            labels: labels(),
        }
    }

    #[cfg(unix)]
    fn runtime_output_command(event_json: &str) -> (String, Vec<String>) {
        (
            "sh".into(),
            vec![
                "-c".into(),
                format!("printf '%s\\n' '{event_json}'; sleep 1"),
            ],
        )
    }

    #[cfg(unix)]
    fn runtime_output_events_command(event_json_lines: &[String]) -> (String, Vec<String>) {
        let mut script = String::from("printf '%s\\n'");
        for line in event_json_lines {
            script.push(' ');
            script.push('\'');
            script.push_str(line);
            script.push('\'');
        }
        script.push_str("; sleep 1");

        ("sh".into(), vec!["-c".into(), script])
    }

    fn runtime_input_queue_path(circle_id: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join("p2p-chat-runtime")
            .join("requests")
            .join(format!("{circle_id}.jsonl"))
    }

    fn outbound_runtime_message(
        session_id: &str,
        message_id: &str,
        remote_id: &str,
    ) -> TransportRuntimeOutboundMessage {
        TransportRuntimeOutboundMessage {
            session_id: session_id.into(),
            message_id: message_id.into(),
            remote_id: remote_id.into(),
            signed_nostr_event: SignedNostrEvent {
                event_id: remote_id.into(),
                pubkey: "02b4631d6f1d6659d8e7a0f4d1f56ea74413c5fc11d16f55b3e25a03e353dd1510".into(),
                created_at: 1_735_689_600,
                kind: 1,
                tags: Vec::new(),
                content: "queued runtime payload".into(),
                signature: "b".repeat(128),
            },
        }
    }

    #[test]
    fn sync_cache_projects_profiles_into_runtime_registry() {
        let manager = LocalTransportRuntimeManager;
        let previous_cache = TransportCache {
            runtime_sessions: vec![TransportRuntimeSession {
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
                session_label: "mock::mesh::circle-1".into(),
                endpoint: "loopback://mesh/circle-1".into(),
                last_event: "mock runtime idle".into(),
                last_event_at: "not started".into(),
            }],
            ..TransportCache::default()
        };
        let mut cache = TransportCache::default();

        manager
            .sync_cache(
                &previous_cache,
                &mut cache,
                vec![TransportRuntimeProfile {
                    circle_id: "circle-1".into(),
                    driver: "local-mock-mesh-daemon".into(),
                    adapter_kind: TransportRuntimeAdapterKind::Embedded,
                    launch_status: TransportRuntimeLaunchStatus::Embedded,
                    launch_command: None,
                    launch_arguments: Vec::new(),
                    resolved_launch_command: None,
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Manual,
                    state: TransportRuntimeState::Starting,
                    session_label: "mock::mesh::circle-1".into(),
                    endpoint: "loopback://mesh/circle-1".into(),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: false,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("runtime manager should sync");

        assert_eq!(cache.runtime_sessions.len(), 1);
        assert!(matches!(
            cache.runtime_sessions[0].state,
            TransportRuntimeState::Starting
        ));
        assert!(matches!(
            cache.runtime_sessions[0].desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(matches!(
            cache.runtime_sessions[0].queue_state,
            TransportRuntimeQueueState::Idle
        ));
        assert_eq!(cache.runtime_sessions[0].restart_attempts, 0);
        assert_eq!(cache.runtime_sessions[0].next_retry_in, None);
        assert_eq!(cache.runtime_sessions[0].next_retry_at_ms, None);
        assert_eq!(cache.runtime_sessions[0].last_failure_reason, None);
        assert_eq!(cache.runtime_sessions[0].last_failure_at, None);
        assert_eq!(cache.runtime_sessions[0].generation, 2);
        assert_eq!(
            cache.runtime_sessions[0].last_event,
            "mock runtime handshake enqueued"
        );
    }

    #[test]
    fn sync_cache_records_successful_local_command_launch_attempt() {
        let manager = LocalTransportRuntimeManager;
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let mut cache = TransportCache::default();

        manager
            .sync_cache(
                &TransportCache::default(),
                &mut cache,
                vec![TransportRuntimeProfile {
                    circle_id: "circle-1".into(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(current_executable.to_string_lossy().into_owned()),
                    launch_arguments: vec!["--help".into()],
                    resolved_launch_command: Some(
                        current_executable.to_string_lossy().into_owned(),
                    ),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Starting,
                    session_label: "native::ws::circle-1".into(),
                    endpoint: "native://relay/circle-1".into(),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("runtime manager should record launch attempt");

        assert_eq!(
            cache.runtime_sessions[0].last_launch_result,
            Some(TransportRuntimeLaunchResult::Spawned)
        );
        assert!(cache.runtime_sessions[0].last_launch_pid.is_some());
        assert_eq!(
            cache.runtime_sessions[0].last_launch_at.as_deref(),
            Some("now")
        );
        assert_eq!(cache.runtime_sessions[0].last_failure_reason, None);
        assert_eq!(
            cache.runtime_sessions[0].last_event,
            "native runtime launch spawned"
        );
    }

    #[test]
    fn sync_cache_records_failed_local_command_launch_attempt() {
        let manager = LocalTransportRuntimeManager;
        let invalid_command = std::env::temp_dir();
        let mut cache = TransportCache::default();

        manager
            .sync_cache(
                &TransportCache::default(),
                &mut cache,
                vec![TransportRuntimeProfile {
                    circle_id: "circle-1".into(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(invalid_command.to_string_lossy().into_owned()),
                    launch_arguments: Vec::new(),
                    resolved_launch_command: Some(invalid_command.to_string_lossy().into_owned()),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Starting,
                    session_label: "native::ws::circle-1".into(),
                    endpoint: "native://relay/circle-1".into(),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("runtime manager should record failed launch attempt");

        assert_eq!(
            cache.runtime_sessions[0].last_launch_result,
            Some(TransportRuntimeLaunchResult::Failed)
        );
        assert_eq!(cache.runtime_sessions[0].last_launch_pid, None);
        assert_eq!(
            cache.runtime_sessions[0].last_launch_at.as_deref(),
            Some("now")
        );
        assert!(matches!(
            cache.runtime_sessions[0].state,
            TransportRuntimeState::Inactive
        ));
        assert!(matches!(
            cache.runtime_sessions[0].queue_state,
            TransportRuntimeQueueState::Backoff
        ));
        assert_eq!(cache.runtime_sessions[0].restart_attempts, 1);
        assert!(cache.runtime_sessions[0]
            .last_failure_reason
            .as_deref()
            .is_some_and(|message| message.contains("failed to launch")));
        assert_eq!(
            cache.runtime_sessions[0].last_event,
            "native runtime launch failed"
        );
    }

    #[test]
    fn sync_cache_disconnect_stops_managed_local_command_runtime() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("disconnect");
        let (command, args) = sleeping_command();
        let mut cache = TransportCache::default();

        manager
            .sync_cache(
                &TransportCache::default(),
                &mut cache,
                vec![TransportRuntimeProfile {
                    circle_id: circle_id.clone(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(command.clone()),
                    launch_arguments: args.clone(),
                    resolved_launch_command: Some(command.clone()),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Starting,
                    session_label: format!("native::ws::{circle_id}"),
                    endpoint: format!("native://relay/{circle_id}"),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("connect should spawn runtime");

        let previous_cache = cache.clone();
        let mut disconnected_cache = TransportCache::default();
        manager
            .sync_cache(
                &previous_cache,
                &mut disconnected_cache,
                vec![TransportRuntimeProfile {
                    circle_id: circle_id.clone(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(command.clone()),
                    launch_arguments: args.clone(),
                    resolved_launch_command: Some(command.clone()),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Inactive,
                    session_label: format!("native::ws::{circle_id}"),
                    endpoint: format!("native://relay/{circle_id}"),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Disconnect,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("disconnect should stop runtime");

        assert!(matches!(
            disconnected_cache.runtime_sessions[0].state,
            TransportRuntimeState::Inactive
        ));
        assert_eq!(
            disconnected_cache.runtime_sessions[0].last_event,
            "mock runtime released"
        );

        let previous_cache = disconnected_cache.clone();
        let mut restarted_cache = TransportCache::default();
        manager
            .sync_cache(
                &previous_cache,
                &mut restarted_cache,
                vec![TransportRuntimeProfile {
                    circle_id: circle_id.clone(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(command.clone()),
                    launch_arguments: args,
                    resolved_launch_command: Some(command),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Starting,
                    session_label: format!("native::ws::{circle_id}"),
                    endpoint: format!("native://relay/{circle_id}"),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some(circle_id),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("runtime should be launchable again after disconnect");

        assert_eq!(
            restarted_cache.runtime_sessions[0].last_launch_result,
            Some(TransportRuntimeLaunchResult::Spawned)
        );
    }

    #[test]
    fn sync_cache_hydrate_marks_runtime_inactive_after_managed_process_exits() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("hydrate-exit");
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let mut connected_cache = TransportCache::default();

        manager
            .sync_cache(
                &TransportCache::default(),
                &mut connected_cache,
                vec![TransportRuntimeProfile {
                    circle_id: circle_id.clone(),
                    driver: "native-preview-relay-runtime".into(),
                    adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                    launch_status: TransportRuntimeLaunchStatus::Ready,
                    launch_command: Some(current_executable.to_string_lossy().into_owned()),
                    launch_arguments: vec!["--help".into()],
                    resolved_launch_command: Some(
                        current_executable.to_string_lossy().into_owned(),
                    ),
                    launch_error: None,
                    recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                    state: TransportRuntimeState::Starting,
                    session_label: format!("native::ws::{circle_id}"),
                    endpoint: format!("native://relay/{circle_id}"),
                    labels: labels(),
                }],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("connect should spawn runtime");

        let mut previous_cache = connected_cache.clone();
        let mut hydrated_cache = TransportCache::default();
        let mut detected_exit = false;

        for _ in 0..40 {
            hydrated_cache = TransportCache::default();
            manager
                .sync_cache(
                    &previous_cache,
                    &mut hydrated_cache,
                    vec![TransportRuntimeProfile {
                        circle_id: circle_id.clone(),
                        driver: "native-preview-relay-runtime".into(),
                        adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                        launch_status: TransportRuntimeLaunchStatus::Ready,
                        launch_command: Some(current_executable.to_string_lossy().into_owned()),
                        launch_arguments: vec!["--help".into()],
                        resolved_launch_command: Some(
                            current_executable.to_string_lossy().into_owned(),
                        ),
                        launch_error: None,
                        recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                        state: TransportRuntimeState::Active,
                        session_label: format!("native::ws::{circle_id}"),
                        endpoint: format!("native://relay/{circle_id}"),
                        labels: labels(),
                    }],
                    None,
                    &[],
                    &[],
                )
                .expect("hydrate should inspect managed runtime");

            if matches!(
                hydrated_cache.runtime_sessions[0].state,
                TransportRuntimeState::Inactive
            ) {
                detected_exit = true;
                break;
            }

            previous_cache = hydrated_cache.clone();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            detected_exit,
            "hydrate should detect exited runtime within polling window"
        );

        assert!(matches!(
            hydrated_cache.runtime_sessions[0].state,
            TransportRuntimeState::Inactive
        ));
        assert!(matches!(
            hydrated_cache.runtime_sessions[0].queue_state,
            TransportRuntimeQueueState::Backoff
        ));
        assert_eq!(hydrated_cache.runtime_sessions[0].restart_attempts, 1);
        assert_eq!(
            hydrated_cache.runtime_sessions[0].last_event,
            "native runtime process exited"
        );
        assert!(hydrated_cache.runtime_sessions[0]
            .last_failure_reason
            .as_deref()
            .is_some_and(|message| message.contains("exited with status")));
    }

    #[cfg(unix)]
    #[test]
    fn sync_cache_drains_runtime_stdout_chat_effects() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("stdout-effects");
        let event_json = serde_json::to_string(&TransportRuntimeOutputEvent::MergeRemoteMessages(
            MergeRemoteMessagesInput {
                session_id: "session-1".into(),
                messages: vec![MessageItem {
                    id: "runtime-remote-1".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: "runtime stdout relay update".into(),
                    time: "now".into(),
                    meta: None,
                    delivery_status: None,
                    remote_id: Some("relay:runtime:1".into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            },
        ))
        .expect("runtime output event should serialize");
        let (command, args) = runtime_output_command(&event_json);
        let mut connected_cache = TransportCache::default();
        let mut drained_effects = manager
            .sync_cache(
                &TransportCache::default(),
                &mut connected_cache,
                vec![local_command_profile(
                    &circle_id,
                    command.clone(),
                    args.clone(),
                    TransportRuntimeState::Starting,
                )],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("connect should launch runtime");

        let mut previous_cache = connected_cache;
        for _ in 0..40 {
            if !drained_effects.is_empty() {
                break;
            }

            let mut hydrated_cache = TransportCache::default();
            drained_effects = manager
                .sync_cache(
                    &previous_cache,
                    &mut hydrated_cache,
                    vec![local_command_profile(
                        &circle_id,
                        command.clone(),
                        args.clone(),
                        TransportRuntimeState::Active,
                    )],
                    None,
                    &[],
                    &[],
                )
                .expect("hydrate should drain runtime stdout effects");
            previous_cache = hydrated_cache;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(drained_effects.remote_message_merges.len(), 1);
        let merge = &drained_effects.remote_message_merges[0];
        assert_eq!(merge.session_id, "session-1");
        assert_eq!(merge.messages.len(), 1);
        assert_eq!(merge.messages[0].body, "runtime stdout relay update");
        assert_eq!(
            merge.messages[0].remote_id.as_deref(),
            Some("relay:runtime:1")
        );

        stop_local_command_runtime(&circle_id).expect("runtime cleanup should succeed");
    }

    #[cfg(unix)]
    #[test]
    fn sync_cache_applies_runtime_stdout_cache_effects() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("stdout-cache-effects");
        let event_json_lines = vec![
            serde_json::to_string(&TransportRuntimeOutputEvent::SetCirclePeerPresence(
                CirclePeerPresenceUpdate {
                    circle_id: circle_id.clone(),
                    presence: PeerPresence::Online,
                    last_seen: "runtime peer sweep".into(),
                },
            ))
            .expect("peer presence event should serialize"),
            serde_json::to_string(&TransportRuntimeOutputEvent::SetCircleSessionSyncState(
                CircleSessionSyncUpdate {
                    circle_id: circle_id.clone(),
                    state: SessionSyncState::Idle,
                    last_merge: "runtime merge complete".into(),
                },
            ))
            .expect("session sync event should serialize"),
            serde_json::to_string(&TransportRuntimeOutputEvent::AppendActivity {
                activity: TransportActivityItem {
                    id: format!("runtime-activity-{circle_id}"),
                    circle_id: circle_id.clone(),
                    kind: TransportActivityKind::DiscoverPeers,
                    level: TransportActivityLevel::Success,
                    title: "Preview runtime peer sweep committed".into(),
                    detail: "runtime updated peer presence from stdout".into(),
                    time: "now".into(),
                },
            })
            .expect("activity event should serialize"),
        ];
        let (command, args) = runtime_output_events_command(&event_json_lines);
        let mut connected_cache = TransportCache {
            peers: vec![DiscoveredPeer {
                circle_id: circle_id.clone(),
                contact_id: "alice-contact".into(),
                name: "Alice".into(),
                handle: "@alice".into(),
                presence: PeerPresence::Offline,
                route: "native relay".into(),
                shared_sessions: 1,
                last_seen: "offline".into(),
                blocked: false,
            }],
            session_sync: vec![SessionSyncItem {
                circle_id: circle_id.clone(),
                session_id: "session-1".into(),
                session_name: "Alice".into(),
                state: SessionSyncState::Syncing,
                pending_messages: 0,
                source: "native relay".into(),
                last_merge: "pending merge".into(),
            }],
            ..TransportCache::default()
        };
        manager
            .sync_cache(
                &TransportCache::default(),
                &mut connected_cache,
                vec![local_command_profile(
                    &circle_id,
                    command.clone(),
                    args.clone(),
                    TransportRuntimeState::Starting,
                )],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[],
                &[],
            )
            .expect("connect should launch runtime");

        let mut previous_cache = connected_cache;
        let mut drained_effects = TransportChatEffects::default();
        for _ in 0..40 {
            let mut hydrated_cache = previous_cache.clone();
            drained_effects = manager
                .sync_cache(
                    &previous_cache,
                    &mut hydrated_cache,
                    vec![local_command_profile(
                        &circle_id,
                        command.clone(),
                        args.clone(),
                        TransportRuntimeState::Active,
                    )],
                    None,
                    &[],
                    &[],
                )
                .expect("hydrate should apply runtime cache effects");
            previous_cache = hydrated_cache;
            if previous_cache.peers[0].last_seen == "runtime peer sweep"
                && previous_cache.session_sync[0].last_merge == "runtime merge complete"
                && previous_cache
                    .activities
                    .iter()
                    .any(|activity| activity.id == format!("runtime-activity-{circle_id}"))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(drained_effects.is_empty());
        assert!(matches!(
            previous_cache.peers[0].presence,
            PeerPresence::Online
        ));
        assert_eq!(previous_cache.peers[0].last_seen, "runtime peer sweep");
        assert!(matches!(
            previous_cache.session_sync[0].state,
            SessionSyncState::Idle
        ));
        assert_eq!(
            previous_cache.session_sync[0].last_merge,
            "runtime merge complete"
        );
        assert_eq!(
            previous_cache.activities[0].id,
            format!("runtime-activity-{circle_id}")
        );
        assert_eq!(
            previous_cache.activities[0].title,
            "Preview runtime peer sweep committed"
        );

        stop_local_command_runtime(&circle_id).expect("runtime cleanup should succeed");
    }

    #[test]
    fn sync_cache_sync_sessions_enqueues_request_for_live_local_command_runtime() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("queued-sync");
        let queue_path = runtime_input_queue_path(&circle_id);
        let _ = std::fs::remove_file(&queue_path);
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let previous_cache = TransportCache {
            runtime_sessions: vec![TransportRuntimeSession {
                circle_id: circle_id.clone(),
                driver: "native-preview-relay-runtime".into(),
                adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                launch_status: TransportRuntimeLaunchStatus::Ready,
                launch_command: Some(current_executable.to_string_lossy().into_owned()),
                launch_arguments: vec![
                    "preview-relay".into(),
                    "--circle".into(),
                    circle_id.clone(),
                    "--session".into(),
                    "session-1".into(),
                ],
                resolved_launch_command: Some(current_executable.to_string_lossy().into_owned()),
                launch_error: None,
                last_launch_result: Some(TransportRuntimeLaunchResult::Spawned),
                last_launch_pid: Some(1234),
                last_launch_at: Some("now".into()),
                desired_state: TransportRuntimeDesiredState::Running,
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                queue_state: TransportRuntimeQueueState::Idle,
                restart_attempts: 0,
                next_retry_in: None,
                next_retry_at_ms: None,
                last_failure_reason: None,
                last_failure_at: None,
                state: TransportRuntimeState::Active,
                generation: 1,
                state_since: "now".into(),
                session_label: format!("native::ws::{circle_id}"),
                endpoint: format!("native://relay/{circle_id}"),
                last_event: "native runtime active".into(),
                last_event_at: "now".into(),
            }],
            ..TransportCache::default()
        };

        let mut cache = TransportCache::default();
        let drained_effects = manager
            .sync_cache(
                &previous_cache,
                &mut cache,
                vec![local_command_profile(
                    &circle_id,
                    current_executable.to_string_lossy().into_owned(),
                    vec![
                        "preview-relay".into(),
                        "--circle".into(),
                        circle_id.clone(),
                        "--session".into(),
                        "session-1".into(),
                    ],
                    TransportRuntimeState::Active,
                )],
                Some(&TransportCircleActionInput {
                    circle_id: circle_id.clone(),
                    action: TransportCircleAction::SyncSessions,
                    active_circle_id: Some(circle_id.clone()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                }),
                &[outbound_runtime_message(
                    "session-1",
                    "message-1",
                    "event-1",
                )],
                &[],
            )
            .expect("sync sessions should enqueue runtime request");

        assert!(drained_effects.is_empty());
        let contents =
            std::fs::read_to_string(&queue_path).expect("runtime input queue should be written");
        assert!(contents.contains("\"kind\":\"applyCircleAction\""));
        assert!(contents.contains("\"action\":\"syncSessions\""));
        assert!(contents.contains("\"outboundMessages\""));
        assert!(contents.contains("\"eventId\":\"event-1\""));
        assert_eq!(cache.outbound_dispatches.len(), 1);
        assert_eq!(cache.outbound_dispatches[0].message_id, "message-1");
        assert_eq!(cache.outbound_dispatches[0].event_id, "event-1");
        assert_eq!(cache.outbound_dispatches[0].runtime_generation, 1);

        let _ = std::fs::remove_file(queue_path);
    }

    #[test]
    fn sync_cache_hydrate_enqueues_background_publish_for_live_local_command_runtime() {
        let manager = LocalTransportRuntimeManager;
        let circle_id = unique_circle_id("publish");
        let queue_path = runtime_input_queue_path(&circle_id);
        let _ = std::fs::remove_file(&queue_path);
        let current_executable =
            std::env::current_exe().expect("current test executable path should resolve");
        let previous_cache = TransportCache {
            runtime_sessions: vec![TransportRuntimeSession {
                circle_id: circle_id.clone(),
                driver: "native-preview-relay-runtime".into(),
                adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                launch_status: TransportRuntimeLaunchStatus::Ready,
                launch_command: Some(current_executable.to_string_lossy().into_owned()),
                launch_arguments: vec![
                    "preview-relay".into(),
                    "--circle".into(),
                    circle_id.clone(),
                    "--session".into(),
                    "session-1".into(),
                ],
                resolved_launch_command: Some(current_executable.to_string_lossy().into_owned()),
                launch_error: None,
                last_launch_result: Some(TransportRuntimeLaunchResult::Spawned),
                last_launch_pid: Some(1234),
                last_launch_at: Some("now".into()),
                desired_state: TransportRuntimeDesiredState::Running,
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                queue_state: TransportRuntimeQueueState::Idle,
                restart_attempts: 0,
                next_retry_in: None,
                next_retry_at_ms: None,
                last_failure_reason: None,
                last_failure_at: None,
                state: TransportRuntimeState::Active,
                generation: 1,
                state_since: "now".into(),
                session_label: format!("native::ws::{circle_id}"),
                endpoint: format!("native://relay/{circle_id}"),
                last_event: "native runtime active".into(),
                last_event_at: "now".into(),
            }],
            session_sync: vec![SessionSyncItem {
                circle_id: circle_id.clone(),
                session_id: "session-1".into(),
                session_name: "session-1".into(),
                state: SessionSyncState::Idle,
                pending_messages: 0,
                source: "native relay".into(),
                last_merge: "now".into(),
            }],
            ..TransportCache::default()
        };

        let mut cache = TransportCache {
            session_sync: previous_cache.session_sync.clone(),
            ..TransportCache::default()
        };
        let drained_effects = manager
            .sync_cache(
                &previous_cache,
                &mut cache,
                vec![local_command_profile(
                    &circle_id,
                    current_executable.to_string_lossy().into_owned(),
                    vec![
                        "preview-relay".into(),
                        "--circle".into(),
                        circle_id.clone(),
                        "--session".into(),
                        "session-1".into(),
                    ],
                    TransportRuntimeState::Active,
                )],
                None,
                &[outbound_runtime_message(
                    "session-1",
                    "message-1",
                    "event-1",
                )],
                &[],
            )
            .expect("hydrate should enqueue background publish request");

        assert!(drained_effects.is_empty());
        let contents =
            std::fs::read_to_string(&queue_path).expect("runtime input queue should be written");
        assert!(contents.contains("\"kind\":\"publishOutboundMessages\""));
        assert!(contents.contains("\"requestId\":\"publish:"));
        assert!(contents.contains("\"eventId\":\"event-1\""));
        assert_eq!(cache.outbound_dispatches.len(), 1);
        assert_eq!(cache.outbound_dispatches[0].message_id, "message-1");
        assert_eq!(cache.outbound_dispatches[0].event_id, "event-1");
        assert_eq!(cache.outbound_dispatches[0].runtime_generation, 1);

        let _ = std::fs::remove_file(queue_path);
    }
}
