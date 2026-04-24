use crate::app::shell_auth;
use crate::domain::chat::{
    ChatDomainSeed, CircleStatus, CircleType, ContactItem, MergeRemoteMessagesInput, MessageAuthor,
    MessageDeliveryStatus, MessageItem, MessageSyncSource, RemoteDeliveryReceipt, SessionItem,
    SessionKind,
};
use crate::domain::chat_repository::{
    apply_change_set_to_seed, build_remote_delivery_receipt_change_set,
    merge_remote_messages_into_seed, message_media_label, message_media_local_path,
    message_media_remote_url, ChatRepository,
};
use crate::domain::transport::{
    CircleTransportDiagnostic, RelayProtocol, TransportActivityItem, TransportActivityKind,
    TransportActivityLevel, TransportCapabilities, TransportChatEffects, TransportCircleAction,
    TransportCircleActionInput, TransportEngineKind, TransportHealth, TransportMutationResult,
    TransportRelaySyncFilter, TransportRuntimeBackgroundSyncRequest, TransportRuntimeLaunchResult,
    TransportRuntimeOutboundMedia, TransportRuntimeOutboundMessage, TransportRuntimeSession,
    TransportService, TransportSnapshot, TransportSnapshotInput,
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
use crate::infra::media_upload;
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
        let background_sync_requests = collect_background_relay_sync_requests(
            &seed,
            &normalized_previous_cache,
            current_user_pubkey.as_deref(),
        );
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
            let outbound_media_messages = collect_transport_outbound_media_messages(
                &self.app_handle,
                &seed,
                &normalized_previous_cache,
                None,
            );
            let runtime_chat_effects = runtime_manager.sync_cache(
                &normalized_previous_cache,
                &mut state.cache,
                runtime_profiles,
                None,
                &outbound_messages,
                &outbound_media_messages,
                &[],
                &background_sync_requests,
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
                let outbound_media_messages = collect_transport_outbound_media_messages(
                    &self.app_handle,
                    &seed,
                    &working_cache,
                    Some(&action_input),
                );
                let relay_sync_filters =
                    relay_sync_filters(&seed, &action_input, current_user_pubkey.as_deref());
                let runtime_chat_effects = runtime_manager.sync_cache(
                    &working_cache,
                    &mut next_state.cache,
                    recovery_runtime_profiles,
                    Some(&action_input),
                    &outbound_messages,
                    &outbound_media_messages,
                    &relay_sync_filters,
                    &[],
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
        let outbound_media_messages = collect_transport_outbound_media_messages(
            &self.app_handle,
            &seed,
            &normalized_previous_cache,
            Some(&input),
        );
        let relay_sync_filters = relay_sync_filters(&seed, &input, current_user_pubkey.as_deref());
        let runtime_chat_effects = runtime_manager.sync_cache(
            &normalized_previous_cache,
            &mut state.cache,
            runtime_profiles,
            Some(&input),
            &outbound_messages,
            &outbound_media_messages,
            &relay_sync_filters,
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

fn collect_transport_outbound_media_messages<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    seed: &ChatDomainSeed,
    cache: &TransportCache,
    action: Option<&TransportCircleActionInput>,
) -> Vec<TransportRuntimeOutboundMedia> {
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
        .outbound_media_dispatches
        .iter()
        .filter(|dispatch| {
            allowed_circle_id
                .map(|circle_id| dispatch.circle_id == circle_id)
                .unwrap_or(true)
        })
        .map(|dispatch| {
            (
                dispatch.session_id.as_str(),
                dispatch.message_id.as_str(),
                dispatch.local_path.as_str(),
            )
        })
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
                    if !message_is_pending_local_outbound_media_publish(message) {
                        return None;
                    }
                    let local_path = message_media_local_path(message)?;
                    if dispatched_message_ids.contains(&(
                        session.id.as_str(),
                        message.id.as_str(),
                        local_path.as_str(),
                    )) {
                        return None;
                    }
                    let remote_url = media_upload::resolve_outbound_chat_media_remote_url(
                        app_handle,
                        &local_path,
                        &message.body,
                    )
                    .ok()?;

                    Some(TransportRuntimeOutboundMedia {
                        session_id: session.id.clone(),
                        message_id: message.id.clone(),
                        remote_id: message
                            .remote_id
                            .clone()
                            .unwrap_or_else(|| acked_remote_message_id(&message.id)),
                        kind: message.kind.clone(),
                        name: message.body.clone(),
                        label: message_media_label(message)?,
                        local_path,
                        remote_url,
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

fn message_is_pending_local_outbound_media_publish(message: &MessageItem) -> bool {
    matches!(message.author, MessageAuthor::Me)
        && !matches!(
            message.sync_source,
            Some(MessageSyncSource::Relay | MessageSyncSource::System)
        )
        && matches!(
            message.kind,
            crate::domain::chat::MessageKind::File
                | crate::domain::chat::MessageKind::Image
                | crate::domain::chat::MessageKind::Video
        )
        && message_media_local_path(message).is_some()
        && message_media_remote_url(message).is_none()
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
    current_user_pubkey: Option<&str>,
) -> Vec<TransportRelaySyncFilter> {
    if !action_supports_relay_sync(&input.action) {
        return Vec::new();
    }

    let current_user_pubkey = current_user_pubkey.and_then(normalize_nostr_pubkey);
    let contact_pubkeys = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey).map(|pubkey| (contact.id.as_str(), pubkey))
        })
        .collect::<HashMap<_, _>>();
    let mut filters = Vec::new();
    let mut direct_contact_pubkeys = seed
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
    direct_contact_pubkeys.sort_unstable();
    direct_contact_pubkeys.dedup();
    if !direct_contact_pubkeys.is_empty() {
        filters.push(TransportRelaySyncFilter {
            authors: direct_contact_pubkeys.clone(),
            tagged_pubkeys: Vec::new(),
        });
    }
    if let Some(current_user_pubkey) = current_user_pubkey.as_ref() {
        filters.push(TransportRelaySyncFilter {
            authors: Vec::new(),
            tagged_pubkeys: vec![current_user_pubkey.clone()],
        });
        if !direct_contact_pubkeys.is_empty() {
            filters.push(TransportRelaySyncFilter {
                authors: vec![current_user_pubkey.clone()],
                tagged_pubkeys: direct_contact_pubkeys.clone(),
            });
        }
    }

    filters.extend(
        seed.sessions
            .iter()
            .filter(|session| {
                session.circle_id == input.circle_id
                    && !session.archived.unwrap_or(false)
                    && matches!(session.kind, SessionKind::Group)
            })
            .flat_map(|session| {
                let Some(group) = seed
                    .groups
                    .iter()
                    .find(|group| group.session_id == session.id)
                else {
                    return Vec::new();
                };
                let mut member_pubkeys = group
                    .members
                    .iter()
                    .filter_map(|member| contact_pubkeys.get(member.contact_id.as_str()).cloned())
                    .collect::<Vec<_>>();
                member_pubkeys.sort_unstable();
                member_pubkeys.dedup();
                if member_pubkeys.is_empty() {
                    return Vec::new();
                }

                let tagged_pubkeys = if member_pubkeys.len() >= 2 {
                    member_pubkeys.clone()
                } else {
                    Vec::new()
                };
                let mut session_filters = vec![TransportRelaySyncFilter {
                    tagged_pubkeys: tagged_pubkeys.clone(),
                    authors: member_pubkeys.clone(),
                }];
                if let Some(current_user_pubkey) = current_user_pubkey.as_ref() {
                    session_filters.push(TransportRelaySyncFilter {
                        authors: vec![current_user_pubkey.clone()],
                        tagged_pubkeys: member_pubkeys,
                    });
                }

                session_filters
            }),
    );
    let has_self_chat_session = seed.sessions.iter().any(|session| {
        session.circle_id == input.circle_id
            && !session.archived.unwrap_or(false)
            && matches!(session.kind, SessionKind::SelfChat)
    });
    if has_self_chat_session {
        if let Some(current_user_pubkey) = current_user_pubkey {
            filters.push(TransportRelaySyncFilter {
                authors: vec![current_user_pubkey],
                tagged_pubkeys: Vec::new(),
            });
        }
    }
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
    if !action_supports_relay_sync(&input.action) {
        return None;
    }

    relay_sync_cursor_created_at(cache, &input.circle_id)
        .or_else(|| latest_peer_relay_created_at(seed, &input.circle_id))
        .map(|created_at| created_at.saturating_sub(RELAY_SYNC_OVERLAP_SECS))
}

fn action_supports_relay_sync(action: &TransportCircleAction) -> bool {
    matches!(
        action,
        TransportCircleAction::Sync | TransportCircleAction::SyncSessions
    )
}

fn collect_background_relay_sync_requests(
    seed: &ChatDomainSeed,
    cache: &TransportCache,
    current_user_pubkey: Option<&str>,
) -> Vec<TransportRuntimeBackgroundSyncRequest> {
    seed.circles
        .iter()
        .filter(|circle| relay_supports_background_sync(circle.relay.as_str()))
        .filter_map(|circle| {
            let has_visible_session = seed.sessions.iter().any(|session| {
                session.circle_id == circle.id && !session.archived.unwrap_or(false)
            });
            if !has_visible_session {
                return None;
            }

            let action_input = TransportCircleActionInput {
                circle_id: circle.id.clone(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some(circle.id.clone()),
                use_tor_network: false,
                experimental_transport: false,
                sync_since_created_at: None,
            };
            let relay_sync_filters = relay_sync_filters(seed, &action_input, current_user_pubkey);
            if relay_sync_filters.is_empty() {
                return None;
            }

            Some(TransportRuntimeBackgroundSyncRequest {
                circle_id: circle.id.clone(),
                sync_since_created_at: relay_sync_since_created_at(seed, cache, &action_input),
                relay_sync_filters,
            })
        })
        .collect()
}

fn relay_supports_background_sync(relay: &str) -> bool {
    relay.starts_with("ws://") || relay.starts_with("wss://")
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

    let valid_media_dispatches = seed
        .message_store
        .iter()
        .flat_map(|(session_id, messages)| {
            let circle_id = session_circle_ids.get(session_id.as_str()).copied();
            messages.iter().filter_map(move |message| {
                let circle_id = circle_id?;
                if !message_is_pending_local_outbound_media_publish(message) {
                    return None;
                }
                let local_path = message_media_local_path(message)?;

                Some((
                    circle_id.to_string(),
                    session_id.clone(),
                    message.id.clone(),
                    local_path,
                ))
            })
        })
        .collect::<HashSet<_>>();

    cache.outbound_media_dispatches.retain(|dispatch| {
        valid_media_dispatches.contains(&(
            dispatch.circle_id.clone(),
            dispatch.session_id.clone(),
            dispatch.message_id.clone(),
            dispatch.local_path.clone(),
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
    seed: &mut ChatDomainSeed,
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
            .or_else(|| {
                ensure_direct_peer_message_target_session(
                    seed,
                    &merge.session_id,
                    &normalized_message,
                    current_user_pubkey,
                )
            })
            .or_else(|| {
                should_preserve_unresolved_remote_merge(&normalized_message)
                    .then(|| merge.session_id.clone())
            });
        let Some(target_session_id) = target_session_id else {
            continue;
        };
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

fn should_preserve_unresolved_remote_merge(message: &MessageItem) -> bool {
    if !matches!(message.author, MessageAuthor::Peer) {
        return true;
    }

    message_sender_pubkey(message).is_none()
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
    let tagged_pubkeys = tagged_pubkeys_for_remote_message(message);
    let raw_p_tag_count = raw_p_tag_count_for_remote_message(message);
    if current_user_pubkey.is_some_and(|pubkey| pubkey == sender_pubkey) {
        return resolve_group_self_message_target_session(seed, fallback_circle_id, message)
            .or_else(|| {
                (raw_p_tag_count == 1)
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
            (raw_p_tag_count == 1)
                .then(|| {
                    resolve_direct_peer_message_target_session(
                        seed,
                        fallback_circle_id,
                        &sender_pubkey,
                        current_user_pubkey,
                        &tagged_pubkeys,
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
    current_user_pubkey: Option<&str>,
    tagged_pubkeys: &HashSet<String>,
) -> Option<String> {
    let current_user_pubkey = current_user_pubkey.and_then(normalize_nostr_pubkey)?;
    if tagged_pubkeys.len() != 1 || !tagged_pubkeys.contains(&current_user_pubkey) {
        return None;
    }

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

    let candidate_session_ids = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == circle_id
                && !session.archived.unwrap_or(false)
                && matches!(session.kind, SessionKind::Direct)
                && session
                    .contact_id
                    .as_deref()
                    .is_some_and(|contact_id| matching_contact_ids.contains(contact_id))
        })
        .map(|session| session.id.clone())
        .collect::<Vec<_>>();

    if candidate_session_ids.len() == 1 {
        candidate_session_ids.into_iter().next()
    } else {
        None
    }
}

fn ensure_direct_peer_message_target_session(
    seed: &mut ChatDomainSeed,
    fallback_session_id: &str,
    message: &MessageItem,
    current_user_pubkey: Option<&str>,
) -> Option<String> {
    if !matches!(message.author, MessageAuthor::Peer)
        || !matches!(message.sync_source, Some(MessageSyncSource::Relay))
    {
        return None;
    }

    let sender_pubkey = message_sender_pubkey(message)?;
    let current_user_pubkey = current_user_pubkey.and_then(normalize_nostr_pubkey)?;
    let tagged_pubkeys = tagged_pubkeys_for_remote_message(message);
    if raw_p_tag_count_for_remote_message(message) != 1
        || tagged_pubkeys.len() != 1
        || !tagged_pubkeys.contains(&current_user_pubkey)
    {
        return None;
    }

    let fallback_circle_id = seed
        .sessions
        .iter()
        .find(|session| session.id == fallback_session_id)
        .map(|session| session.circle_id.clone())?;
    let matching_contact_ids = seed
        .contacts
        .iter()
        .filter_map(|contact| {
            normalize_nostr_pubkey(&contact.pubkey)
                .filter(|pubkey| pubkey == &sender_pubkey)
                .map(|_| contact.id.clone())
        })
        .collect::<Vec<_>>();
    let contact_id = match matching_contact_ids.as_slice() {
        [contact_id] => contact_id.clone(),
        [] => {
            let contact = build_auto_direct_contact(seed, &sender_pubkey);
            let contact_id = contact.id.clone();
            seed.contacts.push(contact);
            contact_id
        }
        _ => return None,
    };
    let matching_session_indices = seed
        .sessions
        .iter()
        .enumerate()
        .filter(|(_, session)| {
            session.circle_id == fallback_circle_id
                && matches!(session.kind, SessionKind::Direct)
                && session.contact_id.as_deref() == Some(contact_id.as_str())
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();

    match matching_session_indices.as_slice() {
        [index] => {
            let session = seed.sessions.get_mut(*index)?;
            session.archived = Some(false);
            Some(session.id.clone())
        }
        [] => {
            let contact = seed
                .contacts
                .iter()
                .find(|contact| contact.id == contact_id)
                .cloned()?;
            let session = build_auto_direct_session(seed, &fallback_circle_id, &contact);
            let session_id = session.id.clone();
            seed.sessions.push(session);
            Some(session_id)
        }
        _ => None,
    }
}

fn build_auto_direct_contact(seed: &ChatDomainSeed, sender_pubkey: &str) -> ContactItem {
    let slug = sender_pubkey
        .chars()
        .take(12)
        .collect::<String>()
        .to_lowercase();
    let short = sender_pubkey
        .chars()
        .take(6)
        .collect::<String>()
        .to_uppercase();
    ContactItem {
        id: build_unique_contact_id(&format!("relay-contact-{slug}"), &seed.contacts),
        name: format!("Remote {short}"),
        initials: build_auto_contact_initials(sender_pubkey),
        handle: format!("@relay-{slug}"),
        pubkey: sender_pubkey.into(),
        ethereum_address: None,
        subtitle: "Imported from relay".into(),
        bio: "Auto-created from an inbound relay direct message.".into(),
        online: Some(false),
        blocked: Some(false),
    }
}

fn build_auto_direct_session(
    seed: &ChatDomainSeed,
    circle_id: &str,
    contact: &ContactItem,
) -> SessionItem {
    let session_id = build_unique_session_id(&format!("session-{}", contact.id), &seed.sessions);
    SessionItem {
        id: session_id,
        circle_id: circle_id.into(),
        name: contact.name.clone(),
        initials: contact.initials.clone(),
        subtitle: "Start a conversation".into(),
        time: "now".into(),
        unread_count: None,
        muted: None,
        pinned: None,
        draft: None,
        kind: SessionKind::Direct,
        category: "friends".into(),
        members: None,
        contact_id: Some(contact.id.clone()),
        archived: Some(false),
    }
}

fn build_auto_contact_initials(sender_pubkey: &str) -> String {
    let initials = sender_pubkey
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if initials.is_empty() {
        "RC".into()
    } else {
        initials
    }
}

fn build_unique_contact_id(base_id: &str, contacts: &[ContactItem]) -> String {
    let mut candidate = base_id.to_string();
    let mut suffix = 2;

    while contacts.iter().any(|contact| contact.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn build_unique_session_id(base_id: &str, sessions: &[SessionItem]) -> String {
    let mut candidate = base_id.to_string();
    let mut suffix = 2;

    while sessions.iter().any(|session| session.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
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

    let expected_tagged_pubkeys = member_pubkeys
        .iter()
        .filter(|pubkey| pubkey.as_str() != sender_pubkey)
        .cloned()
        .collect::<HashSet<_>>();
    !expected_tagged_pubkeys.is_empty() && tagged_pubkeys == &expected_tagged_pubkeys
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
    !member_pubkeys.is_empty() && tagged_pubkeys == &member_pubkeys
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
    use crate::app::chat_mutations;
    use crate::domain::chat::{
        AuthRuntimeState, AuthRuntimeSummary, AuthSessionSummary, CircleItem, CircleType,
        ContactItem, GroupMember, GroupProfile, GroupRole, LoginAccessKind, LoginAccessSummary,
        LoginCircleSelectionMode, LoginMethod, MergeRemoteDeliveryReceiptsInput,
        MergeRemoteMessagesInput, MessageAuthor, MessageDeliveryStatus, MessageItem, MessageKind,
        MessageSyncSource, SendMessageInput, SessionItem, SessionKind, ShellStateSnapshot,
        SignedNostrEvent,
    };
    use crate::domain::transport::{
        TransportActivityKind, TransportActivityLevel, TransportCircleAction,
        TransportCircleActionInput, TransportOutboundDispatch, TransportOutboundMediaDispatch,
        TransportRelaySyncFilter, TransportRuntimeAdapterKind, TransportRuntimeDesiredState,
        TransportRuntimeLaunchResult, TransportRuntimeLaunchStatus, TransportRuntimeQueueState,
        TransportRuntimeRecoveryPolicy, TransportRuntimeRegistryEntry, TransportRuntimeState,
        TransportSnapshotInput,
    };
    use crate::domain::transport_repository::{
        TransportRelayBackgroundSyncMarker, TransportRelaySyncCursor,
    };
    use crate::infra::{auth_runtime_credential_store, shell_state_store};
    use secp256k1::{Secp256k1, SecretKey};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::process::Command;
    use std::str::FromStr;
    use std::sync::MutexGuard;
    use std::thread;
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestAppGuard {
        _env_guard: MutexGuard<'static, ()>,
        app: tauri::App<tauri::test::MockRuntime>,
        config_root: PathBuf,
        previous_xdg_config_home: Option<String>,
        previous_tmpdir: Option<String>,
        previous_tmp: Option<String>,
        previous_temp: Option<String>,
        previous_upload_driver: Option<String>,
        previous_upload_endpoint: Option<String>,
    }

    impl Drop for TestAppGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous_xdg_config_home {
                std::env::set_var("XDG_CONFIG_HOME", previous);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }
            if let Some(previous) = &self.previous_tmpdir {
                std::env::set_var("TMPDIR", previous);
            } else {
                std::env::remove_var("TMPDIR");
            }
            if let Some(previous) = &self.previous_tmp {
                std::env::set_var("TMP", previous);
            } else {
                std::env::remove_var("TMP");
            }
            if let Some(previous) = &self.previous_temp {
                std::env::set_var("TEMP", previous);
            } else {
                std::env::remove_var("TEMP");
            }
            if let Some(previous) = &self.previous_upload_driver {
                std::env::set_var("P2P_CHAT_MEDIA_UPLOAD_DRIVER", previous);
            } else {
                std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_DRIVER");
            }
            if let Some(previous) = &self.previous_upload_endpoint {
                std::env::set_var("P2P_CHAT_MEDIA_UPLOAD_ENDPOINT", previous);
            } else {
                std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_ENDPOINT");
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
        let runtime_tmp_root = config_root.join("tmp");
        std::fs::create_dir_all(&runtime_tmp_root).expect("failed to create runtime temp root");

        let previous_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
        let previous_tmpdir = std::env::var("TMPDIR").ok();
        let previous_tmp = std::env::var("TMP").ok();
        let previous_temp = std::env::var("TEMP").ok();
        let previous_upload_driver = std::env::var("P2P_CHAT_MEDIA_UPLOAD_DRIVER").ok();
        let previous_upload_endpoint = std::env::var("P2P_CHAT_MEDIA_UPLOAD_ENDPOINT").ok();
        std::env::set_var("XDG_CONFIG_HOME", &config_root);
        std::env::set_var("TMPDIR", &runtime_tmp_root);
        std::env::set_var("TMP", &runtime_tmp_root);
        std::env::set_var("TEMP", &runtime_tmp_root);
        std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_DRIVER");
        std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_ENDPOINT");

        let app = tauri::test::mock_app();
        TestAppGuard {
            _env_guard: env_guard,
            app,
            config_root,
            previous_xdg_config_home,
            previous_tmpdir,
            previous_tmp,
            previous_temp,
            previous_upload_driver,
            previous_upload_endpoint,
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
            ethereum_address: None,
            subtitle: "test".into(),
            bio: "test".into(),
            online: Some(false),
            blocked: Some(false),
        }
    }

    fn load_seed_from_store(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
    ) -> ChatDomainSeed {
        SqliteChatRepository::new(app_handle)
            .load_domain_seed()
            .expect("failed to load domain seed")
    }

    fn save_seed_to_store(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        seed: ChatDomainSeed,
    ) {
        SqliteChatRepository::new(app_handle)
            .save_domain_seed(seed)
            .expect("failed to save domain seed");
    }

    fn load_transport_cache_from_store(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
    ) -> TransportCache {
        SqliteTransportRepository::new(app_handle)
            .load_transport_cache()
            .expect("failed to load transport cache")
    }

    fn set_circle_relay(seed: &mut ChatDomainSeed, circle_id: &str, relay: &str) {
        let circle = seed
            .circles
            .iter_mut()
            .find(|circle| circle.id == circle_id)
            .expect("missing circle");
        circle.relay = relay.into();
    }

    fn set_contact_pubkey(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        contact_id: &str,
        pubkey: &str,
    ) {
        let mut seed = load_seed_from_store(app_handle);
        let contact = seed
            .contacts
            .iter_mut()
            .find(|contact| contact.id == contact_id)
            .expect("missing contact");
        contact.pubkey = pubkey.into();
        save_seed_to_store(app_handle, seed);
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

    fn seed_authenticated_local_secret_runtime(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        session_id: &str,
    ) {
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            session_id,
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
    }

    fn seed_authenticated_local_secret_runtime_with_secret_key(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        session_id: &str,
        secret_key_hex: &str,
    ) {
        let current_user_pubkey = valid_pubkey_hex(secret_key_hex);
        let seed = load_seed_from_store(app_handle);
        let active_circle_id = seed
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| session.circle_id.clone())
            .unwrap_or_else(|| "main-circle".into());

        let mut shell = ShellStateSnapshot::from(seed);
        shell.is_authenticated = true;
        shell.auth_session = Some(AuthSessionSummary {
            login_method: LoginMethod::ExistingAccount,
            access: LoginAccessSummary {
                kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: Some(current_user_pubkey.clone()),
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: "2026-04-21T10:00:00Z".into(),
        });
        shell.auth_runtime = Some(AuthRuntimeSummary {
            state: AuthRuntimeState::Connected,
            login_method: LoginMethod::ExistingAccount,
            access_kind: LoginAccessKind::HexKey,
            label: "npub1local".into(),
            pubkey: Some(current_user_pubkey.clone()),
            error: None,
            can_send_messages: true,
            send_blocked_reason: None,
            persisted_in_native_store: true,
            credential_persisted_in_native_store: true,
            updated_at: "2026-04-21T10:00:00Z".into(),
        });
        shell.active_circle_id = active_circle_id;
        shell.selected_session_id = session_id.into();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(shell).expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state");
        auth_runtime_credential_store::save(
            app_handle,
            &auth_runtime_credential_store::StoredAuthRuntimeCredential {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                secret_key_hex: secret_key_hex.into(),
                pubkey: current_user_pubkey,
                stored_at: "2026-04-21T10:00:00Z".into(),
            },
        )
        .expect("failed to seed local auth credential");
    }

    fn manual_live_service_relay_url() -> String {
        std::env::var("P2P_CHAT_LIVE_RELAY_URL")
            .ok()
            .or_else(|| {
                std::env::var("P2P_CHAT_LIVE_RELAY_URLS")
                    .ok()
                    .and_then(|value| {
                        value
                            .split(',')
                            .map(str::trim)
                            .find(|value| !value.is_empty())
                            .map(str::to_string)
                    })
            })
            .unwrap_or_else(|| "wss://nos.lol".into())
    }

    fn ensure_live_runtime_binary_ready() {
        let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let output = Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(&manifest_path)
            .arg("--bin")
            .arg("p2p-chat-runtime")
            .output()
            .expect("failed to invoke cargo build for p2p-chat-runtime");
        assert!(
            output.status.success(),
            "cargo build --bin p2p-chat-runtime failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
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

    fn relay_copy_of_signed_local_message(message: &MessageItem) -> MessageItem {
        let signed_nostr_event = message
            .signed_nostr_event
            .clone()
            .expect("local signed message should include signed event");

        MessageItem {
            id: signed_nostr_event.event_id.clone(),
            kind: message.kind.clone(),
            author: MessageAuthor::Peer,
            body: signed_nostr_event.content.clone(),
            time: message.time.clone(),
            meta: message.meta.clone(),
            delivery_status: None,
            remote_id: Some(signed_nostr_event.event_id.clone()),
            sync_source: Some(MessageSyncSource::Relay),
            acked_at: None,
            signed_nostr_event: Some(signed_nostr_event),
            reply_to: None,
        }
    }

    fn local_media_message(
        id: &str,
        kind: MessageKind,
        body: &str,
        label: &str,
        local_path: &str,
    ) -> MessageItem {
        let meta = match kind {
            MessageKind::File => serde_json::json!({
                "version": 1,
                "label": label,
                "localPath": local_path,
            })
            .to_string(),
            MessageKind::Image | MessageKind::Video => serde_json::json!({
                "version": 2,
                "label": label,
                "localPath": local_path,
            })
            .to_string(),
            _ => label.to_string(),
        };

        MessageItem {
            id: id.into(),
            kind,
            author: MessageAuthor::Me,
            body: body.into(),
            time: "now".into(),
            meta: Some(meta),
            delivery_status: Some(MessageDeliveryStatus::Sent),
            remote_id: Some(format!("relay-ack:{id}")),
            sync_source: Some(MessageSyncSource::Local),
            acked_at: Some("now".into()),
            signed_nostr_event: None,
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
    fn collect_transport_outbound_media_messages_filters_to_local_media_without_remote_url() {
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
                local_media_message(
                    "message-image",
                    MessageKind::Image,
                    "preview.png",
                    "PNG · 32 KB",
                    "/tmp/chat-media/images/preview.png",
                ),
                MessageItem {
                    meta: Some(
                        serde_json::json!({
                            "version": 3,
                            "label": "PNG · uploaded",
                            "localPath": "/tmp/chat-media/images/already-uploaded.png",
                            "remoteUrl": "https://cdn.example.test/chat-media/already-uploaded.png",
                        })
                        .to_string(),
                    ),
                    ..local_media_message(
                        "message-image-uploaded",
                        MessageKind::Image,
                        "already-uploaded.png",
                        "PNG · uploaded",
                        "/tmp/chat-media/images/already-uploaded.png",
                    )
                },
            ],
        );
        seed.message_store.insert(
            "session-2".into(),
            vec![local_media_message(
                "message-video-other-circle",
                MessageKind::Video,
                "clip.mp4",
                "MP4 · 0:08",
                "/tmp/chat-media/videos/clip.mp4",
            )],
        );

        let guard = test_app();
        let outbound_media_messages = collect_transport_outbound_media_messages(
            guard.app.handle(),
            &seed,
            &TransportCache {
                outbound_media_dispatches: vec![TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image-uploaded".into(),
                    remote_id: "relay-ack:message-image-uploaded".into(),
                    local_path: "/tmp/chat-media/images/already-uploaded.png".into(),
                    runtime_generation: 1,
                    request_id: "sync:circle-1:media-uploaded".into(),
                    dispatched_at: "now".into(),
                }],
                ..TransportCache::default()
            },
            Some(&TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            }),
        );

        assert_eq!(outbound_media_messages.len(), 1);
        assert_eq!(outbound_media_messages[0].session_id, "session-1");
        assert_eq!(outbound_media_messages[0].message_id, "message-image");
        assert!(matches!(
            outbound_media_messages[0].kind,
            MessageKind::Image
        ));
        assert_eq!(outbound_media_messages[0].label, "PNG · 32 KB");
        assert_eq!(
            outbound_media_messages[0].local_path,
            "/tmp/chat-media/images/preview.png"
        );
        assert!(outbound_media_messages[0]
            .remote_url
            .starts_with("http://127.0.0.1:45115/chat-media/asset/"));
    }

    #[test]
    fn collect_transport_outbound_media_messages_skips_already_dispatched_local_media() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![local_media_message(
                "message-image",
                MessageKind::Image,
                "preview.png",
                "PNG · 32 KB",
                "/tmp/chat-media/images/preview.png",
            )],
        );

        let guard = test_app();
        let outbound_media_messages = collect_transport_outbound_media_messages(
            guard.app.handle(),
            &seed,
            &TransportCache {
                outbound_media_dispatches: vec![TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image".into(),
                    remote_id: "relay-ack:message-image".into(),
                    local_path: "/tmp/chat-media/images/preview.png".into(),
                    runtime_generation: 1,
                    request_id: "publish:circle-1:1".into(),
                    dispatched_at: "now".into(),
                }],
                ..TransportCache::default()
            },
            None,
        );

        assert!(outbound_media_messages.is_empty());
    }

    #[test]
    fn collect_transport_outbound_media_messages_uses_configured_upload_backend() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = crate::infra::media_store::store_chat_media_asset(
            app_handle,
            crate::domain::chat::StoreChatMediaAssetInput {
                kind: crate::domain::chat::ChatMediaKind::Image,
                name: "preview.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("upload server should bind");
        let address = listener
            .local_addr()
            .expect("upload server address should resolve");
        std::env::set_var("P2P_CHAT_MEDIA_UPLOAD_DRIVER", "filedrop");
        std::env::set_var(
            "P2P_CHAT_MEDIA_UPLOAD_ENDPOINT",
            format!("http://{address}"),
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("upload server should accept");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = std::io::Read::read(&mut stream, &mut buffer)
                    .expect("upload request should read");
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(header_end) = bytes
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .map(|index| index + 4)
                {
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if !name.eq_ignore_ascii_case("content-length") {
                                return None;
                            }
                            value.trim().parse::<usize>().ok()
                        })
                        .unwrap_or(0);
                    if bytes.len() >= header_end + content_length {
                        break;
                    }
                }
            }
            let response_body =
                r#"{"url":"https://cdn.example.test/chat-media/uploaded-preview.png"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            std::io::Write::write_all(&mut stream, response.as_bytes())
                .expect("upload response should be written");
        });

        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![local_media_message(
                "message-image",
                MessageKind::Image,
                "preview.png",
                "PNG · 32 KB",
                &stored.local_path,
            )],
        );

        let outbound_media_messages = collect_transport_outbound_media_messages(
            app_handle,
            &seed,
            &TransportCache::default(),
            Some(&TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::Sync,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            }),
        );
        server.join().expect("upload server should exit cleanly");
        std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_DRIVER");
        std::env::remove_var("P2P_CHAT_MEDIA_UPLOAD_ENDPOINT");

        assert_eq!(outbound_media_messages.len(), 1);
        assert_eq!(
            outbound_media_messages[0].remote_url,
            "https://cdn.example.test/chat-media/uploaded-preview.png"
        );
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
            None,
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
            None,
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
    fn relay_sync_filters_include_self_authored_direct_group_and_self_chat_queries() {
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
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
        seed.sessions
            .push(self_session("self-circle-1", "circle-1"));
        seed.groups
            .push(group_profile("group-1", &["bob-contact", "carol-contact"]));

        let filters = relay_sync_filters(
            &seed,
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::SyncSessions,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
            Some(current_user_pubkey.as_str()),
        );
        let mut expected_group_pubkeys = vec![group_sender_pubkey, other_group_pubkey];
        expected_group_pubkeys.sort_unstable();

        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: vec![direct_pubkey.clone()],
            tagged_pubkeys: Vec::new(),
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: Vec::new(),
            tagged_pubkeys: vec![current_user_pubkey.clone()],
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: vec![current_user_pubkey.clone()],
            tagged_pubkeys: vec![direct_pubkey],
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: expected_group_pubkeys.clone(),
            tagged_pubkeys: expected_group_pubkeys.clone(),
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: vec![current_user_pubkey.clone()],
            tagged_pubkeys: expected_group_pubkeys,
        }));
        assert!(filters.contains(&TransportRelaySyncFilter {
            authors: vec![current_user_pubkey],
            tagged_pubkeys: Vec::new(),
        }));
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
    fn relay_sync_since_created_at_supports_sync_sessions_actions() {
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
            &TransportCache::default(),
            &TransportCircleActionInput {
                circle_id: "circle-1".into(),
                action: TransportCircleAction::SyncSessions,
                active_circle_id: Some("circle-1".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            },
        );

        assert_eq!(since, Some(1_735_689_300));
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
    fn sqlite_transport_repository_roundtrips_relay_background_sync_markers() {
        let guard = test_app();
        let repository = SqliteTransportRepository::new(guard.app.handle());
        let expected = vec![
            TransportRelayBackgroundSyncMarker {
                circle_id: "circle-1".into(),
                last_requested_at_ms: 1_735_689_300,
            },
            TransportRelayBackgroundSyncMarker {
                circle_id: "circle-2".into(),
                last_requested_at_ms: 1_735_690_100,
            },
        ];

        repository
            .save_transport_cache(TransportCache {
                relay_background_sync_markers: expected.clone(),
                ..TransportCache::default()
            })
            .expect("transport cache should save");
        let cache = repository
            .load_transport_cache()
            .expect("transport cache should load");

        assert_eq!(cache.relay_background_sync_markers, expected);
    }

    #[test]
    fn sqlite_transport_repository_roundtrips_outbound_media_dispatches() {
        let guard = test_app();
        let repository = SqliteTransportRepository::new(guard.app.handle());
        let expected = vec![TransportOutboundMediaDispatch {
            circle_id: "circle-1".into(),
            session_id: "session-1".into(),
            message_id: "message-media-1".into(),
            remote_id: "relay-ack:message-media-1".into(),
            local_path: "/tmp/chat-media/images/preview.png".into(),
            runtime_generation: 3,
            request_id: "publish:circle-1:123".into(),
            dispatched_at: "now".into(),
        }];

        repository
            .save_transport_cache(TransportCache {
                outbound_media_dispatches: expected.clone(),
                ..TransportCache::default()
            })
            .expect("transport cache should save");
        let cache = repository
            .load_transport_cache()
            .expect("transport cache should load");

        assert_eq!(cache.outbound_media_dispatches, expected);
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
    fn reconcile_transport_outbound_dispatches_drops_uploaded_missing_and_stale_media_records() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions.push(direct_session("session-1", "circle-1"));
        seed.message_store.insert(
            "session-1".into(),
            vec![
                local_media_message(
                    "message-image-queued",
                    MessageKind::Image,
                    "queued.png",
                    "PNG · queued",
                    "/tmp/chat-media/images/queued.png",
                ),
                MessageItem {
                    meta: Some(
                        serde_json::json!({
                            "version": 3,
                            "label": "PNG · uploaded",
                            "localPath": "/tmp/chat-media/images/uploaded.png",
                            "remoteUrl": "https://cdn.example.test/chat-media/uploaded.png",
                        })
                        .to_string(),
                    ),
                    ..local_media_message(
                        "message-image-uploaded",
                        MessageKind::Image,
                        "uploaded.png",
                        "PNG · uploaded",
                        "/tmp/chat-media/images/uploaded.png",
                    )
                },
            ],
        );
        let mut cache = TransportCache {
            runtime_registry: vec![runtime_entry(TransportRuntimeState::Active)],
            outbound_media_dispatches: vec![
                TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image-queued".into(),
                    remote_id: "relay-ack:message-image-queued".into(),
                    local_path: "/tmp/chat-media/images/queued.png".into(),
                    runtime_generation: 1,
                    request_id: "publish:circle-1:queued".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image-uploaded".into(),
                    remote_id: "relay-ack:message-image-uploaded".into(),
                    local_path: "/tmp/chat-media/images/uploaded.png".into(),
                    runtime_generation: 1,
                    request_id: "publish:circle-1:uploaded".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image-queued".into(),
                    remote_id: "relay-ack:message-image-queued".into(),
                    local_path: "/tmp/chat-media/images/queued.png".into(),
                    runtime_generation: 0,
                    request_id: "publish:circle-1:stale-generation".into(),
                    dispatched_at: "now".into(),
                },
                TransportOutboundMediaDispatch {
                    circle_id: "circle-1".into(),
                    session_id: "session-1".into(),
                    message_id: "message-image-missing".into(),
                    remote_id: "relay-ack:message-image-missing".into(),
                    local_path: "/tmp/chat-media/images/missing.png".into(),
                    runtime_generation: 1,
                    request_id: "publish:circle-1:missing".into(),
                    dispatched_at: "now".into(),
                },
            ],
            ..TransportCache::default()
        };

        reconcile_transport_outbound_dispatches(&mut cache, &seed);

        assert_eq!(cache.outbound_media_dispatches.len(), 1);
        assert_eq!(
            cache.outbound_media_dispatches[0].message_id,
            "message-image-queued"
        );
        assert_eq!(
            cache.outbound_media_dispatches[0].local_path,
            "/tmp/chat-media/images/queued.png"
        );
    }

    #[test]
    fn apply_transport_chat_effects_routes_inbound_peer_message_to_matching_direct_session() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-event-1",
            &sender_pubkey,
            "hello from alice via relay",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), current_user_pubkey.clone()]];
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
    fn apply_transport_chat_effects_auto_creates_direct_session_for_unknown_sender() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        let mut inbound_message = inbound_relay_message(
            "relay-event-new-direct-1",
            &sender_pubkey,
            "hello from a brand new relay contact",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), current_user_pubkey.clone()]];

        let changed = apply_transport_chat_effects(
            &mut seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "session-primary".into(),
                    messages: vec![inbound_message],
                }],
                ..TransportChatEffects::default()
            },
            Some(current_user_pubkey.as_str()),
        )
        .expect("chat effects apply");

        assert!(changed);
        assert_eq!(seed.contacts.len(), 1);
        let contact = seed.contacts.first().expect("missing auto-created contact");
        assert_eq!(contact.pubkey, sender_pubkey);
        assert_eq!(contact.subtitle, "Imported from relay");
        let session = seed
            .sessions
            .iter()
            .find(|session| {
                session.id != "session-primary"
                    && session.circle_id == "circle-1"
                    && matches!(session.kind, SessionKind::Direct)
                    && session.contact_id.as_deref() == Some(contact.id.as_str())
            })
            .expect("missing auto-created direct session");
        let routed_messages = seed
            .message_store
            .get(&session.id)
            .expect("auto-created direct session should receive routed message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-event-new-direct-1");
        assert_eq!(session.subtitle, "hello from a brand new relay contact");
        assert_eq!(session.unread_count, Some(1));
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_routes_peer_reply_and_reconciles_local_direct_receipt() {
        let peer_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &peer_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));

        let local_remote_id = "event-local-outbound";
        let local_body = "Hello Alice from local sender.";
        let reply_body = "Relay reply from Alice.";
        let mut local_outbound = signed_text_message(
            "local-outbound",
            Some(MessageDeliveryStatus::Sending),
            local_remote_id,
        );
        local_outbound.body = local_body.into();
        local_outbound
            .signed_nostr_event
            .as_mut()
            .expect("local outbound should include signed event")
            .content = local_body.into();
        local_outbound
            .signed_nostr_event
            .as_mut()
            .expect("local outbound should include signed event")
            .tags = vec![vec!["p".into(), peer_pubkey.clone()]];
        seed.message_store
            .insert("session-alice".into(), vec![local_outbound.clone()]);

        let mut inbound_reply =
            inbound_relay_message("relay-event-reply", &peer_pubkey, reply_body);
        inbound_reply
            .signed_nostr_event
            .as_mut()
            .expect("inbound reply should include signed event")
            .tags = vec![
            vec!["p".into(), current_user_pubkey.clone()],
            vec!["e".into(), local_remote_id.into()],
        ];

        let changed = apply_transport_chat_effects(
            &mut seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "session-primary".into(),
                    messages: vec![inbound_reply],
                }],
                remote_delivery_receipt_merges: vec![MergeRemoteDeliveryReceiptsInput {
                    session_id: "session-alice".into(),
                    receipts: vec![RemoteDeliveryReceipt {
                        remote_id: local_remote_id.into(),
                        message_id: None,
                        delivery_status: MessageDeliveryStatus::Sent,
                        acked_at: Some("relay-ok".into()),
                    }],
                }],
                ..TransportChatEffects::default()
            },
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
            .get("session-alice")
            .expect("matching direct session should keep both routed messages");
        assert_eq!(routed_messages.len(), 2);

        let reconciled_local = routed_messages
            .iter()
            .find(|message| message.id == "local-outbound")
            .expect("local outbound message should still be present");
        assert!(matches!(reconciled_local.author, MessageAuthor::Me));
        assert!(matches!(
            reconciled_local.sync_source,
            Some(MessageSyncSource::Local)
        ));
        assert_eq!(reconciled_local.remote_id.as_deref(), Some(local_remote_id));
        assert!(matches!(
            reconciled_local.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(reconciled_local.acked_at.as_deref(), Some("relay-ok"));

        let routed_reply = routed_messages
            .iter()
            .find(|message| message.remote_id.as_deref() == Some("relay-event-reply"))
            .expect("peer reply should be routed into the matching direct session");
        assert!(matches!(routed_reply.author, MessageAuthor::Peer));
        assert!(matches!(
            routed_reply.sync_source,
            Some(MessageSyncSource::Relay)
        ));
        let reply_preview = routed_reply
            .reply_to
            .as_ref()
            .expect("peer reply should hydrate the local reply preview");
        assert_eq!(reply_preview.message_id, "local-outbound");
        assert_eq!(reply_preview.remote_id.as_deref(), Some(local_remote_id));
        assert!(matches!(reply_preview.author, MessageAuthor::Me));
        assert_eq!(reply_preview.author_label, "You");
        assert_eq!(reply_preview.snippet, local_body);

        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "session-alice")
            .expect("missing routed session");
        assert_eq!(session.subtitle, reply_body);
        assert_eq!(session.unread_count, Some(1));

        let contact = seed
            .contacts
            .iter()
            .find(|contact| contact.id == "alice-contact")
            .expect("missing direct contact");
        assert_eq!(contact.online, Some(true));
    }

    #[test]
    fn apply_transport_chat_effects_creates_direct_session_in_current_circle_when_contact_exists_elsewhere(
    ) {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
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
        let mut inbound_message =
            inbound_relay_message("relay-event-2", &sender_pubkey, "stay inside circle-1");
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), current_user_pubkey.clone()]];
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
            .get("session-alice-circle-2")
            .map_or(true, |messages| messages.is_empty()));
        let circle_1_session = seed
            .sessions
            .iter()
            .find(|session| {
                session.id != "session-primary"
                    && session.id != "session-alice-circle-2"
                    && session.circle_id == "circle-1"
                    && session.contact_id.as_deref() == Some("alice-contact")
            })
            .expect("current circle should get a direct session for the known contact");
        let routed_messages = seed
            .message_store
            .get(&circle_1_session.id)
            .expect("newly created current-circle session should receive message");
        assert_eq!(routed_messages.len(), 1);
        assert_eq!(routed_messages[0].id, "relay-event-2");
    }

    #[test]
    fn apply_transport_chat_effects_drops_ambiguous_peer_direct_route() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice-1",
            "circle-1",
            "alice-contact",
        ));
        seed.sessions.push(direct_session_with_contact(
            "session-alice-2",
            "circle-1",
            "alice-contact",
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-event-ambiguous-direct-1",
            &sender_pubkey,
            "do not guess between duplicate direct sessions",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), current_user_pubkey.clone()]];
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

        assert!(!changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-alice-1")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-alice-2")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_drops_peer_direct_message_when_p_tag_is_not_current_user() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let unrelated_pubkey =
            valid_pubkey_hex("7777777777777777777777777777777777777777777777777777777777777777");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-event-not-for-me-1",
            &sender_pubkey,
            "do not route to my direct chat",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), unrelated_pubkey]];
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

        assert!(!changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-alice")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_drops_peer_direct_message_without_current_user_pubkey() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let current_user_pubkey =
            valid_pubkey_hex("9999999999999999999999999999999999999999999999999999999999999999");
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.contacts.push(contact("alice-contact", &sender_pubkey));
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        let mut inbound_message = inbound_relay_message(
            "relay-event-missing-self-1",
            &sender_pubkey,
            "drop when direct target cannot be confirmed",
        );
        inbound_message
            .signed_nostr_event
            .as_mut()
            .expect("relay message should have signed event")
            .tags = vec![vec!["p".into(), current_user_pubkey]];
        let chat_effects = TransportChatEffects {
            remote_message_merges: vec![MergeRemoteMessagesInput {
                session_id: "session-primary".into(),
                messages: vec![inbound_message],
            }],
            ..TransportChatEffects::default()
        };

        let changed = apply_transport_chat_effects(&mut seed, &chat_effects, None)
            .expect("chat effects apply");

        assert!(!changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-alice")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_preserves_unresolved_peer_media_merge_in_fallback_session() {
        let mut seed = seed(CircleStatus::Open, "18 ms");
        seed.sessions
            .push(direct_session("session-primary", "circle-1"));
        seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        let inbound_message = MessageItem {
            id: "relay-media-1".into(),
            kind: MessageKind::Image,
            author: MessageAuthor::Peer,
            body: "preview.png".into(),
            time: "now".into(),
            meta: Some(
                serde_json::json!({
                    "version": 2,
                    "label": "PNG · 1280 x 720 · 84 KB",
                    "remoteUrl": "https://files.example.test/chat-media/preview.png",
                })
                .to_string(),
            ),
            delivery_status: None,
            remote_id: Some("relay-media-remote-1".into()),
            sync_source: Some(MessageSyncSource::Relay),
            acked_at: None,
            signed_nostr_event: None,
            reply_to: None,
        };
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
        let preserved_messages = seed
            .message_store
            .get("session-primary")
            .expect("fallback session should retain unresolved media merge");
        assert_eq!(preserved_messages.len(), 1);
        assert_eq!(preserved_messages[0].id, "relay-media-1");
        assert!(seed
            .message_store
            .get("session-alice")
            .map_or(true, |messages| messages.is_empty()));
    }

    #[test]
    fn apply_transport_chat_effects_routes_inbound_peer_message_to_matching_group_session() {
        let sender_pubkey = valid_sender_pubkey_hex();
        let other_member_pubkey = valid_group_member_pubkey_hex();
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
    fn apply_transport_chat_effects_drops_peer_group_message_with_unknown_tags() {
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
            inbound_relay_message("relay-group-unknown-tag-1", &sender_pubkey, "do not guess");
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

        assert!(!changed);
        assert!(seed
            .message_store
            .get("session-primary")
            .map_or(true, |messages| messages.is_empty()));
        assert!(seed
            .message_store
            .get("session-design")
            .map_or(true, |messages| messages.is_empty()));
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
    fn local_signed_direct_messages_round_trip_between_two_accounts() {
        const ALICE_SECRET_KEY_HEX: &str =
            "1111111111111111111111111111111111111111111111111111111111111111";
        const BOB_SECRET_KEY_HEX: &str =
            "2222222222222222222222222222222222222222222222222222222222222222";
        let alice_pubkey = valid_pubkey_hex(ALICE_SECRET_KEY_HEX);
        let bob_pubkey = valid_pubkey_hex(BOB_SECRET_KEY_HEX);
        let alice_body = "Hi Bob, checking the signed direct message path.";
        let bob_body = "Received. Signed reply path looks good too.";

        let guard = test_app();
        let app_handle = guard.app.handle();

        let mut alice_seed = seed(CircleStatus::Open, "18 ms");
        alice_seed
            .contacts
            .push(contact("bob-contact", &bob_pubkey));
        alice_seed.sessions.push(direct_session_with_contact(
            "session-bob",
            "circle-1",
            "bob-contact",
        ));
        alice_seed
            .sessions
            .push(self_session("fallback-alice", "circle-1"));
        save_seed_to_store(app_handle, alice_seed);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "session-bob",
            ALICE_SECRET_KEY_HEX,
        );

        let alice_sent_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: "session-bob".into(),
                body: alice_body.into(),
                reply_to_message_id: None,
            },
        )
        .expect("alice should send a signed direct message");
        let alice_sent_message = alice_sent_seed
            .message_store
            .get("session-bob")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("alice outbound message should persist");
        assert!(matches!(
            alice_sent_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        let alice_remote_id = alice_sent_message
            .remote_id
            .clone()
            .expect("alice signed message should have remote id");
        let alice_signed_event = alice_sent_message
            .signed_nostr_event
            .as_ref()
            .expect("alice signed message should include event");
        assert_eq!(alice_signed_event.event_id, alice_remote_id);
        assert_eq!(
            alice_signed_event.tags,
            vec![vec!["p".into(), bob_pubkey.clone()]]
        );
        let alice_seed_after_send = load_seed_from_store(app_handle);

        let mut bob_seed = seed(CircleStatus::Open, "18 ms");
        bob_seed
            .contacts
            .push(contact("alice-contact", &alice_pubkey));
        bob_seed.sessions.push(direct_session_with_contact(
            "session-alice",
            "circle-1",
            "alice-contact",
        ));
        bob_seed
            .sessions
            .push(self_session("fallback-bob", "circle-1"));
        save_seed_to_store(app_handle, bob_seed);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "session-alice",
            BOB_SECRET_KEY_HEX,
        );

        let mut bob_received_seed = load_seed_from_store(app_handle);
        let bob_changed = apply_transport_chat_effects(
            &mut bob_received_seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "fallback-bob".into(),
                    messages: vec![relay_copy_of_signed_local_message(&alice_sent_message)],
                }],
                ..TransportChatEffects::default()
            },
            Some(bob_pubkey.as_str()),
        )
        .expect("bob should merge alice's signed relay message");
        assert!(bob_changed);
        assert!(bob_received_seed
            .message_store
            .get("fallback-bob")
            .map_or(true, |messages| messages.is_empty()));
        let bob_received_message = bob_received_seed
            .message_store
            .get("session-alice")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("bob should receive alice's routed direct message");
        assert_eq!(bob_received_message.id, alice_remote_id);
        assert_eq!(
            bob_received_message.remote_id.as_deref(),
            Some(alice_remote_id.as_str())
        );
        assert!(matches!(bob_received_message.author, MessageAuthor::Peer));
        assert!(matches!(
            bob_received_message.sync_source,
            Some(MessageSyncSource::Relay)
        ));
        let bob_session = bob_received_seed
            .sessions
            .iter()
            .find(|session| session.id == "session-alice")
            .expect("missing bob direct session");
        assert_eq!(bob_session.subtitle, alice_body);
        assert_eq!(bob_session.unread_count, Some(1));
        save_seed_to_store(app_handle, bob_received_seed);

        let bob_reply_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: "session-alice".into(),
                body: bob_body.into(),
                reply_to_message_id: Some(bob_received_message.id.clone()),
            },
        )
        .expect("bob should reply to alice");
        let bob_reply_message = bob_reply_seed
            .message_store
            .get("session-alice")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("bob reply should persist");
        assert!(matches!(
            bob_reply_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        let bob_reply_preview = bob_reply_message
            .reply_to
            .as_ref()
            .expect("bob reply should keep a preview for alice's message");
        assert_eq!(bob_reply_preview.message_id, bob_received_message.id);
        assert_eq!(
            bob_reply_preview.remote_id.as_deref(),
            bob_received_message.remote_id.as_deref()
        );
        assert!(matches!(bob_reply_preview.author, MessageAuthor::Peer));
        assert_eq!(bob_reply_preview.author_label, "Peer");
        assert_eq!(bob_reply_preview.snippet, alice_body);
        let bob_reply_remote_id = bob_reply_message
            .remote_id
            .clone()
            .expect("bob signed reply should have remote id");
        let bob_signed_event = bob_reply_message
            .signed_nostr_event
            .as_ref()
            .expect("bob reply should include signed event");
        assert_eq!(bob_signed_event.event_id, bob_reply_remote_id);
        assert_eq!(
            bob_signed_event.tags,
            vec![
                vec!["p".into(), alice_pubkey.clone()],
                vec!["e".into(), alice_remote_id.clone()],
            ]
        );

        save_seed_to_store(app_handle, alice_seed_after_send);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "session-bob",
            ALICE_SECRET_KEY_HEX,
        );

        let mut alice_received_seed = load_seed_from_store(app_handle);
        let alice_changed = apply_transport_chat_effects(
            &mut alice_received_seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "fallback-alice".into(),
                    messages: vec![relay_copy_of_signed_local_message(&bob_reply_message)],
                }],
                ..TransportChatEffects::default()
            },
            Some(alice_pubkey.as_str()),
        )
        .expect("alice should merge bob's signed relay reply");
        assert!(alice_changed);
        assert!(alice_received_seed
            .message_store
            .get("fallback-alice")
            .map_or(true, |messages| messages.is_empty()));
        let alice_messages = alice_received_seed
            .message_store
            .get("session-bob")
            .expect("alice direct session should keep both messages");
        assert_eq!(alice_messages.len(), 2);
        let alice_received_reply = alice_messages
            .iter()
            .find(|message| message.remote_id.as_deref() == Some(bob_reply_remote_id.as_str()))
            .expect("alice should store bob's reply");
        assert!(matches!(alice_received_reply.author, MessageAuthor::Peer));
        assert!(matches!(
            alice_received_reply.sync_source,
            Some(MessageSyncSource::Relay)
        ));
        let alice_reply_preview = alice_received_reply
            .reply_to
            .as_ref()
            .expect("alice should hydrate reply preview from bob's e tag");
        assert_eq!(alice_reply_preview.message_id, alice_sent_message.id);
        assert_eq!(
            alice_reply_preview.remote_id.as_deref(),
            Some(alice_remote_id.as_str())
        );
        assert!(matches!(alice_reply_preview.author, MessageAuthor::Me));
        assert_eq!(alice_reply_preview.author_label, "You");
        assert_eq!(alice_reply_preview.snippet, alice_body);
        let alice_session = alice_received_seed
            .sessions
            .iter()
            .find(|session| session.id == "session-bob")
            .expect("missing alice direct session");
        assert_eq!(alice_session.subtitle, bob_body);
        assert_eq!(alice_session.unread_count, Some(1));
    }

    #[test]
    fn local_signed_direct_messages_bootstrap_unknown_contact_on_first_inbound_message() {
        const ALICE_SECRET_KEY_HEX: &str =
            "1111111111111111111111111111111111111111111111111111111111111111";
        const BOB_SECRET_KEY_HEX: &str =
            "2222222222222222222222222222222222222222222222222222222222222222";
        let alice_pubkey = valid_pubkey_hex(ALICE_SECRET_KEY_HEX);
        let bob_pubkey = valid_pubkey_hex(BOB_SECRET_KEY_HEX);
        let alice_body = "Hi Bob, this is the first message before you add me.";
        let bob_body = "Received without pre-adding. Reply path also works.";

        let guard = test_app();
        let app_handle = guard.app.handle();

        let mut alice_seed = seed(CircleStatus::Open, "18 ms");
        alice_seed
            .contacts
            .push(contact("bob-contact", &bob_pubkey));
        alice_seed.sessions.push(direct_session_with_contact(
            "session-bob",
            "circle-1",
            "bob-contact",
        ));
        alice_seed
            .sessions
            .push(self_session("fallback-alice", "circle-1"));
        save_seed_to_store(app_handle, alice_seed);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "session-bob",
            ALICE_SECRET_KEY_HEX,
        );

        let alice_sent_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: "session-bob".into(),
                body: alice_body.into(),
                reply_to_message_id: None,
            },
        )
        .expect("alice should send a signed direct message");
        let alice_sent_message = alice_sent_seed
            .message_store
            .get("session-bob")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("alice outbound message should persist");
        let alice_remote_id = alice_sent_message
            .remote_id
            .clone()
            .expect("alice signed message should have remote id");
        let alice_seed_after_send = load_seed_from_store(app_handle);

        let mut bob_seed = seed(CircleStatus::Open, "18 ms");
        bob_seed
            .sessions
            .push(self_session("fallback-bob", "circle-1"));
        save_seed_to_store(app_handle, bob_seed);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "fallback-bob",
            BOB_SECRET_KEY_HEX,
        );

        let mut bob_received_seed = load_seed_from_store(app_handle);
        let bob_changed = apply_transport_chat_effects(
            &mut bob_received_seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "fallback-bob".into(),
                    messages: vec![relay_copy_of_signed_local_message(&alice_sent_message)],
                }],
                ..TransportChatEffects::default()
            },
            Some(bob_pubkey.as_str()),
        )
        .expect("bob should merge alice's signed relay message");
        assert!(bob_changed);
        assert!(bob_received_seed
            .message_store
            .get("fallback-bob")
            .map_or(true, |messages| messages.is_empty()));
        let bob_contact = bob_received_seed
            .contacts
            .iter()
            .find(|contact| contact.pubkey == alice_pubkey)
            .cloned()
            .expect("bob should auto-create alice contact from relay");
        let bob_session = bob_received_seed
            .sessions
            .iter()
            .find(|session| {
                matches!(session.kind, SessionKind::Direct)
                    && session.contact_id.as_deref() == Some(bob_contact.id.as_str())
            })
            .cloned()
            .expect("bob should auto-create direct session from relay");
        let bob_received_message = bob_received_seed
            .message_store
            .get(&bob_session.id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("bob should receive alice's routed direct message");
        assert_eq!(bob_received_message.id, alice_remote_id);
        assert_eq!(bob_session.subtitle, alice_body);
        assert_eq!(bob_session.unread_count, Some(1));
        save_seed_to_store(app_handle, bob_received_seed);

        let bob_reply_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: bob_session.id.clone(),
                body: bob_body.into(),
                reply_to_message_id: Some(bob_received_message.id.clone()),
            },
        )
        .expect("bob should reply from the auto-created direct session");
        let bob_reply_message = bob_reply_seed
            .message_store
            .get(&bob_session.id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("bob reply should persist");
        let bob_reply_remote_id = bob_reply_message
            .remote_id
            .clone()
            .expect("bob signed reply should have remote id");

        save_seed_to_store(app_handle, alice_seed_after_send);
        seed_authenticated_local_secret_runtime_with_secret_key(
            app_handle,
            "session-bob",
            ALICE_SECRET_KEY_HEX,
        );

        let mut alice_received_seed = load_seed_from_store(app_handle);
        let alice_changed = apply_transport_chat_effects(
            &mut alice_received_seed,
            &TransportChatEffects {
                remote_message_merges: vec![MergeRemoteMessagesInput {
                    session_id: "fallback-alice".into(),
                    messages: vec![relay_copy_of_signed_local_message(&bob_reply_message)],
                }],
                ..TransportChatEffects::default()
            },
            Some(alice_pubkey.as_str()),
        )
        .expect("alice should merge bob's signed relay reply");
        assert!(alice_changed);
        let alice_messages = alice_received_seed
            .message_store
            .get("session-bob")
            .expect("alice direct session should keep both messages");
        assert_eq!(alice_messages.len(), 2);
        assert!(alice_messages.iter().any(|message| {
            message.remote_id.as_deref() == Some(bob_reply_remote_id.as_str())
                && matches!(message.author, MessageAuthor::Peer)
                && matches!(message.sync_source, Some(MessageSyncSource::Relay))
        }));
    }

    #[test]
    #[ignore = "manual live public relay service smoke test"]
    fn manual_live_public_relay_snapshot_autostart_publish_smoke() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        ensure_live_runtime_binary_ready();
        let relay_url = manual_live_service_relay_url();
        let contact_pubkey =
            valid_pubkey_hex("6666666666666666666666666666666666666666666666666666666666666666");

        let mut seed = load_seed_from_store(app_handle);
        set_circle_relay(&mut seed, "main-circle", &relay_url);
        save_seed_to_store(app_handle, seed);
        set_contact_pubkey(app_handle, "mika-contact", &contact_pubkey);
        seed_authenticated_local_secret_runtime(app_handle, "mika");

        let sent_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Local transport service live smoke should autostart publish.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local autostart smoke message");
        let sent_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing outbound local autostart smoke message");
        assert!(matches!(
            sent_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));

        let service = LocalTransportService::new(app_handle);
        let snapshot_input = TransportSnapshotInput {
            active_circle_id: Some("main-circle".into()),
            use_tor_network: false,
            experimental_transport: true,
        };

        let initial_snapshot = service
            .load_snapshot(snapshot_input.clone())
            .expect("initial snapshot should auto-launch the live runtime");
        let initial_runtime = initial_snapshot
            .runtime_sessions
            .iter()
            .find(|session| session.circle_id == "main-circle")
            .expect("missing runtime session for main-circle");
        assert!(matches!(
            initial_runtime.desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(
            matches!(
                initial_runtime.last_launch_result,
                Some(TransportRuntimeLaunchResult::Spawned)
                    | Some(TransportRuntimeLaunchResult::Reused)
            ) || matches!(
                initial_runtime.state,
                TransportRuntimeState::Starting | TransportRuntimeState::Active
            ),
            "initial snapshot should have auto-started or observed a live runtime; runtime={initial_runtime:?}"
        );

        let mut background_sync_scheduled = false;
        let mut published = false;
        let mut latest_runtime = initial_runtime.clone();

        for _ in 0..50 {
            thread::sleep(Duration::from_millis(500));
            let snapshot = service
                .load_snapshot(snapshot_input.clone())
                .expect("live autostart smoke snapshot refresh should load");
            if let Some(runtime) = snapshot
                .runtime_sessions
                .iter()
                .find(|session| session.circle_id == "main-circle")
            {
                latest_runtime = runtime.clone();
            }

            let seed = load_seed_from_store(app_handle);
            let cache = load_transport_cache_from_store(app_handle);
            background_sync_scheduled |= cache
                .relay_background_sync_markers
                .iter()
                .any(|marker| marker.circle_id == "main-circle");
            published = seed
                .message_store
                .get("mika")
                .and_then(|messages| {
                    messages
                        .iter()
                        .find(|message| message.id == sent_message.id)
                })
                .is_some_and(|message| {
                    matches!(message.delivery_status, Some(MessageDeliveryStatus::Sent))
                        && message.remote_id == sent_message.remote_id
                        && message
                            .signed_nostr_event
                            .as_ref()
                            .map(|event| event.event_id.as_str())
                            == sent_message
                                .signed_nostr_event
                                .as_ref()
                                .map(|event| event.event_id.as_str())
                });

            if published
                || (background_sync_scheduled
                    && matches!(latest_runtime.state, TransportRuntimeState::Inactive))
            {
                break;
            }
        }

        let _ = service.apply_circle_action(TransportCircleActionInput {
            circle_id: "main-circle".into(),
            action: TransportCircleAction::Disconnect,
            active_circle_id: Some("main-circle".into()),
            use_tor_network: false,
            experimental_transport: true,
            sync_since_created_at: None,
        });

        let final_seed = load_seed_from_store(app_handle);
        let final_cache = load_transport_cache_from_store(app_handle);
        let final_message = final_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == sent_message.id)
            })
            .cloned();

        assert!(
            matches!(
                latest_runtime.state,
                TransportRuntimeState::Active | TransportRuntimeState::Starting
            ),
            "latest runtime session={latest_runtime:?}"
        );
        assert!(
            background_sync_scheduled,
            "load_snapshot should schedule automatic background relay sync for the auto-started live runtime; markers={:?}",
            final_cache.relay_background_sync_markers
        );
        assert!(
            published,
            "local signed message should publish through the auto-started runtime on `{relay_url}`; latest_runtime={latest_runtime:?}; final_message={final_message:?}; dispatches={:?}; activities={:?}",
            final_cache.outbound_dispatches,
            final_cache.activities
        );
    }

    #[test]
    #[ignore = "manual live public relay service smoke test"]
    fn manual_live_public_relay_connect_publish_smoke() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        ensure_live_runtime_binary_ready();
        let relay_url = manual_live_service_relay_url();
        let contact_pubkey =
            valid_pubkey_hex("6666666666666666666666666666666666666666666666666666666666666666");

        let mut seed = load_seed_from_store(app_handle);
        set_circle_relay(&mut seed, "main-circle", &relay_url);
        save_seed_to_store(app_handle, seed);
        set_contact_pubkey(app_handle, "mika-contact", &contact_pubkey);
        seed_authenticated_local_secret_runtime(app_handle, "mika");

        let sent_seed = chat_mutations::send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Local transport service live smoke should auto-publish.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local smoke message");
        let sent_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing outbound local smoke message");
        assert!(matches!(
            sent_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));

        let service = LocalTransportService::new(app_handle);
        let snapshot_input = TransportSnapshotInput {
            active_circle_id: Some("main-circle".into()),
            use_tor_network: false,
            experimental_transport: true,
        };

        let connect = service
            .apply_circle_action(TransportCircleActionInput {
                circle_id: "main-circle".into(),
                action: TransportCircleAction::Connect,
                active_circle_id: Some("main-circle".into()),
                use_tor_network: false,
                experimental_transport: true,
                sync_since_created_at: None,
            })
            .expect("connect action should launch the live runtime");
        let initial_runtime = connect
            .snapshot
            .runtime_sessions
            .iter()
            .find(|session| session.circle_id == "main-circle")
            .expect("missing runtime session for main-circle");
        assert!(matches!(
            initial_runtime.desired_state,
            TransportRuntimeDesiredState::Running
        ));
        assert!(matches!(
            initial_runtime.state,
            TransportRuntimeState::Starting | TransportRuntimeState::Active
        ));

        let mut background_sync_scheduled = false;
        let mut published = false;
        let mut latest_runtime = initial_runtime.clone();

        for _ in 0..50 {
            thread::sleep(Duration::from_millis(500));
            let snapshot = service
                .load_snapshot(snapshot_input.clone())
                .expect("live smoke snapshot refresh should load");
            if let Some(runtime) = snapshot
                .runtime_sessions
                .iter()
                .find(|session| session.circle_id == "main-circle")
            {
                latest_runtime = runtime.clone();
            }

            let seed = load_seed_from_store(app_handle);
            let cache = load_transport_cache_from_store(app_handle);
            background_sync_scheduled |= cache
                .relay_background_sync_markers
                .iter()
                .any(|marker| marker.circle_id == "main-circle");
            published = seed
                .message_store
                .get("mika")
                .and_then(|messages| {
                    messages
                        .iter()
                        .find(|message| message.id == sent_message.id)
                })
                .is_some_and(|message| {
                    matches!(message.delivery_status, Some(MessageDeliveryStatus::Sent))
                        && message.remote_id == sent_message.remote_id
                        && message
                            .signed_nostr_event
                            .as_ref()
                            .map(|event| event.event_id.as_str())
                            == sent_message
                                .signed_nostr_event
                                .as_ref()
                                .map(|event| event.event_id.as_str())
                });

            if published
                || (background_sync_scheduled
                    && matches!(latest_runtime.state, TransportRuntimeState::Inactive))
            {
                break;
            }
        }

        let _ = service.apply_circle_action(TransportCircleActionInput {
            circle_id: "main-circle".into(),
            action: TransportCircleAction::Disconnect,
            active_circle_id: Some("main-circle".into()),
            use_tor_network: false,
            experimental_transport: true,
            sync_since_created_at: None,
        });

        let final_seed = load_seed_from_store(app_handle);
        let final_cache = load_transport_cache_from_store(app_handle);
        let final_message = final_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == sent_message.id)
            })
            .cloned();

        assert!(
            matches!(
                latest_runtime.state,
                TransportRuntimeState::Active | TransportRuntimeState::Starting
            ),
            "latest runtime session={latest_runtime:?}"
        );
        assert!(
            background_sync_scheduled,
            "load_snapshot should schedule automatic background relay sync for the connected live runtime; markers={:?}",
            final_cache.relay_background_sync_markers
        );
        assert!(
            published,
            "local signed message should publish through the connected runtime on `{relay_url}`; latest_runtime={latest_runtime:?}; final_message={final_message:?}; dispatches={:?}; activities={:?}",
            final_cache.outbound_dispatches,
            final_cache.activities
        );
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
