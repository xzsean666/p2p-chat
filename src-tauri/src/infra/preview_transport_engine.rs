use crate::domain::chat::{ChatDomainSeed, CircleStatus, CircleType, MessageKind};
use crate::domain::transport::{
    PeerPresence, RelayProtocol, SessionSyncState, TransportCircleAction,
    TransportCircleActionInput, TransportEngineKind, TransportHealth,
};
use crate::domain::transport_engine::{TransportEngine, TransportEngineState};
use crate::domain::transport_repository::TransportCache;
use crate::infra::transport_state_builder::{
    apply_transport_circle_action_to_seed, build_transport_engine_state,
    build_transport_engine_state_after_action,
};

pub struct PreviewTransportEngine;

impl TransportEngine for PreviewTransportEngine {
    fn kind(&self) -> TransportEngineKind {
        TransportEngineKind::NativePreview
    }

    fn build_state(
        &self,
        seed: &ChatDomainSeed,
        previous_cache: &TransportCache,
    ) -> Result<TransportEngineState, String> {
        let mut state = build_transport_engine_state(self.kind(), seed, previous_cache);
        apply_preview_state_adjustments(&mut state, None);
        Ok(state)
    }

    fn apply_circle_action(
        &self,
        seed: &mut ChatDomainSeed,
        previous_cache: &TransportCache,
        input: &TransportCircleActionInput,
    ) -> Result<TransportEngineState, String> {
        apply_transport_circle_action_to_seed(seed, input)?;
        apply_preview_seed_adjustments(seed, input);
        let mut state =
            build_transport_engine_state_after_action(self.kind(), seed, previous_cache, input)?;
        apply_preview_state_adjustments(&mut state, Some(&input.action));
        rewrite_preview_sync_effects(&mut state, input.use_tor_network);
        Ok(state)
    }
}

fn apply_preview_seed_adjustments(seed: &mut ChatDomainSeed, input: &TransportCircleActionInput) {
    let session_count = seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == input.circle_id && !session.archived.unwrap_or(false)
        })
        .count() as u32;

    if let Some(circle) = seed
        .circles
        .iter_mut()
        .find(|circle| circle.id == input.circle_id)
    {
        match input.action {
            TransportCircleAction::Connect => {
                if !circle.relay.starts_with("invite://") {
                    circle.status = CircleStatus::Open;
                    circle.latency = format!(
                        "{} ms",
                        preview_latency_ms(
                            &circle.relay,
                            &circle.circle_type,
                            input.use_tor_network,
                            input.experimental_transport,
                        )
                    );
                }
            }
            TransportCircleAction::DiscoverPeers => {
                if session_count > 0 && !circle.relay.starts_with("invite://") {
                    circle.status = CircleStatus::Open;
                    circle.latency = format!(
                        "{} ms",
                        preview_latency_ms(
                            &circle.relay,
                            &circle.circle_type,
                            input.use_tor_network,
                            input.experimental_transport,
                        )
                    );
                }
            }
            TransportCircleAction::Sync | TransportCircleAction::SyncSessions => {
                circle.status = CircleStatus::Open;
                circle.latency = format!(
                    "{} ms",
                    preview_latency_ms(
                        &circle.relay,
                        &circle.circle_type,
                        input.use_tor_network,
                        input.experimental_transport,
                    )
                );
            }
            TransportCircleAction::Disconnect => {}
        }
    }
}

fn apply_preview_state_adjustments(
    state: &mut TransportEngineState,
    action: Option<&TransportCircleAction>,
) {
    for diagnostic in &mut state.diagnostics {
        diagnostic.last_sync = preview_last_sync(&diagnostic.protocol, &diagnostic.health, action);

        if let Some(latency_ms) = &mut diagnostic.latency_ms {
            *latency_ms = latency_ms.saturating_sub(preview_latency_discount(&diagnostic.protocol));
        }

        if matches!(diagnostic.health, TransportHealth::Online) {
            diagnostic.peer_count = diagnostic.peer_count.saturating_add(1);
            diagnostic.reachable = true;
            diagnostic.queued_messages = diagnostic.queued_messages.saturating_sub(1);
        }
    }

    for peer in &mut state.cache.peers {
        peer.route = label_with_native_suffix(&peer.route);

        if peer.blocked {
            continue;
        }

        peer.last_seen = match action {
            Some(TransportCircleAction::Connect) => "native engine boot".into(),
            Some(TransportCircleAction::DiscoverPeers) => "native discovery sweep".into(),
            Some(TransportCircleAction::Sync) => "native relay sync".into(),
            Some(TransportCircleAction::SyncSessions) => "native merge relay".into(),
            Some(TransportCircleAction::Disconnect) => "relay offline".into(),
            None => match peer.presence {
                PeerPresence::Online => "native session active".into(),
                PeerPresence::Idle => "native standby".into(),
                PeerPresence::Offline => "relay offline".into(),
            },
        };

        if matches!(
            action,
            Some(TransportCircleAction::Connect | TransportCircleAction::DiscoverPeers)
        ) && matches!(peer.presence, PeerPresence::Offline)
        {
            peer.presence = PeerPresence::Idle;
        }

        if matches!(
            action,
            Some(TransportCircleAction::Sync | TransportCircleAction::SyncSessions)
        ) && matches!(peer.presence, PeerPresence::Idle)
        {
            peer.presence = PeerPresence::Online;
        }
    }

    for item in &mut state.cache.session_sync {
        item.source = label_with_native_suffix(&item.source);

        if matches!(
            action,
            Some(TransportCircleAction::Connect | TransportCircleAction::DiscoverPeers)
        ) && matches!(item.state, SessionSyncState::Idle)
        {
            item.state = SessionSyncState::Syncing;
        }

        if matches!(
            action,
            Some(TransportCircleAction::Sync | TransportCircleAction::SyncSessions)
        ) && !matches!(
            item.state,
            SessionSyncState::Pending | SessionSyncState::Conflict
        ) {
            item.state = SessionSyncState::Idle;
        }

        item.last_merge = match action {
            Some(TransportCircleAction::Connect) => {
                preview_merge_label(&item.last_merge, "native engine boot")
            }
            Some(TransportCircleAction::DiscoverPeers) => {
                preview_merge_label(&item.last_merge, "native peer sweep")
            }
            Some(TransportCircleAction::Sync) => {
                preview_merge_label(&item.last_merge, "native relay sync")
            }
            Some(TransportCircleAction::SyncSessions) => {
                if matches!(item.state, SessionSyncState::Conflict) {
                    item.last_merge.clone()
                } else {
                    "native merge complete".into()
                }
            }
            Some(TransportCircleAction::Disconnect) => "offline".into(),
            None => preview_merge_label(&item.last_merge, "native checkpoint"),
        };
    }
}

fn preview_last_sync(
    protocol: &RelayProtocol,
    health: &TransportHealth,
    action: Option<&TransportCircleAction>,
) -> String {
    if matches!(health, TransportHealth::Offline) {
        return "offline".into();
    }

    if matches!(action, Some(TransportCircleAction::Connect)) {
        return match protocol {
            RelayProtocol::Mesh => "native mesh runtime boot".into(),
            RelayProtocol::Invite => "native invite runtime boot".into(),
            RelayProtocol::Websocket => "native websocket runtime boot".into(),
        };
    }

    if matches!(action, Some(TransportCircleAction::DiscoverPeers)) {
        return match protocol {
            RelayProtocol::Mesh => "native mesh discovery sweep".into(),
            RelayProtocol::Invite => "native invite peer resolve".into(),
            RelayProtocol::Websocket => "native relay peer sweep".into(),
        };
    }

    if matches!(
        action,
        Some(TransportCircleAction::Sync | TransportCircleAction::SyncSessions)
    ) {
        return match protocol {
            RelayProtocol::Mesh => "native mesh checkpoint".into(),
            RelayProtocol::Invite => "native invite checkpoint".into(),
            RelayProtocol::Websocket => "native relay checkpoint".into(),
        };
    }

    match (protocol, health) {
        (RelayProtocol::Mesh, TransportHealth::Online) => "native mesh runtime active".into(),
        (RelayProtocol::Invite, TransportHealth::Online) => "native invite preview active".into(),
        (RelayProtocol::Websocket, TransportHealth::Online) => "native relay runtime active".into(),
        (RelayProtocol::Mesh, TransportHealth::Degraded) => "native mesh runtime warmup".into(),
        (RelayProtocol::Invite, TransportHealth::Degraded) => "native invite warmup".into(),
        (RelayProtocol::Websocket, TransportHealth::Degraded) => "native relay warmup".into(),
        (_, TransportHealth::Offline) => "offline".into(),
    }
}

fn preview_latency_discount(protocol: &RelayProtocol) -> u32 {
    match protocol {
        RelayProtocol::Mesh => 6,
        RelayProtocol::Invite => 10,
        RelayProtocol::Websocket => 8,
    }
}

fn preview_latency_ms(
    relay: &str,
    circle_type: &CircleType,
    use_tor_network: bool,
    experimental_transport: bool,
) -> u32 {
    let base_latency = if relay.starts_with("mesh://") {
        12
    } else if relay.starts_with("invite://") {
        48
    } else {
        24
    };
    let circle_penalty = match circle_type {
        CircleType::Paid => 14,
        CircleType::Bitchat => 4,
        CircleType::Custom => 6,
        CircleType::Default => 0,
    };
    let tor_penalty = if use_tor_network { 20 } else { 0 };
    let experimental_adjustment = if experimental_transport { 4 } else { 0 };

    base_latency + circle_penalty + tor_penalty + experimental_adjustment
}

fn rewrite_preview_sync_effects(state: &mut TransportEngineState, use_tor_network: bool) {
    let body = if use_tor_network {
        "Native preview transport synced this circle through the privacy path."
    } else {
        "Native preview transport synced this circle through the local runtime."
    };

    for merge in &mut state.chat_effects.remote_message_merges {
        if let Some(message) = merge.messages.iter_mut().rev().find(|message| {
            matches!(message.kind, MessageKind::System) && message.id.contains("-sync-")
        }) {
            message.body = body.into();
            break;
        }
    }
}

fn label_with_native_suffix(label: &str) -> String {
    if label.contains("native") {
        label.into()
    } else {
        format!("{label} · native")
    }
}

fn preview_merge_label(current: &str, next: &str) -> String {
    if current == "offline" || current == "pending merge" {
        current.into()
    } else {
        next.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        CircleItem, CircleStatus, CircleType, ContactItem, GroupProfile, MessageSyncSource,
        SessionItem, SessionKind,
    };
    use crate::domain::transport::TransportActivityKind;
    use std::collections::HashMap;

    #[test]
    fn preview_engine_reports_preview_kind() {
        let engine = PreviewTransportEngine;
        let state = engine
            .build_state(
                &ChatDomainSeed {
                    circles: vec![CircleItem {
                        id: "circle-1".into(),
                        name: "Circle".into(),
                        relay: "wss://relay.example.com".into(),
                        circle_type: CircleType::Default,
                        status: CircleStatus::Open,
                        latency: "42 ms".into(),
                        description: "test".into(),
                    }],
                    contacts: vec![ContactItem {
                        id: "contact-1".into(),
                        name: "Alex".into(),
                        initials: "A".into(),
                        handle: "@alex".into(),
                        pubkey: "pubkey".into(),
                        ethereum_address: None,
                        subtitle: "friend".into(),
                        bio: "bio".into(),
                        online: Some(true),
                        blocked: Some(false),
                    }],
                    sessions: vec![SessionItem {
                        id: "session-1".into(),
                        circle_id: "circle-1".into(),
                        name: "Alex".into(),
                        initials: "A".into(),
                        subtitle: "hello".into(),
                        time: "today".into(),
                        unread_count: None,
                        muted: None,
                        pinned: None,
                        draft: None,
                        kind: SessionKind::Direct,
                        category: "friends".into(),
                        members: None,
                        contact_id: Some("contact-1".into()),
                        archived: Some(false),
                    }],
                    groups: Vec::<GroupProfile>::new(),
                    message_store: HashMap::from([(String::from("session-1"), Vec::new())]),
                },
                &TransportCache::default(),
            )
            .expect("preview engine state should build");

        assert!(matches!(state.kind, TransportEngineKind::NativePreview));
    }

    #[test]
    fn preview_engine_connect_promotes_websocket_circle_to_open() {
        let engine = PreviewTransportEngine;
        let mut seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "wss://relay.example.com".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Closed,
                latency: "--".into(),
                description: "test".into(),
            }],
            contacts: vec![ContactItem {
                id: "contact-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                handle: "@alex".into(),
                pubkey: "pubkey".into(),
                ethereum_address: None,
                subtitle: "friend".into(),
                bio: "bio".into(),
                online: Some(true),
                blocked: Some(false),
            }],
            sessions: vec![SessionItem {
                id: "session-1".into(),
                circle_id: "circle-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                subtitle: "hello".into(),
                time: "today".into(),
                unread_count: None,
                muted: None,
                pinned: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                contact_id: Some("contact-1".into()),
                archived: Some(false),
            }],
            groups: Vec::<GroupProfile>::new(),
            message_store: HashMap::from([(String::from("session-1"), Vec::new())]),
        };

        let state = engine
            .apply_circle_action(
                &mut seed,
                &TransportCache::default(),
                &TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::Connect,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                },
            )
            .expect("preview connect should succeed");

        assert!(matches!(seed.circles[0].status, CircleStatus::Open));
        assert_eq!(seed.circles[0].latency, "28 ms");
        assert!(state.diagnostics[0].last_sync.contains("native"));
        assert!(matches!(state.kind, TransportEngineKind::NativePreview));
        assert!(matches!(
            state.cache.activities[0].kind,
            TransportActivityKind::Connect
        ));
    }

    #[test]
    fn preview_engine_sync_sessions_rewrites_system_message_in_chat_effects() {
        let engine = PreviewTransportEngine;
        let mut seed = ChatDomainSeed {
            circles: vec![CircleItem {
                id: "circle-1".into(),
                name: "Circle".into(),
                relay: "mesh://circle-1".into(),
                circle_type: CircleType::Default,
                status: CircleStatus::Open,
                latency: "18 ms".into(),
                description: "test".into(),
            }],
            contacts: vec![ContactItem {
                id: "contact-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                handle: "@alex".into(),
                pubkey: "pubkey".into(),
                ethereum_address: None,
                subtitle: "friend".into(),
                bio: "bio".into(),
                online: Some(true),
                blocked: Some(false),
            }],
            sessions: vec![SessionItem {
                id: "session-1".into(),
                circle_id: "circle-1".into(),
                name: "Alex".into(),
                initials: "A".into(),
                subtitle: "hello".into(),
                time: "today".into(),
                unread_count: Some(2),
                muted: None,
                pinned: None,
                draft: None,
                kind: SessionKind::Direct,
                category: "friends".into(),
                members: None,
                contact_id: Some("contact-1".into()),
                archived: Some(false),
            }],
            groups: Vec::<GroupProfile>::new(),
            message_store: HashMap::from([(String::from("session-1"), Vec::new())]),
        };

        let state = engine
            .apply_circle_action(
                &mut seed,
                &TransportCache::default(),
                &TransportCircleActionInput {
                    circle_id: "circle-1".into(),
                    action: TransportCircleAction::SyncSessions,
                    active_circle_id: Some("circle-1".into()),
                    use_tor_network: false,
                    experimental_transport: true,
                    sync_since_created_at: None,
                },
            )
            .expect("preview sync should succeed");

        assert!(seed.message_store["session-1"].is_empty());
        assert_eq!(seed.sessions[0].subtitle, "hello");
        assert_eq!(state.chat_effects.remote_message_merges.len(), 1);
        let message_merge = &state.chat_effects.remote_message_merges[0];
        assert_eq!(message_merge.session_id, "session-1");
        assert!(message_merge.messages.iter().any(|message| {
            matches!(message.kind, MessageKind::Text)
                && message.body == "Alex sent a fresh relay update."
                && matches!(message.sync_source, Some(MessageSyncSource::Relay))
                && message.remote_id.is_some()
        }));
        assert!(message_merge.messages.iter().any(|message| {
            matches!(message.kind, MessageKind::System)
                && message.id.contains("-sync-")
                && message.body
                    == "Native preview transport synced this circle through the local runtime."
                && matches!(message.sync_source, Some(MessageSyncSource::System))
        }));
    }
}
