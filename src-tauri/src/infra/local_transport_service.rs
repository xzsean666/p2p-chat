use crate::app::shell_auth;
use crate::domain::chat::{
    ChatDomainSeed, CircleStatus, CircleType, MergeRemoteMessagesInput, MessageAuthor,
    MessageDeliveryStatus, MessageItem, MessageSyncSource, RemoteDeliveryReceipt, SessionKind,
};
use crate::domain::chat_repository::{
    apply_change_set_to_seed, build_remote_delivery_receipt_change_set,
    merge_remote_messages_into_seed, ChatRepository,
};
use crate::domain::transport::{
    CircleTransportDiagnostic, RelayProtocol, TransportActivityItem, TransportActivityKind,
    TransportActivityLevel, TransportCapabilities, TransportChatEffects, TransportCircleAction,
    TransportCircleActionInput, TransportEngineKind, TransportHealth, TransportMutationResult,
    TransportRelaySyncFilter, TransportRuntimeLaunchResult, TransportRuntimeOutboundMessage,
    TransportRuntimeSession, TransportService, TransportSnapshot, TransportSnapshotInput,
};
use crate::domain::transport_adapter::TransportRuntimeOptions;
use crate::domain::transport_engine::{
    overall_transport_health, TransportEngine, TransportEngineState,
};
use crate::domain::transport_repository::{
    TransportCache, TransportRelaySyncCursor, TransportRepository,
};
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
use nostr_connect::prelude::PublicKey as NostrPublicKey;
use std::collections::{HashMap, HashSet};
use tauri::Runtime;

pub struct LocalTransportService<R: Runtime> {
    app_handle: tauri::AppHandle<R>,
}

const RELAY_SYNC_OVERLAP_SECS: u64 = 300;

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
        let current_user_pubkey = self.resolve_current_user_pubkey();
        let mut pending_chat_effects = TransportChatEffects::default();
        let mut seed_changed = normalize_seed_runtime_status(&mut seed, &previous_cache);
        let initial_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let (mut normalized_previous_cache, normalized_chat_effects) =
            self.normalize_runtime_cache(&previous_cache, &initial_runtime_profiles)?;
        pending_chat_effects.append(normalized_chat_effects);
        seed_changed |= normalize_seed_runtime_status(&mut seed, &normalized_previous_cache);
        seed_changed |= reconcile_open_circle_message_delivery(&mut seed);
        reconcile_transport_outbound_dispatches(&mut normalized_previous_cache, &seed);
        reconcile_transport_relay_sync_cursors(&mut normalized_previous_cache, &seed);
        let runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let recovery_actions = collect_local_transport_recovery_actions(
            &seed,
            &normalized_previous_cache,
            &runtime_profiles,
            input.experimental_transport,
        );
        let state = if recovery_actions.is_empty() {
            let mut state = engine.build_state(&seed, &normalized_previous_cache)?;
            let outbound_messages =
                collect_transport_outbound_messages(&seed, &normalized_previous_cache, None);
            let runtime_chat_effects = runtime_manager.sync_cache(
                &normalized_previous_cache,
                &mut state.cache,
                runtime_profiles,
                None,
                &outbound_messages,
                &[],
            )?;
            pending_chat_effects.append(runtime_chat_effects);
            state
                .chat_effects
                .append(std::mem::take(&mut pending_chat_effects));
            let chat_effects_changed = apply_transport_chat_effects(
                &mut seed,
                &state.chat_effects,
                current_user_pubkey.as_deref(),
            )?;
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
                reconcile_transport_outbound_dispatches(&mut working_cache, &seed);
                reconcile_transport_relay_sync_cursors(&mut working_cache, &seed);
                let action_input =
                    enrich_runtime_sync_action_input(&seed, &working_cache, action_input);
                let next_state =
                    engine.apply_circle_action(&mut seed, &working_cache, &action_input)?;
                let mut next_state = next_state;
                let recovery_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
                let outbound_messages =
                    collect_transport_outbound_messages(&seed, &working_cache, Some(&action_input));
                let relay_sync_filters = relay_sync_filters(&seed, &action_input);
                let runtime_chat_effects = runtime_manager.sync_cache(
                    &working_cache,
                    &mut next_state.cache,
                    recovery_runtime_profiles,
                    Some(&action_input),
                    &outbound_messages,
                    &relay_sync_filters,
                )?;
                pending_chat_effects.append(runtime_chat_effects);
                next_state
                    .chat_effects
                    .append(std::mem::take(&mut pending_chat_effects));
                let chat_effects_changed = apply_transport_chat_effects(
                    &mut seed,
                    &next_state.chat_effects,
                    current_user_pubkey.as_deref(),
                )?;
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
                reconcile_transport_outbound_dispatches(&mut next_state.cache, &seed);
                reconcile_transport_relay_sync_cursors(&mut next_state.cache, &seed);
                merge_runtime_session_activities(&mut next_state.cache, &working_cache);
                working_cache = next_state.cache.clone();
                state = Some(next_state);
            }
            state.ok_or_else(|| "local recovery worker produced no transport state".to_string())?
        };

        let mut state = state;
        reconcile_transport_outbound_dispatches(&mut state.cache, &seed);
        reconcile_transport_relay_sync_cursors(&mut state.cache, &seed);

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
        let current_user_pubkey = self.resolve_current_user_pubkey();
        let mut pending_chat_effects = TransportChatEffects::default();
        normalize_seed_runtime_status(&mut seed, &previous_cache);
        let initial_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let (mut normalized_previous_cache, normalized_chat_effects) =
            self.normalize_runtime_cache(&previous_cache, &initial_runtime_profiles)?;
        pending_chat_effects.append(normalized_chat_effects);
        normalize_seed_runtime_status(&mut seed, &normalized_previous_cache);
        reconcile_transport_outbound_dispatches(&mut normalized_previous_cache, &seed);
        reconcile_transport_relay_sync_cursors(&mut normalized_previous_cache, &seed);
        let normalized_runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let input = enrich_runtime_sync_action_input(&seed, &normalized_previous_cache, input);
        validate_runtime_action(&input, &normalized_runtime_profiles)?;
        let mut state =
            engine.apply_circle_action(&mut seed, &normalized_previous_cache, &input)?;
        let runtime_profiles = runtime.build_profiles(&seed, runtime_options)?;
        let outbound_messages =
            collect_transport_outbound_messages(&seed, &normalized_previous_cache, Some(&input));
        let relay_sync_filters = relay_sync_filters(&seed, &input);
        let runtime_chat_effects = runtime_manager.sync_cache(
            &normalized_previous_cache,
            &mut state.cache,
            runtime_profiles,
            Some(&input),
            &outbound_messages,
            &relay_sync_filters,
        )?;
        pending_chat_effects.append(runtime_chat_effects);
        state
            .chat_effects
            .append(std::mem::take(&mut pending_chat_effects));
        let chat_effects_changed = apply_transport_chat_effects(
            &mut seed,
            &state.chat_effects,
            current_user_pubkey.as_deref(),
        )?;
        if chat_effects_changed {
            state.chat_effects = TransportChatEffects::default();
        }
        let (state_after_reconcile, reconciled_seed_changed) =
            reconcile_seed_and_transport_state(engine, &mut seed, state, chat_effects_changed)?;
        let _ = reconciled_seed_changed;
        let mut state = state_after_reconcile;
        reconcile_transport_outbound_dispatches(&mut state.cache, &seed);
        reconcile_transport_relay_sync_cursors(&mut state.cache, &seed);
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

    fn resolve_current_user_pubkey(&self) -> Option<String> {
        shell_auth::load_saved_shell_snapshot(&self.app_handle)
            .ok()
            .and_then(|shell| {
                shell
                    .auth_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.pubkey.as_deref())
                    .or_else(|| {
                        shell
                            .auth_session
                            .as_ref()
                            .and_then(|session| session.access.pubkey.as_deref())
                    })
                    .and_then(normalize_nostr_pubkey)
            })
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
            &[],
            &[],
        )?;
        Ok((normalized_cache, chat_effects))
    }
}

fn collect_transport_outbound_messages(
    seed: &ChatDomainSeed,
    cache: &TransportCache,
    action: Option<&TransportCircleActionInput>,
) -> Vec<TransportRuntimeOutboundMessage> {
    let allowed_circle_id = match action {
        Some(action)
            if matches!(
                action.action,
                TransportCircleAction::Sync | TransportCircleAction::SyncSessions
            ) =>
        {
            Some(action.circle_id.as_str())
        }
        Some(_) => return Vec::new(),
        None => None,
    };

    let dispatched_message_ids = cache
        .outbound_dispatches
        .iter()
        .filter(|dispatch| {
            allowed_circle_id
                .map(|circle_id| dispatch.circle_id == circle_id)
                .unwrap_or(true)
        })
        .map(|dispatch| (dispatch.session_id.as_str(), dispatch.message_id.as_str()))
        .collect::<HashSet<_>>();

    seed.sessions
        .iter()
        .filter(|session| {
            allowed_circle_id
                .map(|circle_id| session.circle_id == circle_id)
                .unwrap_or(true)
        })
        .flat_map(|session| {
            seed.message_store
                .get(&session.id)
                .into_iter()
                .flatten()
                .filter_map(|message| {
                    if !message_is_pending_local_outbound_publish(message) {
                        return None;
                    }

                    let signed_nostr_event = message.signed_nostr_event.clone()?;
                    if dispatched_message_ids.contains(&(session.id.as_str(), message.id.as_str()))
                    {
                        return None;
                    }

                    Some(TransportRuntimeOutboundMessage {
                        session_id: session.id.clone(),
                        message_id: message.id.clone(),
                        remote_id: message
                            .remote_id
                            .clone()
                            .unwrap_or_else(|| signed_nostr_event.event_id.clone()),
                        signed_nostr_event,
                    })
                })
        })
        .collect()
}

fn message_is_pending_local_outbound_publish(message: &MessageItem) -> bool {
    matches!(message.author, MessageAuthor::Me)
        && !matches!(
            message.sync_source,
            Some(MessageSyncSource::Relay | MessageSyncSource::System)
        )
        && matches!(
            message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        )
        && message.signed_nostr_event.is_some()
}

fn enrich_runtime_sync_action_input(
    seed: &ChatDomainSeed,
    cache: &TransportCache,
    mut input: TransportCircleActionInput,
) -> TransportCircleActionInput {
    input.sync_since_created_at = relay_sync_since_created_at(seed, cache, &input);
    input
}

fn relay_sync_filters(
    seed: &ChatDomainSeed,
    input: &TransportCircleActionInput,
) -> Vec<TransportRelaySyncFilter> {
    if !matches!(input.action, TransportCircleAction::Sync) {
        return Vec::new();
    }

    let contact_pubkeys = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey).map(|pubkey| (contact.id.as_str(), pubkey))
        })
        .collect::<HashMap<_, _>>();
    let mut filters = Vec::new();
    let mut direct_authors = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == input.circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Direct)
        })
        .filter_map(|session| session.contact_id.as_deref())
        .filter_map(|contact_id| contact_pubkeys.get(contact_id).cloned())
        .collect::<Vec<_>>();
    direct_authors.sort_unstable();
    direct_authors.dedup();
    if !direct_authors.is_empty() {
        filters.push(TransportRelaySyncFilter {
            authors: direct_authors,
            tagged_pubkeys: Vec::new(),
        });
    }

    filters.extend(
        seed.sessions
            .iter()
            .filter(|session| {
                session.circle_id == input.circle_id
                    && !session.archived.unwrap_or(false)
                    && matches!(session.kind, SessionKind::Group)
            })
            .filter_map(|session| {
                let group = seed
                    .groups
                    .iter()
                    .find(|group| group.session_id == session.id)?;
                let mut member_pubkeys = group
                    .members
                    .iter()
                    .filter_map(|member| contact_pubkeys.get(member.contact_id.as_str()).cloned())
                    .collect::<Vec<_>>();
                member_pubkeys.sort_unstable();
                member_pubkeys.dedup();
                if member_pubkeys.is_empty() {
                    return None;
                }

                Some(TransportRelaySyncFilter {
                    tagged_pubkeys: if member_pubkeys.len() >= 2 {
                        member_pubkeys.clone()
                    } else {
                        Vec::new()
                    },
                    authors: member_pubkeys,
                })
            }),
    );
    filters.sort_by(|left, right| {
        left.authors
            .cmp(&right.authors)
            .then(left.tagged_pubkeys.cmp(&right.tagged_pubkeys))
    });
    filters.dedup();
    filters
}

fn relay_sync_since_created_at(
    seed: &ChatDomainSeed,
    cache: &TransportCache,
    input: &TransportCircleActionInput,
) -> Option<u64> {
    if !matches!(input.action, TransportCircleAction::Sync) {
        return None;
    }

    relay_sync_cursor_created_at(cache, &input.circle_id)
        .or_else(|| latest_peer_relay_created_at(seed, &input.circle_id))
        .map(|created_at| created_at.saturating_sub(RELAY_SYNC_OVERLAP_SECS))
}

fn relay_sync_cursor_created_at(cache: &TransportCache, circle_id: &str) -> Option<u64> {
    cache
        .relay_sync_cursors
        .iter()
        .filter(|cursor| cursor.circle_id == circle_id)
        .map(|cursor| cursor.last_created_at)
        .max()
}

fn latest_peer_relay_created_at(seed: &ChatDomainSeed, circle_id: &str) -> Option<u64> {
    seed.sessions
        .iter()
        .filter(|session| session.circle_id == circle_id)
        .filter_map(|session| seed.message_store.get(&session.id))
        .flat_map(|messages| messages.iter())
        .filter(|message| {
            matches!(message.author, MessageAuthor::Peer)
                && matches!(message.sync_source, Some(MessageSyncSource::Relay))
        })
        .filter_map(|message| {
            message
                .signed_nostr_event
                .as_ref()
                .map(|event| event.created_at)
        })
        .max()
}

fn reconcile_transport_relay_sync_cursors(cache: &mut TransportCache, seed: &ChatDomainSeed) {
    let mut last_created_at_by_circle =
        cache
            .relay_sync_cursors
            .iter()
            .fold(HashMap::<String, u64>::new(), |mut acc, cursor| {
                acc.entry(cursor.circle_id.clone())
                    .and_modify(|last_created_at| {
                        *last_created_at = (*last_created_at).max(cursor.last_created_at);
                    })
                    .or_insert(cursor.last_created_at);
                acc
            });

    for circle in &seed.circles {
        if let Some(last_created_at) = latest_peer_relay_created_at(seed, &circle.id) {
            last_created_at_by_circle
                .entry(circle.id.clone())
                .and_modify(|cursor_created_at| {
                    *cursor_created_at = (*cursor_created_at).max(last_created_at);
                })
                .or_insert(last_created_at);
        }
    }

    cache.relay_sync_cursors = seed
        .circles
        .iter()
        .filter_map(|circle| {
            last_created_at_by_circle
                .get(&circle.id)
                .copied()
                .map(|last_created_at| TransportRelaySyncCursor {
                    circle_id: circle.id.clone(),
                    last_created_at,
                })
        })
        .collect();
}

fn reconcile_transport_outbound_dispatches(cache: &mut TransportCache, seed: &ChatDomainSeed) {
    let session_circle_ids = seed
        .sessions
        .iter()
        .map(|session| (session.id.as_str(), session.circle_id.as_str()))
        .collect::<HashMap<_, _>>();
    let runtime_generations = current_runtime_generation_by_circle(cache);
    let valid_dispatches = seed
        .message_store
        .iter()
        .flat_map(|(session_id, messages)| {
            let circle_id = session_circle_ids.get(session_id.as_str()).copied();
            messages.iter().filter_map(move |message| {
                let circle_id = circle_id?;
                if !message_is_pending_local_outbound_publish(message) {
                    return None;
                }
                let signed_nostr_event = message.signed_nostr_event.as_ref()?;

                Some((
                    circle_id.to_string(),
                    session_id.clone(),
                    message.id.clone(),
                    signed_nostr_event.event_id.clone(),
                ))
            })
        })
        .collect::<HashSet<_>>();

    cache.outbound_dispatches.retain(|dispatch| {
        valid_dispatches.contains(&(
            dispatch.circle_id.clone(),
            dispatch.session_id.clone(),
            dispatch.message_id.clone(),
            dispatch.event_id.clone(),
        )) && runtime_generations
            .get(dispatch.circle_id.as_str())
            .map(|generation| *generation == dispatch.runtime_generation)
            .unwrap_or(true)
    });
}

fn current_runtime_generation_by_circle(cache: &TransportCache) -> HashMap<String, u32> {
    if !cache.runtime_registry.is_empty() {
        return cache
            .runtime_registry
            .iter()
            .map(|entry| (entry.circle_id.clone(), entry.generation))
            .collect();
    }

    cache
        .runtime_sessions
        .iter()
        .map(|session| (session.circle_id.clone(), session.generation))
        .collect()
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
                    && message.signed_nostr_event.is_none()
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
    current_user_pubkey: Option<&str>,
) -> Result<bool, String> {
    if chat_effects.is_empty() {
        return Ok(false);
    }

    let mut changed = false;

    for merge in &chat_effects.remote_message_merges {
        if merge.messages.is_empty() {
            continue;
        }

        for routed_merge in route_remote_message_merges(seed, merge, current_user_pubkey) {
            if routed_merge.messages.is_empty() {
                continue;
            }

            merge_remote_messages_into_seed(seed, &routed_merge.session_id, routed_merge.messages)?;
            changed = true;
        }
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

fn route_remote_message_merges(
    seed: &ChatDomainSeed,
    merge: &MergeRemoteMessagesInput,
    current_user_pubkey: Option<&str>,
) -> Vec<MergeRemoteMessagesInput> {
    let mut ordered_session_ids = Vec::<String>::new();
    let mut messages_by_session = HashMap::<String, Vec<MessageItem>>::new();

    for message in &merge.messages {
        let normalized_message =
            normalize_relay_message_for_current_user(message, current_user_pubkey);
        let target_session_id = existing_message_session_id(seed, &normalized_message)
            .or_else(|| {
                resolve_remote_message_target_session(
                    seed,
                    &merge.session_id,
                    &normalized_message,
                    current_user_pubkey,
                )
            })
            .unwrap_or_else(|| merge.session_id.clone());
        if !messages_by_session.contains_key(&target_session_id) {
            ordered_session_ids.push(target_session_id.clone());
        }
        messages_by_session
            .entry(target_session_id)
            .or_default()
            .push(normalized_message);
    }

    ordered_session_ids
        .into_iter()
        .filter_map(|session_id| {
            messages_by_session
                .remove(&session_id)
                .map(|messages| MergeRemoteMessagesInput {
                    session_id,
                    messages,
                })
        })
        .collect()
}

fn existing_message_session_id(seed: &ChatDomainSeed, message: &MessageItem) -> Option<String> {
    seed.message_store
        .iter()
        .find_map(|(session_id, messages)| {
            messages
                .iter()
                .any(|existing_message| same_message_identity(existing_message, message))
                .then(|| session_id.clone())
        })
}

fn same_message_identity(existing: &MessageItem, incoming: &MessageItem) -> bool {
    if existing.id == incoming.id {
        return true;
    }

    matches!(
        (existing.remote_id.as_deref(), incoming.remote_id.as_deref()),
        (Some(existing_remote_id), Some(incoming_remote_id))
            if existing_remote_id == incoming_remote_id
    )
}

fn resolve_remote_message_target_session(
    seed: &ChatDomainSeed,
    fallback_session_id: &str,
    message: &MessageItem,
    current_user_pubkey: Option<&str>,
) -> Option<String> {
    let sender_pubkey = message_sender_pubkey(message)?;
    let fallback_circle_id = seed
        .sessions
        .iter()
        .find(|session| session.id == fallback_session_id)
        .map(|session| session.circle_id.as_str())?;
    let raw_p_tag_count = raw_p_tag_count_for_remote_message(message);
    if current_user_pubkey.is_some_and(|pubkey| pubkey == sender_pubkey) {
        return resolve_group_self_message_target_session(seed, fallback_circle_id, message)
            .or_else(|| {
                (raw_p_tag_count < 2)
                    .then(|| {
                        resolve_direct_self_message_target_session(
                            seed,
                            fallback_circle_id,
                            message,
                        )
                    })
                    .flatten()
            })
            .or_else(|| {
                (raw_p_tag_count == 0)
                    .then(|| resolve_self_chat_target_session(seed, fallback_circle_id))
                    .flatten()
            });
    }

    if !matches!(message.author, MessageAuthor::Peer) {
        return None;
    }

    resolve_group_peer_message_target_session(seed, fallback_circle_id, &sender_pubkey, message)
        .or_else(|| {
            (raw_p_tag_count < 2)
                .then(|| {
                    resolve_direct_peer_message_target_session(
                        seed,
                        fallback_circle_id,
                        &sender_pubkey,
                    )
                })
                .flatten()
        })
}

fn normalize_relay_message_for_current_user(
    message: &MessageItem,
    current_user_pubkey: Option<&str>,
) -> MessageItem {
    let mut normalized_message = message.clone();
    if !matches!(
        normalized_message.sync_source,
        Some(MessageSyncSource::Relay)
    ) {
        return normalized_message;
    }
    let Some(current_user_pubkey) = current_user_pubkey else {
        return normalized_message;
    };
    let Some(sender_pubkey) = message_sender_pubkey(message) else {
        return normalized_message;
    };
    if sender_pubkey != current_user_pubkey {
        return normalized_message;
    }

    normalized_message.author = MessageAuthor::Me;
    if normalized_message.delivery_status.is_none() {
        normalized_message.delivery_status = Some(MessageDeliveryStatus::Sent);
    }
    normalized_message
}

fn message_sender_pubkey(message: &MessageItem) -> Option<String> {
    message
        .signed_nostr_event
        .as_ref()
        .and_then(|event| normalize_nostr_pubkey(&event.pubkey))
}

fn resolve_direct_peer_message_target_session(
    seed: &ChatDomainSeed,
    circle_id: &str,
    sender_pubkey: &str,
) -> Option<String> {
    let matching_contact_ids = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey)
                .filter(|pubkey| pubkey == sender_pubkey)
                .map(|_| contact.id.as_str())
        })
        .collect::<HashSet<_>>();
    if matching_contact_ids.is_empty() {
        return None;
    }

    seed.sessions
        .iter()
        .find(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Direct)
                && session
                    .contact_id
                    .as_deref()
                    .is_some_and(|contact_id| matching_contact_ids.contains(contact_id))
        })
        .map(|session| session.id.clone())
}

fn resolve_direct_self_message_target_session(
    seed: &ChatDomainSeed,
    circle_id: &str,
    message: &MessageItem,
) -> Option<String> {
    let tagged_pubkeys = tagged_pubkeys_for_remote_message(message);
    if tagged_pubkeys.is_empty() {
        return None;
    }

    let candidate_session_ids = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Direct)
        })
        .filter_map(|session| {
            let contact_id = session.contact_id.as_deref()?;
            let contact = seed
                .contacts
                .iter()
                .find(|contact| contact.id == contact_id)?;
            let contact_pubkey = normalize_nostr_pubkey(&contact.pubkey)?;
            tagged_pubkeys
                .contains(&contact_pubkey)
                .then(|| session.id.clone())
        })
        .collect::<Vec<_>>();

    if candidate_session_ids.len() == 1 {
        candidate_session_ids.into_iter().next()
    } else {
        None
    }
}

fn resolve_group_peer_message_target_session(
    seed: &ChatDomainSeed,
    circle_id: &str,
    sender_pubkey: &str,
    message: &MessageItem,
) -> Option<String> {
    let tagged_pubkeys = tagged_pubkeys_for_remote_message(message);
    if tagged_pubkeys.is_empty() {
        return None;
    }

    let contact_pubkeys = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey).map(|pubkey| (contact.id.as_str(), pubkey))
        })
        .collect::<HashMap<_, _>>();
    let candidate_session_ids = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Group)
        })
        .filter_map(|session| {
            let group = seed
                .groups
                .iter()
                .find(|group| group.session_id == session.id)?;
            group_message_matches_session(group, &contact_pubkeys, sender_pubkey, &tagged_pubkeys)
                .then(|| session.id.clone())
        })
        .collect::<Vec<_>>();

    if candidate_session_ids.len() == 1 {
        candidate_session_ids.into_iter().next()
    } else {
        None
    }
}

fn resolve_group_self_message_target_session(
    seed: &ChatDomainSeed,
    circle_id: &str,
    message: &MessageItem,
) -> Option<String> {
    let tagged_pubkeys = tagged_pubkeys_for_remote_message(message);
    if tagged_pubkeys.is_empty() {
        return None;
    }

    let contact_pubkeys = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey).map(|pubkey| (contact.id.as_str(), pubkey))
        })
        .collect::<HashMap<_, _>>();
    let candidate_session_ids = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Group)
        })
        .filter_map(|session| {
            let group = seed
                .groups
                .iter()
                .find(|group| group.session_id == session.id)?;
            group_self_message_matches_session(group, &contact_pubkeys, &tagged_pubkeys)
                .then(|| session.id.clone())
        })
        .collect::<Vec<_>>();

    if candidate_session_ids.len() == 1 {
        candidate_session_ids.into_iter().next()
    } else {
        None
    }
}

fn group_message_matches_session(
    group: &crate::domain::chat::GroupProfile,
    contact_pubkeys: &HashMap<&str, String>,
    sender_pubkey: &str,
    tagged_pubkeys: &HashSet<String>,
) -> bool {
    let member_pubkeys = group
        .members
        .iter()
        .filter_map(|member| contact_pubkeys.get(member.contact_id.as_str()).cloned())
        .collect::<HashSet<_>>();
    if member_pubkeys.is_empty() || !member_pubkeys.contains(sender_pubkey) {
        return false;
    }

    member_pubkeys
        .iter()
        .filter(|pubkey| pubkey.as_str() != sender_pubkey)
        .all(|pubkey| tagged_pubkeys.contains(pubkey))
}

fn group_self_message_matches_session(
    group: &crate::domain::chat::GroupProfile,
    contact_pubkeys: &HashMap<&str, String>,
    tagged_pubkeys: &HashSet<String>,
) -> bool {
    let member_pubkeys = group
        .members
        .iter()
        .filter_map(|member| contact_pubkeys.get(member.contact_id.as_str()).cloned())
        .collect::<HashSet<_>>();
    !member_pubkeys.is_empty()
        && member_pubkeys
            .iter()
            .all(|pubkey| tagged_pubkeys.contains(pubkey))
}

fn resolve_self_chat_target_session(seed: &ChatDomainSeed, circle_id: &str) -> Option<String> {
    let candidate_session_ids = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::SelfChat)
        })
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();

    if candidate_session_ids.len() == 1 {
        candidate_session_ids.into_iter().next()
    } else {
        None
    }
}

fn tagged_pubkeys_for_remote_message(message: &MessageItem) -> HashSet<String> {
    message
        .signed_nostr_event
        .as_ref()
        .into_iter()
        .flat_map(|event| event.tags.iter())
        .filter_map(|tag| {
            if tag.first().map(String::as_str) != Some("p") {
                return None;
            }

            tag.get(1).and_then(|value| normalize_nostr_pubkey(value))
        })
        .collect()
}

fn raw_p_tag_count_for_remote_message(message: &MessageItem) -> usize {
    message
        .signed_nostr_event
        .as_ref()
        .into_iter()
        .flat_map(|event| event.tags.iter())
        .filter(|tag| tag.first().map(String::as_str) == Some("p"))
        .count()
}

fn normalize_nostr_pubkey(value: &str) -> Option<String> {
    NostrPublicKey::parse(value.trim())
        .ok()
        .map(|pubkey| pubkey.to_hex())
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
        CircleItem, CircleType, ContactItem, GroupMember, GroupProfile, GroupRole,
        MergeRemoteMessagesInput, MessageAuthor, MessageDeliveryStatus, MessageItem, MessageKind,
        MessageSyncSource, SessionItem, SessionKind, SignedNostrEvent,
    };
    use crate::domain::transport::{
        TransportActivityKind, TransportActivityLevel, TransportCircleAction,
        TransportCircleActionInput, TransportOutboundDispatch, TransportRelaySyncFilter,
        TransportRuntimeAdapterKind, TransportRuntimeDesiredState, TransportRuntimeLaunchResult,
        TransportRuntimeLaunchStatus, TransportRuntimeQueueState, TransportRuntimeRecoveryPolicy,
        TransportRuntimeRegistryEntry, TransportRuntimeState,
    };
    use crate::domain::transport_repository::TransportRelaySyncCursor;
    use secp256k1::{Secp256k1, SecretKey};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::str::FromStr;
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

    fn direct_session_with_contact(id: &str, circle_id: &str, contact_id: &str) -> SessionItem {
        SessionItem {
            contact_id: Some(contact_id.into()),
            ..direct_session(id, circle_id)
        }
    }

    fn group_session(id: &str, circle_id: &str, name: &str) -> SessionItem {
        SessionItem {
            id: id.into(),
            circle_id: circle_id.into(),
            name: name.into(),
            initials: "G".into(),
            subtitle: "test".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::Group,
            category: "groups".into(),
            members: Some(3),
            contact_id: None,
            archived: Some(false),
        }
    }

    fn self_session(id: &str, circle_id: &str) -> SessionItem {
        SessionItem {
            id: id.into(),
            circle_id: circle_id.into(),
            name: "Note to Self".into(),
            initials: "ME".into(),
            subtitle: "Private note space".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::SelfChat,
            category: "system".into(),
            members: None,
            contact_id: None,
            archived: Some(false),
        }
    }

    fn group_profile(session_id: &str, member_contact_ids: &[&str]) -> GroupProfile {
        GroupProfile {
            session_id: session_id.into(),
            name: session_id.into(),
            description: "test".into(),
            members: member_contact_ids
                .iter()
                .map(|contact_id| GroupMember {
                    contact_id: (*contact_id).into(),
                    role: Some(GroupRole::Member),
                })
                .collect(),
            muted: None,
        }
    }

    fn contact(id: &str, pubkey: &str) -> ContactItem {
        ContactItem {
            id: id.into(),
            name: id.into(),
            initials: "C".into(),
            handle: format!("@{id}"),
            pubkey: pubkey.into(),
            subtitle: "test".into(),
            bio: "test".into(),
            online: Some(false),
            blocked: Some(false),
        }
    }

    fn valid_sender_pubkey_hex() -> String {
        valid_pubkey_hex("1111111111111111111111111111111111111111111111111111111111111111")
    }

    fn valid_group_member_pubkey_hex() -> String {
        valid_pubkey_hex("2222222222222222222222222222222222222222222222222222222222222222")
    }

    fn valid_unknown_tag_pubkey_hex() -> String {
        valid_pubkey_hex("3333333333333333333333333333333333333333333333333333333333333333")
    }

    fn valid_pubkey_hex(secret_key_hex: &str) -> String {
        let secret_key =
            SecretKey::from_str(secret_key_hex).expect("valid test secret key should parse");
        let secp = Secp256k1::new();
        let (pubkey, _) = secret_key.x_only_public_key(&secp);
        pubkey
            .serialize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
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
            signed_nostr_event: None,
            reply_to: None,
        }
    }

    fn signed_text_message(
        id: &str,
        delivery_status: Option<MessageDeliveryStatus>,
        remote_id: &str,
    ) -> MessageItem {
        MessageItem {
            remote_id: Some(remote_id.into()),
            signed_nostr_event: Some(SignedNostrEvent {
                event_id: remote_id.into(),
                pubkey: "02b4631d6f1d6659d8e7a0f4d1f56ea74413c5fc11d16f55b3e25a03e353dd1510".into(),
                created_at: 1_735_689_600,
                kind: 1,
                tags: Vec::new(),
                content: id.into(),
                signature: "c".repeat(128),
            }),
            ..text_message(id, MessageAuthor::Me, delivery_status)
        }
    }

    fn signed_relay_self_message(id: &str, remote_id: &str) -> MessageItem {
        let mut message = signed_text_message(id, None, remote_id);
        message.sync_source = Some(MessageSyncSource::Relay);
        message
    }

    fn legacy_signed_pending_local_message(id: &str, remote_id: &str) -> MessageItem {
        let mut message = signed_text_message(id, Some(MessageDeliveryStatus::Sending), remote_id);
        message.sync_source = None;
        message
    }

    fn inbound_relay_message(id: &str, sender_pubkey: &str, body: &str) -> MessageItem {
        MessageItem {
            id: id.into(),
            kind: MessageKind::Text,
            author: MessageAuthor::Peer,
            body: body.into(),
            time: "now".into(),
            meta: None,
            delivery_status: None,
            remote_id: Some(id.into()),
            sync_source: Some(MessageSyncSource::Relay),
            acked_at: None,
            signed_nostr_event: Some(SignedNostrEvent {
                event_id: id.into(),
                pubkey: sender_pubkey.into(),
                created_at: 1_735_689_600,
                kind: 1,
                tags: Vec::new(),
                content: body.into(),
                signature: "d".repeat(128),
            }),
            reply_to: None,
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
    fn active_runtime_leaves_signed_local_messages_pending_until_runtime_receipt() {
        let mut seed = seed(CircleStatus::Connecting, "--");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![signed_text_message(
                "signed-sending",
                Some(MessageDeliveryStatus::Sending),
                "event-signed-sending",
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
        assert!(!message_changed);
        let session_messages = &seed.message_store["session-1"];
        assert!(matches!(
            session_messages[0].delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(
            session_messages[0].remote_id.as_deref(),
            Some("event-signed-sending")
        );
        assert_eq!(session_messages[0].acked_at, None);
    }

    #[test]
    fn collect_transport_outbound_messages_filters_to_undispatched_signed_local_messages() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.circles.push(CircleItem {
            id: "circle-2".into(),
            name: "Circle 2".into(),
            relay: "wss://relay-2.example.com".into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Open,
            latency: "18 ms".into(),
            description: "test".into(),
        });
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.sessions.push(direct_session("session-2", "circle-2"));
        seed.message_store.insert(
            "session-1".into(),
            vec![
                signed_text_message(
                    "message-queued",
                    Some(MessageDeliveryStatus::Sending),
                    "event-queued",
                ),
                signed_text_message(
                    "message-sent",
                    Some(MessageDeliveryStatus::Sent),
                    "event-sent",
                ),
                signed_text_message(
                    "message-failed",
                    Some(MessageDeliveryStatus::Failed),
                    "event-failed",
                ),
            ],
        );
        seed.message_store.insert(
            "session-2".into(),
            vec![signed_text_message(
                "message-other-circle",
                Some(MessageDeliveryStatus::Sending),
                "event-other-circle",
            )],
        );

        let cache = TransportCache {
            outbound_dispatches: vec![TransportOutboundDispatch {
                circle_id: "circle-1".into(),
                session_id: "session-1".into(),
                message_id: "message-sent".into(),
                remote_id: "event-sent".into(),
                event_id: "event-sent".into(),
                runtime_generation: 1,
                request_id: "sync:circle-1:1".into(),
                dispatched_at: "now".into(),
            }],
            ..TransportCache::default()
        };

        let outbound_messages = collect_transport_outbound_messages(
            &seed,
            &cache,
            Some(&TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            }),
        );

        assert_eq!(outbound_messages.len(), 1);
        assert_eq!(outbound_messages[0].session_id, "session-1");
        assert_eq!(outbound_messages[0].message_id, "message-queued");
        assert_eq!(outbound_messages[0].remote_id, "event-queued");
    }

    #[test]
    fn collect_transport_outbound_messages_ignores_self_authored_relay_echoes() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![
                signed_text_message(
                    "message-local",
                    Some(MessageDeliveryStatus::Sending),
                    "event-local",
                ),
                signed_relay_self_message("message-relay-self", "event-relay-self"),
            ],
        );

        let outbound_messages =
            collect_transport_outbound_messages(&seed, &TransportCache::default(), None);

        assert_eq!(outbound_messages.len(), 1);
        assert_eq!(outbound_messages[0].message_id, "message-local");
        assert_eq!(outbound_messages[0].remote_id, "event-local");
    }

    #[test]
    fn collect_transport_outbound_messages_keeps_legacy_pending_local_signed_messages() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![legacy_signed_pending_local_message(
                "message-legacy-local",
                "event-legacy-local",
            )],
        );

        let outbound_messages =
            collect_transport_outbound_messages(&seed, &TransportCache::default(), None);

        assert_eq!(outbound_messages.len(), 1);
        assert_eq!(outbound_messages[0].message_id, "message-legacy-local");
        assert_eq!(outbound_messages[0].remote_id, "event-legacy-local");
    }

    #[test]
    fn collect_transport_outbound_messages_without_action_includes_all_circles() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.circles.push(CircleItem {
            id: "circle-2".into(),
            name: "Circle 2".into(),
            relay: "wss://relay-2.example.com".into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Open,
            latency: "18 ms".into(),
            description: "test".into(),
        });
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.sessions.push(direct_session("session-2", "circle-2"));
        seed.message_store.insert(
            "session-1".into(),
            vec![signed_text_message(
                "message-circle-1",
                Some(MessageDeliveryStatus::Sending),
                "event-circle-1",
            )],
        );
        seed.message_store.insert(
            "session-2".into(),
            vec![signed_text_message(
                "message-circle-2",
                Some(MessageDeliveryStatus::Sending),
                "event-circle-2",
            )],
        );

        let outbound_messages =
            collect_transport_outbound_messages(&seed, &TransportCache::default(), None);

        assert_eq!(outbound_messages.len(), 2);
        assert!(outbound_messages
            .iter()
            .any(|message| message.session_id == "session-1"
                && message.remote_id == "event-circle-1"));
        assert!(outbound_messages
            .iter()
            .any(|message| message.session_id == "session-2"
                && message.remote_id == "event-circle-2"));
    }

    #[test]
    fn relay_sync_since_created_at_uses_latest_peer_relay_event_with_overlap() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.sessions.push(direct_session("session-2", "circle-1"));
        let mut older_relay_peer =
            inbound_relay_message("relay-event-1", &sender_pubkey, "older relay peer");
        older_relay_peer
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .created_at = 1_735_689_600;
        let mut local_signed_message = signed_text_message(
            "local-signed-message",
            Some(MessageDeliveryStatus::Sent),
            "event-local-signed",
        );
        local_signed_message
            .signed_nostr_event
            .as_mut()
            .expect("local signed message should have signed event")
            .created_at = 1_735_690_000;
        seed.message_store.insert(
            "session-1".into(),
            vec![older_relay_peer, local_signed_message],
        );
        let mut latest_relay_peer =
            inbound_relay_message("relay-event-2", &sender_pubkey, "latest relay peer");
        latest_relay_peer
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .created_at = 1_735_689_900;
        seed.message_store
            .insert("session-2".into(), vec![latest_relay_peer]);

        let since = relay_sync_since_created_at(
            &seed,
            &TransportCache::default(),
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
        );

        assert_eq!(since, Some(1_735_689_600));
    }

    #[test]
    fn relay_sync_filters_include_direct_authors_and_group_member_p_tags() {
        let direct_pubkey = valid_sender_pubkey_hex();
        let group_sender_pubkey = valid_group_member_pubkey_hex();
        let other_group_pubkey =
            valid_pubkey_hex("4444444444444444444444444444444444444444444444444444444444444444");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts
            .push(contact("direct-contact", &direct_pubkey));
        seed.contacts
            .push(contact("bob-contact", &group_sender_pubkey));
        seed.contacts
            .push(contact("carol-contact", &other_group_pubkey));
        seed.sessions.push(direct_session_with_contact(
            "direct-1",
            "circle-1",
            "direct-contact",
        ));
        seed.sessions
            .push(group_session("group-1", "circle-1", "Design Crew"));
        seed.groups
            .push(group_profile("group-1", &["bob-contact", "carol-contact"]));

        let filters = relay_sync_filters(
            &seed,
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
        );
        let mut expected_group_pubkeys = vec![group_sender_pubkey, other_group_pubkey];
        expected_group_pubkeys.sort_unstable();

        assert_eq!(filters.len(), 2);
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: vec![direct_pubkey],
            tagged_pubkeys: Vec::new(),
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: expected_group_pubkeys.clone(),
            tagged_pubkeys: expected_group_pubkeys,
        }));
    }

    #[test]
    fn relay_sync_filters_fall_back_to_author_only_for_single_member_groups() {
        let group_member_pubkey = valid_group_member_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts
            .push(contact("bob-contact", &group_member_pubkey));
        seed.sessions
            .push(group_session("group-1", "circle-1", "Single Member Group"));
        seed.groups.push(group_profile("group-1", &["bob-contact"]));

        let filters = relay_sync_filters(
            &seed,
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
        );

        assert_eq!(
            filters,
            vec![TransportRelaySyncFilter {
                authors: vec![group_member_pubkey],
                tagged_pubkeys: Vec::new(),
            }]
        );
    }

    #[test]
    fn relay_sync_since_created_at_prefers_persisted_cursor_over_seed_history() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        let mut relay_peer =
            inbound_relay_message("relay-event-1", &sender_pubkey, "older relay peer");
        relay_peer
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .created_at = 1_735_689_600;
        seed.message_store
            .insert("session-1".into(), vec![relay_peer]);

        let since = relay_sync_since_created_at(
            &seed,
            &TransportCache {
                relay_sync_cursors: vec![TransportRelaySyncCursor {
                    circle_id: "circle-1".into(),
                    last_created_at: 1_735_690_200,
                }],
                ..TransportCache::default()
            },
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
        );

        assert_eq!(since, Some(1_735_689_900));
    }

    #[test]
    fn reconcile_transport_relay_sync_cursors_backfills_and_advances_from_seed() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.circles.push(CircleItem {
            id: "circle-2".into(),
            name: "Circle 2".into(),
            relay: "wss://relay-2.example.com".into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Open,
            latency: "18 ms".into(),
            description: "test".into(),
        });
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.sessions.push(direct_session("session-2", "circle-2"));
        let mut circle_1_message =
            inbound_relay_message("relay-event-1", &sender_pubkey, "circle 1 relay peer");
        circle_1_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .created_at = 1_735_689_700;
        let mut circle_2_message =
            inbound_relay_message("relay-event-2", &sender_pubkey, "circle 2 relay peer");
        circle_2_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .created_at = 1_735_690_100;
        seed.message_store
            .insert("session-1".into(), vec![circle_1_message]);
        seed.message_store
            .insert("session-2".into(), vec![circle_2_message]);

        let mut cache = TransportCache {
            relay_sync_cursors: vec![
                TransportRelaySyncCursor {
                    circle_id: "circle-1".into(),
                    last_created_at: 1_735_689_650,
                },
                TransportRelaySyncCursor {
                    circle_id: "circle-ghost".into(),
                    last_created_at: 42,
                },
            ],
            ..TransportCache::default()
        };

        reconcile_transport_relay_sync_cursors(&mut cache, &seed);

        assert_eq!(
            cache.relay_sync_cursors,
            vec![
                TransportRelaySyncCursor {
                    circle_id: "circle-1".into(),
                    last_created_at: 1_735_689_700,
                },
                TransportRelaySyncCursor {
                    circle_id: "circle-2".into(),
                    last_created_at: 1_735_690_100,
                },
            ]
        );
    }

    #[test]
    fn sqlite_transport_repository_roundtrips_relay_sync_cursors() {
        let guard = test_app();
        let repository = SqliteTransportRepository::new(guard.app.handle());
        let expected = vec![
            TransportRelaySyncCursor {
                circle_id: "circle-1".into(),
                last_created_at: 1_735_689_700,
            },
            TransportRelaySyncCursor {
                circle_id: "circle-2".into(),
                last_created_at: 1_735_690_100,
            },
        ];

        repository
            .save_transport_cache(TransportCache {
                relay_sync_cursors: expected.clone(),
                ..TransportCache::default()
            })
            .expect("transport cache should save");
        let cache = repository
            .load_transport_cache()
            .expect("transport cache should load");

        assert_eq!(cache.relay_sync_cursors, expected);
    }

    #[test]
    fn reconcile_transport_outbound_dispatches_drops_missing_replaced_and_stale_generation_records()
    {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![
                signed_text_message(
                    "message-queued",
                    Some(MessageDeliveryStatus::Sending),
                    "event-queued",
                ),
                signed_text_message(
                    "message-sent",
                    Some(MessageDeliveryStatus::Sent),
                    "event-sent",
                ),
            ],
        );
        let mut cache = TransportCache {
            runtime_registry: vec![runtime_entry(TransportRuntimeState::Active)],
            outbound_dispatches: vec![
                TransportOutboundDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-queued".into(),
                    remote_id: "event-queued".into(),
                    event_id: "event-queued".into(),
                    runtime_generation: 1,
                    request_id: "sync:circle-1:1".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-sent".into(),
                    remote_id: "event-sent".into(),
                    event_id: "event-sent".into(),
                    runtime_generation: 1,
                    request_id: "sync:circle-1:sent".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-queued".into(),
                    remote_id: "event-queued".into(),
                    event_id: "event-queued".into(),
                    runtime_generation: 0,
                    request_id: "sync:circle-1:stale-generation".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-stale".into(),
                    remote_id: "event-stale".into(),
                    event_id: "event-stale".into(),
                    runtime_generation: 1,
                    request_id: "sync:circle-1:2".into(),
                    dispatched_at: "now".into(),
                },
            ],
            ..TransportCache::default()
        };

        reconcile_transport_outbound_dispatches(&mut cache, &seed);

        assert_eq!(cache.outbound_dispatches.len(), 1);
        assert_eq!(cache.outbound_dispatches[0].message_id, "message-queued");
        assert_eq!(cache.outbound_dispatches[0].event_id, "event-queued");
    }

    #[test]
    fn apply_transport_chat_effects_routes_inbound_peer_message_to_matching_direct_session() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_relay_message(
                    "relay-event-1",
                    &sender_pubkey,
                    "hello from alice via relay",
                )],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(&mut seed, &chat_effects, None)
            .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-alice")
            .expect("matching direct session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-event-1");
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "session-alice")
            .expect("missing routed session");
        assert_eq!(session.subtitle, "hello from alice via relay");
        assert_eq!(session.unread_count, Some(1));
    }

    #[test]
    fn apply_transport_chat_effects_does_not_route_peer_message_across_circles() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.circles.push(CircleItem {
            id: "circle-2".into(),
            name: "Circle 2".into(),
            relay: "wss://relay-2.example.com".into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Open,
            latency: "18 ms".into(),
            description: "test".into(),
        });
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice-circle-2",
            "circle-2",
            "alice-contact",
        ));
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_relay_message(
                    "relay-event-2",
                    &sender_pubkey,
                    "stay inside circle-1",
                )],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(&mut seed, &chat_effects, None)
            .expect("chat effects apply");

        assert!(changed);
        let original_messages = seed
            .message_store
            .get("session-primary")
            .expect("fallback session should retain unmatched circle message");
        assert_eq!(original_messages.len(), 1);
        assert_eq!(original_messages[0].id, "relay-event-2");
        assert!(seed
            .message_store
            .get("session-alice-circle-2")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_routes_inbound_peer_message_to_matching_group_session() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let other_member_pubkey = valid_group_member_pubkey_hex();
        let unknown_tag_pubkey = valid_unknown_tag_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.contacts
            .push(contact("bob-contact", &other_member_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions
            .push(group_session("session-design", "circle-1", "Design Circle"));
        seed.groups.push(group_profile(
            "session-design",
            &["alice-contact", "bob-contact"],
        ));
        let mut inbound_message =
            inbound_relay_message("relay-group-1", &sender_pubkey, "hello design circle");
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![
            vec!["p".into(), other_member_pubkey],
            vec!["p".into(), unknown_tag_pubkey],
        ];
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(&mut seed, &chat_effects, None)
            .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-design")
            .expect("matching group session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-group-1");
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "session-design")
            .expect("missing routed group session");
        assert_eq!(session.subtitle, "hello design circle");
        assert_eq!(session.unread_count, Some(1));
    }

    #[test]
    fn apply_transport_chat_effects_prefers_group_session_over_direct_session_for_group_tags() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let other_member_pubkey = valid_group_member_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.contacts
            .push(contact("bob-contact", &other_member_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        seed.sessions
            .push(group_session("session-design", "circle-1", "Design Circle"));
        seed.groups.push(group_profile(
            "session-design",
            &["alice-contact", "bob-contact"],
        ));
        let mut inbound_message =
            inbound_relay_message("relay-group-2", &sender_pubkey, "route to group first");
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), other_member_pubkey]];
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(&mut seed, &chat_effects, None)
            .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-alice")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-design")
            .expect("matching group session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-group-2");
    }

    #[test]
    fn apply_transport_chat_effects_routes_self_authored_relay_message_to_matching_direct_session()
    {
        let current_user_pubkey = valid_sender_pubkey_hex();
        let contact_pubkey = valid_group_member_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("bob-contact", &contact_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-bob",
            "circle-1",
            "bob-contact",
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-self-direct-1",
            &current_user_pubkey,
            "sent from another device",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), contact_pubkey]];
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(
            &mut seed,
            &chat_effects,
            Some(current_user_pubkey.as_str()),
        )
        .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-bob")
            .expect("matching direct session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-self-direct-1");
        assert!(matches!(routed_messages[0].author, MessageAuthor::Me));
        assert!(matches!(
            routed_messages[0].sync_source,
            Some(MessageSyncSource::Relay)
        ));
        assert!(matches!(
            routed_messages[0].delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "session-bob")
            .expect("missing routed direct session");
        assert_eq!(session.subtitle, "sent from another device");
        assert_eq!(session.unread_count, None);
    }

    #[test]
    fn apply_transport_chat_effects_routes_self_authored_relay_message_to_matching_group_session() {
        let current_user_pubkey = valid_sender_pubkey_hex();
        let bob_pubkey = valid_group_member_pubkey_hex();
        let carol_pubkey =
            valid_pubkey_hex("4444444444444444444444444444444444444444444444444444444444444444");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("bob-contact", &bob_pubkey));
        seed.contacts.push(contact("carol-contact", &carol_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions
            .push(group_session("session-design", "circle-1", "Design Circle"));
        seed.groups.push(group_profile(
            "session-design",
            &["bob-contact", "carol-contact"],
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-self-group-1",
            &current_user_pubkey,
            "group update from my laptop",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), bob_pubkey], vec!["p".into(), carol_pubkey]];
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(
            &mut seed,
            &chat_effects,
            Some(current_user_pubkey.as_str()),
        )
        .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-design")
            .expect("matching group session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-self-group-1");
        assert!(matches!(routed_messages[0].author, MessageAuthor::Me));
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "session-design")
            .expect("missing routed group session");
        assert_eq!(session.subtitle, "group update from my laptop");
        assert_eq!(session.unread_count, None);
    }

    #[test]
    fn apply_transport_chat_effects_routes_self_authored_relay_message_to_self_chat_session() {
        let current_user_pubkey = valid_sender_pubkey_hex();
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions
            .push(self_session("self-circle-1", "circle-1"));
        let inbound_message = inbound_relay_message(
            "relay-self-note-1",
            &current_user_pubkey,
            "note to self from another device",
        );
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(
            &mut seed,
            &chat_effects,
            Some(current_user_pubkey.as_str()),
        )
        .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("self-circle-1")
            .expect("self session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-self-note-1");
        assert!(matches!(routed_messages[0].author, MessageAuthor::Me));
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "self-circle-1")
            .expect("missing self session");
        assert_eq!(session.subtitle, "note to self from another device");
        assert_eq!(session.unread_count, None);
    }

    #[test]
    fn apply_transport_chat_effects_prefers_existing_message_session_over_self_chat_fallback() {
        let current_user_pubkey = valid_sender_pubkey_hex();
        let bob_pubkey = valid_group_member_pubkey_hex();
        let carol_pubkey =
            valid_pubkey_hex("4444444444444444444444444444444444444444444444444444444444444444");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("bob-contact", &bob_pubkey));
        seed.contacts.push(contact("carol-contact", &carol_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions
            .push(group_session("session-design", "circle-1", "Design Circle"));
        seed.sessions
            .push(self_session("self-circle-1", "circle-1"));
        seed.groups.push(group_profile(
            "session-design",
            &["bob-contact", "carol-contact"],
        ));
        seed.message_store.insert(
            "session-design".into(),
            vec![signed_text_message(
                "local-group-message",
                Some(MessageDeliveryStatus::Sending),
                "relay-existing-group-event",
            )],
        );
        let inbound_message = inbound_relay_message(
            "relay-existing-group-event",
            &current_user_pubkey,
            "group replay without tags",
        );
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(
            &mut seed,
            &chat_effects,
            Some(current_user_pubkey.as_str()),
        )
        .expect("chat effects apply");

        assert!(changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("self-circle-1")
            .map_or(true, |messages| messages.is_empty()));
        let routed_messages = seed
            .message_store
            .get("session-design")
            .expect("existing message should stay in its original session");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "local-group-message");
        assert_eq!(
            routed_messages[0].remote_id.as_deref(),
            Some("relay-existing-group-event")
        );
        assert!(matches!(routed_messages[0].author, MessageAuthor::Me));
        assert!(matches!(
            routed_messages[0].sync_source,
            Some(MessageSyncSource::Local)
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
                sync_since_created_at: None,
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
