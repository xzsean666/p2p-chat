use crate::domain::chat::{
    ChatDomainSeed, CircleStatus, CircleType, MessageAuthor, MessageDeliveryStatus,
    RemoteDeliveryReceipt,
};
use crate::domain::chat_repository::{
    apply_change_set_to_seed, build_remote_delivery_receipt_change_set,
    merge_remote_messages_into_seed, ChatRepository,
};
use crate::domain::transport::{
    CircleTransportDiagnostic, RelayProtocol, TransportActivityItem, TransportActivityKind,
    TransportActivityLevel, TransportCapabilities, TransportChatEffects, TransportCircleAction,
    TransportCircleActionInput, TransportEngineKind, TransportHealth, TransportMutationResult,
    TransportRuntimeLaunchResult, TransportRuntimeSession, TransportService, TransportSnapshot,
    TransportSnapshotInput,
};
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_engine::{
    overall_transport_health, TransportEngine, TransportEngineState,
};
use crate::domain::transport_repository::{TransportCache, TransportRepository};
use crate::domain::transport_runtime_registry::{
    project_runtime_session, runtime_registry_entry_from_session,
};
use crate::infra::local_transport_recovery_worker::{
    collect_local_transport_recovery_actions, recovery_action_input,
};
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use crate::infra::sqlite_transport_repository::SqliteTransportRepository;
use crate::infra::transport_engine_factory::select_transport_engine;
use crate::infra::transport_runtime_factory::select_transport_runtime;
use crate::infra::transport_runtime_manager_factory::select_transport_runtime_manager;
use std::collections::HashSet;
use tauri::Runtime;

pub struct LocalTransportService<R: Runtime> {
    app_handle: tauri::AppHandle<R>,
}

impl<R: Runtime> LocalTransportService<R> {
    pub fn new(app_handle: &tauri::AppHandle<R>) -> Self {
        Self {
            app_handle: app_handle.clone(),
        }
    }
}

impl<R: Runtime> TransportService for LocalTransportService<R> {
    fn load_snapshot(&self, input: TransportSnapshotInput) -> Result<TransportSnapshot, String> {
        let mut seed = self.load_seed()?;
        let previous_cache = self.load_transport_cache()?;
        let engine = select_transport_engine(input.experimental_transport);
        let runtime = select_transport_runtime(input.experimental_transport);
        let runtime_manager = select_transport_runtime_manager();
        let runtime_options = TransportRuntimeOptions {
            use_tor_network: input.use_tor_network,
            experimental_transport: input.experimental_transport,
        };
        let mut pending_chat_effects = TransportChatEffects::default();
        let mut seed_changed = normalize_seed_runtime_status(&mut seed, &previous_cache);
        let initial_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let (normalized_previous_cache, normalized_chat_effects) =
            self.normalize_runtime_cache(&previous_cache, &initial_runtime_profiles)?;
        pending_chat_effects.append(normalized_chat_effects);
        seed_changed |= normalize_seed_runtime_status(&mut seed, &normalized_previous_cache);
        seed_changed |= reconcile_open_circle_message_delivery(&mut seed);
        let runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let recovery_actions = collect_local_transport_recovery_actions(
            &seed,
            &normalized_previous_cache,
            &runtime_profiles,
            input.experimental_transport,
        );
        let state = if recovery_actions.is_empty() {
            let mut state = engine.build_state(&seed, &normalized_previous_cache)?;
            let runtime_chat_effects = runtime_manager.sync_cache(
                &normalized_previous_cache,
                &mut state.cache,
                runtime_profiles,
                None,
            )?;
            pending_chat_effects.append(runtime_chat_effects);
            state
                .chat_effects
                .append(std::mem::take(&mut pending_chat_effects));
            let chat_effects_changed =
                apply_transport_chat_effects(&mut seed, &state.chat_effects)?;
            if chat_effects_changed {
                state.chat_effects = TransportChatEffects::default();
            }
            let (state, reconciled_seed_changed) =
                reconcile_seed_and_transport_state(engine, &mut seed, state, chat_effects_changed)?;
            seed_changed |= reconciled_seed_changed;
            let mut state = state;
            merge_runtime_session_activities(&mut state.cache, &normalized_previous_cache);
            state
        } else {
            let mut working_cache = normalized_previous_cache.clone();
            let mut state = None;

            for action in &recovery_actions {
                let action_input =
                    recovery_action_input(action, input.active_circle_id.clone(), runtime_options);
                let next_state =
                    engine.apply_circle_action(&mut seed, &working_cache, &action_input)?;
                let mut next_state = next_state;
                let recovery_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
                let runtime_chat_effects = runtime_manager.sync_cache(
                    &working_cache,
                    &mut next_state.cache,
                    recovery_runtime_profiles,
                    Some(&action_input),
                )?;
                pending_chat_effects.append(runtime_chat_effects);
                next_state
                    .chat_effects
                    .append(std::mem::take(&mut pending_chat_effects));
                let chat_effects_changed =
                    apply_transport_chat_effects(&mut seed, &next_state.chat_effects)?;
                if chat_effects_changed {
                    next_state.chat_effects = TransportChatEffects::default();
                }
                let (next_state, reconciled_seed_changed) = reconcile_seed_and_transport_state(
                    engine,
                    &mut seed,
                    next_state,
                    chat_effects_changed,
                )?;
                seed_changed |= reconciled_seed_changed;
                let mut next_state = next_state;
                merge_runtime_session_activities(&mut next_state.cache, &working_cache);
                working_cache = next_state.cache.clone();
                state = Some(next_state);
            }
            state.ok_or_else(|| "local recovery worker produced no transport state".to_string())?
        };

        if seed_changed {
            self.save_seed(&seed)?;
        }
        let snapshot = build_transport_snapshot(
            &seed,
            input,
            state.kind.clone(),
            state.diagnostics,
            state.cache.clone(),
        );
        self.save_transport_cache(state.cache)?;
        Ok(snapshot)
    }

    fn apply_circle_action(
        &self,
        input: TransportCircleActionInput,
    ) -> Result<TransportMutationResult, String> {
        let mut seed = self.load_seed()?;
        let previous_cache = self.load_transport_cache()?;
        let engine = select_transport_engine(input.experimental_transport);
        let runtime = select_transport_runtime(input.experimental_transport);
        let runtime_manager = select_transport_runtime_manager();
        let runtime_options = TransportRuntimeOptions {
            use_tor_network: input.use_tor_network,
            experimental_transport: input.experimental_transport,
        };
        let mut pending_chat_effects = TransportChatEffects::default();
        normalize_seed_runtime_status(&mut seed, &previous_cache);
        let initial_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let (normalized_previous_cache, normalized_chat_effects) =
            self.normalize_runtime_cache(&previous_cache, &initial_runtime_profiles)?;
        pending_chat_effects.append(normalized_chat_effects);
        normalize_seed_runtime_status(&mut seed, &normalized_previous_cache);
        let normalized_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        validate_runtime_action(&input, &normalized_runtime_profiles)?;
        let mut state =
            engine.apply_circle_action(&mut seed, &normalized_previous_cache, &input)?;
        let runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let runtime_chat_effects = runtime_manager.sync_cache(
            &normalized_previous_cache,
            &mut state.cache,
            runtime_profiles,
            Some(&input),
        )?;
        pending_chat_effects.append(runtime_chat_effects);
        state
            .chat_effects
            .append(std::mem::take(&mut pending_chat_effects));
        let chat_effects_changed = apply_transport_chat_effects(&mut seed, &state.chat_effects)?;
        if chat_effects_changed {
            state.chat_effects = TransportChatEffects::default();
        }
        let (state_after_reconcile, reconciled_seed_changed) =
            reconcile_seed_and_transport_state(engine, &mut seed, state, chat_effects_changed)?;
        let _ = reconciled_seed_changed;
        let mut state = state_after_reconcile;
        merge_runtime_session_activities(&mut state.cache, &normalized_previous_cache);
        self.save_seed(&seed)?;
        self.save_transport_cache(state.cache.clone())?;

        let snapshot = build_transport_snapshot(
            &seed,
            TransportSnapshotInput {
                active_circle_id: input.active_circle_id.or(Some(input.circle_id)),
                use_tor_network: input.use_tor_network,
                experimental_transport: input.experimental_transport,
            },
            state.kind,
            state.diagnostics,
            state.cache,
        );

        Ok(TransportMutationResult { seed, snapshot })
    }
}

impl<R: Runtime> LocalTransportService<R> {
    fn load_seed(&self) -> Result<ChatDomainSeed, String> {
        let repository = SqliteChatRepository::new(&self.app_handle);
        repository.load_domain_seed()
    }

    fn save_seed(&self, seed: &ChatDomainSeed) -> Result<(), String> {
        let repository = SqliteChatRepository::new(&self.app_handle);
        repository.save_domain_seed(seed.clone())
    }

    fn load_transport_cache(&self) -> Result<TransportCache, String> {
        let repository = SqliteTransportRepository::new(&self.app_handle);
        repository.load_transport_cache()
    }

    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String> {
        let repository = SqliteTransportRepository::new(&self.app_handle);
        repository.save_transport_cache(cache)
    }

    fn normalize_runtime_cache(
        &self,
        previous_cache: &TransportCache,
        runtime_profiles: &[crate::domain::transport_runtime_registry::TransportRuntimeProfile],
    ) -> Result<(TransportCache, TransportChatEffects), String> {
        let runtime_manager = select_transport_runtime_manager();
        let mut normalized_cache = previous_cache.clone();
        let chat_effects = runtime_manager.sync_cache(
            previous_cache,
            &mut normalized_cache,
            runtime_profiles.to_vec(),
            None,
        )?;
        Ok((normalized_cache, chat_effects))
    }
}

fn normalize_seed_runtime_status(seed: &mut ChatDomainSeed, cache: &TransportCache) -> bool {
    let runtime_registry = if cache.runtime_registry.is_empty() {
        cache
            .runtime_sessions
            .iter()
            .map(runtime_registry_entry_from_session)
            .collect::<Vec<_>>()
    } else {
        cache.runtime_registry.clone()
    };
    let mut changed = false;

    for circle in &mut seed.circles {
        let Some(runtime) = runtime_registry
            .iter()
            .find(|runtime| runtime.circle_id == circle.id)
        else {
            continue;
        };

        let next_status = circle_status_for_runtime(runtime);
        if std::mem::discriminant(&circle.status) != std::mem::discriminant(&next_status) {
            circle.status = next_status.clone();
            changed = true;
        }

        if !matches!(next_status, CircleStatus::Open) && circle.latency != "--" {
            circle.latency = "--".into();
            changed = true;
        }
    }

    changed
}

fn reconcile_open_circle_message_delivery(seed: &mut ChatDomainSeed) -> bool {
    let open_circle_ids = seed
        .circles
        .iter()
        .filter(|circle| matches!(circle.status, CircleStatus::Open))
        .map(|circle| circle.id.clone())
        .collect::<HashSet<_>>();
    if open_circle_ids.is_empty() {
        return false;
    }

    let target_session_ids = seed
        .sessions
        .iter()
        .filter(|session| open_circle_ids.contains(&session.circle_id))
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();
    let mut changed = false;

    for session_id in target_session_ids {
        let Some(messages) = seed.message_store.get(&session_id) else {
            continue;
        };

        let receipts = messages
            .iter()
            .filter(|message| {
                matches!(message.author, MessageAuthor::Me)
                    && matches!(
                        message.delivery_status,
                        Some(MessageDeliveryStatus::Sending)
                    )
            })
            .map(|message| RemoteDeliveryReceipt {
                remote_id: message
                    .remote_id
                    .clone()
                    .unwrap_or_else(|| acked_remote_message_id(&message.id)),
                message_id: Some(message.id.clone()),
                delivery_status: MessageDeliveryStatus::Sent,
                acked_at: Some(message.time.clone()),
            })
            .collect::<Vec<_>>();
        if receipts.is_empty() {
            continue;
        }

        let change_set = build_remote_delivery_receipt_change_set(seed, &session_id, receipts)
            .expect("open-circle receipt merge should build");
        if !change_set.messages_upsert.is_empty() {
            apply_change_set_to_seed(seed, change_set);
            changed = true;
        }
    }

    changed
}

fn acked_remote_message_id(message_id: &str) -> String {
    format!("relay-ack:{message_id}")
}

fn circle_status_for_runtime(
    runtime: &crate::domain::transport::TransportRuntimeRegistryEntry,
) -> CircleStatus {
    match runtime.state {
        crate::domain::transport::TransportRuntimeState::Active => CircleStatus::Open,
        crate::domain::transport::TransportRuntimeState::Starting => CircleStatus::Connecting,
        crate::domain::transport::TransportRuntimeState::Inactive => CircleStatus::Closed,
    }
}

fn apply_transport_chat_effects(
    seed: &mut ChatDomainSeed,
    chat_effects: &TransportChatEffects,
) -> Result<bool, String> {
    if chat_effects.is_empty() {
        return Ok(false);
    }

    let mut changed = false;

    for merge in &chat_effects.remote_message_merges {
        if merge.messages.is_empty() {
            continue;
        }

        merge_remote_messages_into_seed(seed, &merge.session_id, merge.messages.clone())?;
        changed = true;
    }

    for merge in &chat_effects.remote_delivery_receipt_merges {
        if merge.receipts.is_empty() {
            continue;
        }

        let change_set = build_remote_delivery_receipt_change_set(
            seed,
            &merge.session_id,
            merge.receipts.clone(),
        )?;
        if !change_set.messages_upsert.is_empty() {
            apply_change_set_to_seed(seed, change_set);
            changed = true;
        }
    }

    for session_id in &chat_effects.clear_unread_session_ids {
        changed |= clear_session_unread(seed, session_id);
    }

    Ok(changed)
}

fn clear_session_unread(seed: &mut ChatDomainSeed, session_id: &str) -> bool {
    let Some(session) = seed
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    else {
        return false;
    };

    if session.unread_count.is_none() {
        return false;
    }

    session.unread_count = None;
    true
}

fn reconcile_seed_and_transport_state(
    engine: &dyn TransportEngine,
    seed: &mut ChatDomainSeed,
    state: TransportEngineState,
    chat_effects_changed: bool,
) -> Result<(TransportEngineState, bool), String> {
    let status_changed = normalize_seed_runtime_status(seed, &state.cache);
    let message_changed = reconcile_open_circle_message_delivery(seed);
    if !status_changed && !message_changed && !chat_effects_changed {
        return Ok((state, false));
    }
    if !status_changed && !chat_effects_changed {
        return Ok((state, true));
    }

    let runtime_registry = state.cache.runtime_registry.clone();
    let runtime_sessions = state.cache.runtime_sessions.clone();
    let mut refreshed_state = engine.build_state(seed, &state.cache)?;
    refreshed_state.cache.runtime_registry = runtime_registry;
    refreshed_state.cache.runtime_sessions = runtime_sessions;

    Ok((refreshed_state, true))
}

fn merge_runtime_session_activities(cache: &mut TransportCache, previous_cache: &TransportCache) {
    let previous_runtime_sessions = if previous_cache.runtime_sessions.is_empty() {
        previous_cache
            .runtime_registry
            .iter()
            .map(project_runtime_session)
            .collect::<Vec<_>>()
    } else {
        previous_cache.runtime_sessions.clone()
    };
    let previous_index = previous_runtime_sessions
        .iter()
        .map(|session| (session.circle_id.as_str(), session))
        .collect::<std::collections::HashMap<_, _>>();
    let mut runtime_event_activities = cache
        .runtime_sessions
        .iter()
        .filter_map(|session| {
            let previous = previous_index.get(session.circle_id.as_str()).copied();
            runtime_session_activity_changed(previous, session)
                .then(|| build_runtime_session_activity_item(previous, session))
        })
        .collect::<Vec<_>>();

    if runtime_event_activities.is_empty() {
        return;
    }

    runtime_event_activities.extend(cache.activities.clone());
    cache.activities = trim_transport_activities(runtime_event_activities);
}

fn runtime_session_activity_changed(
    previous: Option<&TransportRuntimeSession>,
    session: &TransportRuntimeSession,
) -> bool {
    match previous {
        Some(previous) => {
            previous.last_event != session.last_event
                || previous.last_event_at != session.last_event_at
                || previous.last_failure_reason != session.last_failure_reason
                || previous.last_failure_at != session.last_failure_at
                || previous.last_launch_result != session.last_launch_result
                || previous.last_launch_at != session.last_launch_at
        }
        None => session.last_event_at == "now" || session.last_failure_reason.is_some(),
    }
}

fn build_runtime_session_activity_item(
    previous: Option<&TransportRuntimeSession>,
    session: &TransportRuntimeSession,
) -> TransportActivityItem {
    let mut detail_parts = vec![format!(
        "{} adapter · {}",
        runtime_adapter_label(&session.adapter_kind),
        session.endpoint
    )];

    if let Some(resolved_launch_command) = &session.resolved_launch_command {
        detail_parts.push(format!("resolved {resolved_launch_command}"));
    }

    if let Some(last_launch_result) = &session.last_launch_result {
        let launch_result = runtime_launch_result_label(last_launch_result);
        if let Some(last_launch_pid) = session.last_launch_pid {
            detail_parts.push(format!("launch {launch_result} · pid {last_launch_pid}"));
        } else {
            detail_parts.push(format!("launch {launch_result}"));
        }
    }

    if let Some(last_failure_reason) = &session.last_failure_reason {
        detail_parts.push(last_failure_reason.clone());
    } else if let Some(launch_error) = &session.launch_error {
        detail_parts.push(launch_error.clone());
    }

    if session.queue_state != crate::domain::transport::TransportRuntimeQueueState::Idle {
        detail_parts.push(format!(
            "{} queue{}",
            runtime_queue_label(&session.queue_state),
            session
                .next_retry_in
                .as_ref()
                .map(|next_retry| format!(" · next {next_retry}"))
                .unwrap_or_default()
        ));
    }

    TransportActivityItem {
        id: runtime_session_activity_id(previous, session),
        circle_id: session.circle_id.clone(),
        kind: TransportActivityKind::Runtime,
        level: runtime_session_activity_level(session),
        title: runtime_session_activity_title(session),
        detail: detail_parts.join(" · "),
        time: "now".into(),
    }
}

fn runtime_session_activity_id(
    previous: Option<&TransportRuntimeSession>,
    session: &TransportRuntimeSession,
) -> String {
    format!(
        "runtime-event-{}-{}-{}-{}-{}",
        session.circle_id,
        session.generation,
        session.last_event.replace(' ', "-"),
        previous
            .map(|item| item.last_event.replace(' ', "-"))
            .unwrap_or_default(),
        session.last_event_at.replace(' ', "-")
    )
}

fn runtime_session_activity_level(session: &TransportRuntimeSession) -> TransportActivityLevel {
    if session.last_failure_reason.is_some()
        || matches!(
            session.last_launch_result,
            Some(TransportRuntimeLaunchResult::Failed)
        )
        || session.last_event.contains("failed")
        || session.last_event.contains("exited")
        || session.last_event.contains("released")
    {
        return TransportActivityLevel::Warn;
    }

    if matches!(
        session.last_launch_result,
        Some(TransportRuntimeLaunchResult::Reused)
    ) || session.last_event.contains("booting")
        || session.last_event.contains("reused")
    {
        return TransportActivityLevel::Info;
    }

    TransportActivityLevel::Success
}

fn runtime_session_activity_title(session: &TransportRuntimeSession) -> String {
    let mut chars = session.last_event.chars();
    let Some(first) = chars.next() else {
        return "Runtime activity".into();
    };

    format!("{}{}", first.to_uppercase(), chars.collect::<String>())
}

fn runtime_adapter_label(
    kind: &crate::domain::transport::TransportRuntimeAdapterKind,
) -> &'static str {
    match kind {
        crate::domain::transport::TransportRuntimeAdapterKind::Embedded => "embedded",
        crate::domain::transport::TransportRuntimeAdapterKind::LocalCommand => "localCommand",
    }
}

fn runtime_launch_result_label(result: &TransportRuntimeLaunchResult) -> &'static str {
    match result {
        TransportRuntimeLaunchResult::Spawned => "spawned",
        TransportRuntimeLaunchResult::Reused => "reused",
        TransportRuntimeLaunchResult::Failed => "failed",
    }
}

fn runtime_queue_label(
    queue_state: &crate::domain::transport::TransportRuntimeQueueState,
) -> &'static str {
    match queue_state {
        crate::domain::transport::TransportRuntimeQueueState::Idle => "idle",
        crate::domain::transport::TransportRuntimeQueueState::Queued => "queued",
        crate::domain::transport::TransportRuntimeQueueState::Backoff => "backoff",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        CircleItem, CircleType, MessageAuthor, MessageDeliveryStatus, MessageItem, MessageKind,
        MessageSyncSource, SessionItem, SessionKind,
    };
    use crate::domain::transport::{
        TransportActivityKind, TransportActivityLevel, TransportCircleAction,
        TransportCircleActionInput, TransportRuntimeAdapterKind, TransportRuntimeDesiredState,
        TransportRuntimeLaunchResult, TransportRuntimeLaunchStatus, TransportRuntimeQueueState,
        TransportRuntimeRecoveryPolicy, TransportRuntimeRegistryEntry, TransportRuntimeState,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestAppGuard {
        _env_guard: MutexGuard<'static, ()>,
        app: tauri::App<tauri::test::MockRuntime>,
        config_root: PathBuf,
        previous_xdg_config_home: Option<String>,
    }

    impl Drop for TestAppGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous_xdg_config_home {
                std::env::set_var("XDG_CONFIG_HOME", previous);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }

            let _ = std::fs::remove_dir_all(&self.config_root);
        }
    }

    fn test_app() -> TestAppGuard {
        let env_guard = crate::test_support::env_lock()
            .lock()
            .expect("env lock poisoned");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let config_root =
            std::env::temp_dir().join(format!("p2p-chat-transport-service-test-{unique}"));
        std::fs::create_dir_all(&config_root).expect("failed to create test config root");

        let previous_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &config_root);

        let app = tauri::test::mock_app();
        TestAppGuard {
            _env_guard: env_guard,
            app,
            config_root,
            previous_xdg_config_home,
        }
    }

    fn seed(status: CircleStatus, latency: &str) -> ChatDomainSeed {
        ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "wss://relay.example.com".into(),
                circle_type: CircleType::Default,
                status,
                latency: latency.into(),
                description: "test".into(),
            }],
            contacts: Vec::new(),
            sessions: Vec::new(),
            groups: Vec::new(),
            message_store: HashMap::new(),
        }
    }

    fn direct_session(id: &str, circle_id: &str) -> SessionItem {
        SessionItem {
            id: id.into(),
            circle_id: circle_id.into(),
            name: id.into(),
            initials: "S".into(),
            subtitle: "test".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::Direct,
            category: "friends".into(),
            members: None,
            contact_id: None,
            archived: Some(false),
        }
    }

    fn text_message(
        id: &str,
        author: MessageAuthor,
        delivery_status: Option<MessageDeliveryStatus>,
    ) -> MessageItem {
        let is_local = matches!(author, MessageAuthor::Me);
        MessageItem {
            id: id.into(),
            kind: MessageKind::Text,
            author,
            body: id.into(),
            time: "now".into(),
            meta: None,
            delivery_status,
            remote_id: None,
            sync_source: Some(if is_local {
                MessageSyncSource::Local
            } else {
                MessageSyncSource::Relay
            }),
            acked_at: None,
        }
    }

    fn runtime_entry(state: TransportRuntimeState) -> TransportRuntimeRegistryEntry {
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
            queue_state: TransportRuntimeQueueState::Idle,
            restart_attempts: 0,
            next_retry_in: None,
            next_retry_at_ms: None,
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
    fn normalize_seed_runtime_status_projects_inactive_runtime_to_closed_circle() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        let changed = normalize_seed_runtime_status(
            &mut seed,
            &TransportCache {
                runtime_registry: vec![runtime_entry(TransportRuntimeState::Inactive)],
                ..TransportCache::default()
            },
        );

        assert!(changed);
        assert!(matches!(seed.circles[0].status, CircleStatus::Closed));
        assert_eq!(seed.circles[0].latency, "--");
    }

    #[test]
    fn normalize_seed_runtime_status_projects_starting_runtime_to_connecting_circle() {
        let mut seed = seed(CircleStatus::Closed, "--");
        let changed = normalize_seed_runtime_status(
            &mut seed,
            &TransportCache {
                runtime_registry: vec![runtime_entry(TransportRuntimeState::Starting)],
                ..TransportCache::default()
            },
        );

        assert!(changed);
        assert!(matches!(seed.circles[0].status, CircleStatus::Connecting));
        assert_eq!(seed.circles[0].latency, "--");
    }

    #[test]
    fn active_runtime_promotes_local_sending_messages_to_sent() {
        let mut seed = seed(CircleStatus::Connecting, "--");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![
                text_message(
                    "sending-me",
                    MessageAuthor::Me,
                    Some(MessageDeliveryStatus::Sending),
                ),
                text_message(
                    "failed-me",
                    MessageAuthor::Me,
                    Some(MessageDeliveryStatus::Failed),
                ),
                text_message(
                    "peer-message",
                    MessageAuthor::Peer,
                    Some(MessageDeliveryStatus::Sending),
                ),
            ],
        );
        seed.circles.push(CircleItem {
            id: "circle-2".into(),
            name: "Circle 2".into(),
            relay: "wss://relay-2.example.com".into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Closed,
            latency: "--".into(),
            description: "test".into(),
        });
        seed.sessions.push(direct_session("session-2", "circle-2"));
        seed.message_store.insert(
            "session-2".into(),
            vec![text_message(
                "other-circle-sending",
                MessageAuthor::Me,
                Some(MessageDeliveryStatus::Sending),
            )],
        );

        let status_changed = normalize_seed_runtime_status(
            &mut seed,
            &TransportCache {
                runtime_registry: vec![runtime_entry(TransportRuntimeState::Active)],
                ..TransportCache::default()
            },
        );
        let message_changed = reconcile_open_circle_message_delivery(&mut seed);

        assert!(status_changed);
        assert!(message_changed);
        let session_messages = &seed.message_store["session-1"];
        assert!(matches!(
            session_messages[0].delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(
            session_messages[0].remote_id.as_deref(),
            Some("relay-ack:sending-me")
        );
        assert_eq!(session_messages[0].acked_at.as_deref(), Some("now"));
        assert!(matches!(
            session_messages[1].delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));
        assert!(matches!(
            session_messages[2].delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(matches!(
            seed.message_store["session-2"][0].delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
    }

    #[test]
    fn apply_circle_action_applies_sync_session_chat_effects_and_rebuilds_snapshot() {
        let guard = test_app();
        let service = LocalTransportService::new(guard.app.handle());

        let result = service
            .apply_circle_action(TransportCircleActionInput {
                circle_id: "main-circle".into(),
                action: TransportCircleAction::SyncSessions,
                active_circle_id: Some("main-circle".into()),
                use_tor_network: false,
                experimental_transport: false,
            })
            .expect("sync sessions action should succeed");

        let alice_messages = result
            .seed
            .message_store
            .get("alice")
            .expect("missing alice message bucket");
        assert!(alice_messages.iter().any(|message| {
            matches!(message.kind, MessageKind::Text)
                && message.body == "Alice Chen sent a fresh relay update."
                && matches!(message.sync_source, Some(MessageSyncSource::Relay))
        }));
        assert!(alice_messages.iter().any(|message| {
            matches!(message.kind, MessageKind::System)
                && message.body == "Session sync completed across discovered relay peers."
                && matches!(message.sync_source, Some(MessageSyncSource::System))
        }));

        let alice_session = result
            .seed
            .sessions
            .iter()
            .find(|session| session.id == "alice")
            .expect("missing alice session");
        assert_eq!(alice_session.unread_count, None);
        assert_eq!(
            alice_session.subtitle,
            "Session sync completed across discovered relay peers."
        );

        let alice_sync = result
            .snapshot
            .session_sync
            .iter()
            .find(|item| item.session_id == "alice")
            .expect("missing alice sync item");
        assert_eq!(alice_sync.pending_messages, 0);
    }

    #[test]
    fn merge_runtime_session_activities_injects_new_runtime_event() {
        let previous_cache = TransportCache {
            runtime_sessions: vec![project_runtime_session(&runtime_entry(
                TransportRuntimeState::Starting,
            ))],
            activities: vec![TransportActivityItem {
                id: "runtime-circle-1".into(),
                circle_id: "circle-1".into(),
                kind: TransportActivityKind::Runtime,
                level: TransportActivityLevel::Info,
                title: "Native runtime warmup".into(),
                detail: "websocket relay via native preview engine · 0 peers · 0 queued".into(),
                time: "native relay warmup".into(),
            }],
            ..TransportCache::default()
        };
        let mut cache = TransportCache {
            runtime_sessions: vec![TransportRuntimeSession {
                circle_id: "circle-1".into(),
                driver: "native-preview-relay-runtime".into(),
                adapter_kind: TransportRuntimeAdapterKind::LocalCommand,
                launch_status: TransportRuntimeLaunchStatus::Ready,
                launch_command: Some("p2p-chat-runtime".into()),
                launch_arguments: vec![
                    "preview-relay".into(),
                    "--circle".into(),
                    "circle-1".into(),
                ],
                resolved_launch_command: Some("/usr/local/bin/p2p-chat-runtime".into()),
                launch_error: Some("failed to launch `p2p-chat-runtime`: missing binary".into()),
                last_launch_result: Some(TransportRuntimeLaunchResult::Failed),
                last_launch_pid: None,
                last_launch_at: Some("now".into()),
                desired_state: TransportRuntimeDesiredState::Running,
                recovery_policy: TransportRuntimeRecoveryPolicy::Auto,
                queue_state: TransportRuntimeQueueState::Backoff,
                restart_attempts: 1,
                next_retry_in: Some("in 3s".into()),
                next_retry_at_ms: Some(3_000),
                last_failure_reason: Some(
                    "failed to launch `p2p-chat-runtime`: missing binary".into(),
                ),
                last_failure_at: Some("now".into()),
                state: TransportRuntimeState::Inactive,
                generation: 2,
                state_since: "now".into(),
                session_label: "native::ws::circle-1".into(),
                endpoint: "native://relay/circle-1".into(),
                last_event: "native runtime launch failed".into(),
                last_event_at: "now".into(),
            }],
            activities: previous_cache.activities.clone(),
            ..TransportCache::default()
        };

        merge_runtime_session_activities(&mut cache, &previous_cache);

        assert_eq!(cache.activities.len(), 2);
        assert_eq!(cache.activities[0].circle_id, "circle-1");
        assert!(matches!(
            cache.activities[0].kind,
            TransportActivityKind::Runtime
        ));
        assert!(matches!(
            cache.activities[0].level,
            TransportActivityLevel::Warn
        ));
        assert_eq!(cache.activities[0].title, "Native runtime launch failed");
        assert!(cache.activities[0]
            .detail
            .contains("failed to launch `p2p-chat-runtime`: missing binary"));
    }
}

fn validate_runtime_action(
    input: &TransportCircleActionInput,
    runtime_profiles: &[crate::domain::transport_runtime_registry::TransportRuntimeProfile],
) -> Result<(), String> {
    if !runtime_action_requires_launch(&input.action) {
        return Ok(());
    }

    let runtime_profile = runtime_profiles
        .iter()
        .find(|profile| profile.circle_id == input.circle_id)
        .ok_or_else(|| format!("runtime profile not found for circle: {}", input.circle_id))?;

    if !matches!(
        runtime_profile.launch_status,
        crate::domain::transport::TransportRuntimeLaunchStatus::Missing
    ) {
        return Ok(());
    }

    let launch_command = runtime_profile
        .launch_command
        .as_deref()
        .unwrap_or("local runtime command");
    let detail = runtime_profile
        .launch_error
        .clone()
        .unwrap_or_else(|| format!("command `{launch_command}` is not available"));

    Err(format!("transport_runtime_launch_missing:{detail}"))
}

fn runtime_action_requires_launch(action: &TransportCircleAction) -> bool {
    !matches!(action, TransportCircleAction::Disconnect)
}

fn build_transport_snapshot(
    seed: &ChatDomainSeed,
    input: TransportSnapshotInput,
    engine: TransportEngineKind,
    diagnostics: Vec<CircleTransportDiagnostic>,
    cache: TransportCache,
) -> TransportSnapshot {
    let connected_relays = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.health, TransportHealth::Online))
        .count() as u32;
    let queued_messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.queued_messages)
        .sum();
    let active_circle_id = input
        .active_circle_id
        .filter(|circle_id| diagnostics.iter().any(|item| item.circle_id == *circle_id))
        .or_else(|| diagnostics.first().map(|item| item.circle_id.clone()))
        .unwrap_or_default();

    TransportSnapshot {
        engine,
        status: overall_transport_health(&diagnostics),
        active_circle_id,
        relay_count: diagnostics.len() as u32,
        connected_relays,
        queued_messages,
        capabilities: TransportCapabilities {
            supports_mesh: diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic.protocol, RelayProtocol::Mesh)),
            supports_paid_relays: seed
                .circles
                .iter()
                .any(|circle| matches!(circle.circle_type, CircleType::Paid)),
            supports_tor: input.use_tor_network,
            experimental_enabled: input.experimental_transport,
        },
        diagnostics,
        peers: cache.peers,
        session_sync: cache.session_sync,
        activities: cache.activities,
        runtime_sessions: cache.runtime_sessions,
    }
}
