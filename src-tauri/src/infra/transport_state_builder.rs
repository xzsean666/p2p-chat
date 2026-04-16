use crate::domain::chat::{
    ChatDomainSeed, ContactItem, MessageAuthor, MessageItem, MessageKind, SessionItem, SessionKind,
};
use crate::domain::transport::{
    CircleTransportDiagnostic, DiscoveredPeer, PeerPresence, SessionSyncItem, SessionSyncState,
    TransportActivityItem, TransportActivityKind, TransportActivityLevel, TransportCircleAction,
    TransportCircleActionInput, TransportEngineKind, TransportHealth,
};
use crate::domain::transport_adapter::{TransportAdapter, TransportRuntimeOptions};
use crate::domain::transport_engine::TransportEngineState;
use crate::domain::transport_repository::TransportCache;
use crate::infra::mock_transport_adapters::{adapter_for_protocol, adapter_for_relay};
use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn build_transport_engine_state(
    kind: TransportEngineKind,
    seed: &ChatDomainSeed,
    previous_cache: &TransportCache,
) -> TransportEngineState {
    let diagnostics = build_diagnostics(seed);
    let cache = build_transport_cache(&kind, seed, &diagnostics, previous_cache, None);

    TransportEngineState {
        kind,
        diagnostics,
        cache,
    }
}

pub(crate) fn build_transport_engine_state_after_action(
    kind: TransportEngineKind,
    seed: &ChatDomainSeed,
    previous_cache: &TransportCache,
    circle_id: &str,
    action: &TransportCircleAction,
) -> Result<TransportEngineState, String> {
    let diagnostics = build_diagnostics(seed);
    let mut cache = build_transport_cache(
        &kind,
        seed,
        &diagnostics,
        previous_cache,
        Some((circle_id, action)),
    );
    let adapter = adapter_for_circle(seed, circle_id)?;
    adapter.apply_cache_action(&mut cache.peers, &mut cache.session_sync, action, circle_id);

    Ok(TransportEngineState {
        kind,
        diagnostics,
        cache,
    })
}

pub(crate) fn apply_transport_circle_action_to_seed(
    seed: &mut ChatDomainSeed,
    input: &TransportCircleActionInput,
) -> Result<(), String> {
    let runtime = TransportRuntimeOptions {
        use_tor_network: input.use_tor_network,
        experimental_transport: input.experimental_transport,
    };
    {
        let circle = seed
            .circles
            .iter_mut()
            .find(|circle| circle.id == input.circle_id)
            .ok_or_else(|| format!("circle not found: {}", input.circle_id))?;
        let adapter = adapter_for_relay(&circle.relay);
        adapter.apply_circle_action(circle, &input.action, runtime);
    }

    if matches!(input.action, TransportCircleAction::SyncSessions) {
        apply_session_sync(
            seed,
            &input.circle_id,
            input.use_tor_network,
            input.experimental_transport,
        );
    }

    Ok(())
}

fn adapter_for_circle(
    seed: &ChatDomainSeed,
    circle_id: &str,
) -> Result<&'static dyn TransportAdapter, String> {
    let relay = seed
        .circles
        .iter()
        .find(|circle| circle.id == circle_id)
        .map(|circle| circle.relay.as_str())
        .ok_or_else(|| format!("circle not found: {circle_id}"))?;

    Ok(adapter_for_relay(relay))
}

fn build_diagnostics(seed: &ChatDomainSeed) -> Vec<CircleTransportDiagnostic> {
    seed.circles
        .iter()
        .map(|circle| {
            let session_count = seed
                .sessions
                .iter()
                .filter(|session| session.circle_id == circle.id)
                .count() as u32;
            adapter_for_relay(&circle.relay).build_diagnostic(circle, session_count)
        })
        .collect()
}

fn build_transport_cache(
    kind: &TransportEngineKind,
    seed: &ChatDomainSeed,
    diagnostics: &[CircleTransportDiagnostic],
    previous_cache: &TransportCache,
    recent_action: Option<(&str, &TransportCircleAction)>,
) -> TransportCache {
    let peers = merge_discovered_peers(
        build_discovered_peers(seed, diagnostics),
        diagnostics,
        previous_cache,
    );
    let session_sync =
        merge_session_sync_items(build_session_sync_items(seed, diagnostics), previous_cache);
    let activities =
        merge_transport_activity_items(kind, diagnostics, previous_cache, recent_action);

    TransportCache {
        peers,
        session_sync,
        activities,
    }
}

fn build_discovered_peers(
    seed: &ChatDomainSeed,
    diagnostics: &[CircleTransportDiagnostic],
) -> Vec<DiscoveredPeer> {
    let contact_index = seed
        .contacts
        .iter()
        .map(|contact| (contact.id.clone(), contact))
        .collect::<HashMap<_, _>>();
    let group_index = seed
        .groups
        .iter()
        .map(|group| (group.session_id.clone(), group))
        .collect::<HashMap<_, _>>();
    let mut peer_sessions = BTreeMap::<(String, String), u32>::new();

    for session in &seed.sessions {
        if let Some(contact_id) = &session.contact_id {
            let entry = peer_sessions
                .entry((session.circle_id.clone(), contact_id.clone()))
                .or_insert(0);
            *entry += 1;
        }

        if matches!(session.kind, SessionKind::Group) {
            if let Some(group) = group_index.get(&session.id) {
                for member in &group.members {
                    let entry = peer_sessions
                        .entry((session.circle_id.clone(), member.contact_id.clone()))
                        .or_insert(0);
                    *entry += 1;
                }
            }
        }
    }

    peer_sessions
        .into_iter()
        .filter_map(|((circle_id, contact_id), shared_sessions)| {
            let contact = contact_index.get(&contact_id)?;
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.circle_id == circle_id)?;

            Some(DiscoveredPeer {
                circle_id,
                contact_id,
                name: contact.name.clone(),
                handle: contact.handle.clone(),
                presence: peer_presence(contact),
                route: adapter_for_protocol(&diagnostic.protocol)
                    .route_label()
                    .into(),
                shared_sessions,
                last_seen: peer_last_seen(contact),
                blocked: contact.blocked.unwrap_or(false),
            })
        })
        .collect()
}

fn build_session_sync_items(
    seed: &ChatDomainSeed,
    diagnostics: &[CircleTransportDiagnostic],
) -> Vec<SessionSyncItem> {
    seed.sessions
        .iter()
        .filter_map(|session| {
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.circle_id == session.circle_id)?;
            let pending_messages = session.unread_count.unwrap_or(0);

            Some(SessionSyncItem {
                circle_id: session.circle_id.clone(),
                session_id: session.id.clone(),
                session_name: session.name.clone(),
                state: session_sync_state(session, diagnostic, pending_messages),
                pending_messages,
                source: adapter_for_protocol(&diagnostic.protocol)
                    .route_label()
                    .into(),
                last_merge: if matches!(diagnostic.health, TransportHealth::Offline) {
                    "offline".into()
                } else if pending_messages > 0 {
                    "pending merge".into()
                } else {
                    session.time.clone()
                },
            })
        })
        .collect()
}

fn merge_discovered_peers(
    derived_peers: Vec<DiscoveredPeer>,
    diagnostics: &[CircleTransportDiagnostic],
    previous_cache: &TransportCache,
) -> Vec<DiscoveredPeer> {
    let cached_index = previous_cache
        .peers
        .iter()
        .map(|peer| ((peer.circle_id.clone(), peer.contact_id.clone()), peer))
        .collect::<HashMap<_, _>>();

    derived_peers
        .into_iter()
        .map(|mut peer| {
            let circle_health = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.circle_id == peer.circle_id)
                .map(|diagnostic| &diagnostic.health);

            if peer.blocked {
                peer.presence = PeerPresence::Offline;
                peer.last_seen = "blocked".into();
                return peer;
            }

            if matches!(circle_health, Some(TransportHealth::Offline)) {
                peer.presence = PeerPresence::Offline;
                peer.last_seen = "relay offline".into();
                return peer;
            }

            if let Some(cached) =
                cached_index.get(&(peer.circle_id.clone(), peer.contact_id.clone()))
            {
                peer.presence = cached.presence.clone();
                peer.last_seen = cached.last_seen.clone();
            }

            peer
        })
        .collect()
}

fn merge_session_sync_items(
    derived_items: Vec<SessionSyncItem>,
    previous_cache: &TransportCache,
) -> Vec<SessionSyncItem> {
    let cached_index = previous_cache
        .session_sync
        .iter()
        .map(|item| ((item.circle_id.clone(), item.session_id.clone()), item))
        .collect::<HashMap<_, _>>();

    derived_items
        .into_iter()
        .map(|mut item| {
            if let Some(cached) =
                cached_index.get(&(item.circle_id.clone(), item.session_id.clone()))
            {
                if matches!(item.state, SessionSyncState::Idle)
                    && matches!(cached.state, SessionSyncState::Syncing)
                {
                    item.state = SessionSyncState::Syncing;
                }

                if item.last_merge != "offline" && item.last_merge != "pending merge" {
                    item.last_merge = cached.last_merge.clone();
                }
            }

            item
        })
        .collect()
}

fn merge_transport_activity_items(
    kind: &TransportEngineKind,
    diagnostics: &[CircleTransportDiagnostic],
    previous_cache: &TransportCache,
    recent_action: Option<(&str, &TransportCircleAction)>,
) -> Vec<TransportActivityItem> {
    let valid_circle_ids = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.circle_id.as_str())
        .collect::<Vec<_>>();
    let mut activities = previous_cache
        .activities
        .iter()
        .filter(|item| {
            valid_circle_ids
                .iter()
                .any(|circle_id| *circle_id == item.circle_id)
        })
        .cloned()
        .collect::<Vec<_>>();

    for diagnostic in diagnostics {
        let runtime_activity = build_runtime_activity_item(kind, diagnostic);

        if let Some(existing) = activities.iter_mut().find(|item| {
            item.circle_id == diagnostic.circle_id
                && matches!(item.kind, TransportActivityKind::Runtime)
        }) {
            existing.level = runtime_activity.level;
            existing.title = runtime_activity.title;
            existing.detail = runtime_activity.detail;
            existing.time = runtime_activity.time;
        } else {
            activities.push(runtime_activity);
        }
    }

    if let Some((circle_id, action)) = recent_action {
        if let Some(diagnostic) = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.circle_id == circle_id)
        {
            activities.insert(0, build_action_activity_item(kind, diagnostic, action));
        }
    }

    trim_transport_activity_items(activities)
}

fn build_runtime_activity_item(
    kind: &TransportEngineKind,
    diagnostic: &CircleTransportDiagnostic,
) -> TransportActivityItem {
    let engine_label = engine_label(kind);
    let protocol_label = protocol_label(&diagnostic.protocol);
    let (title, level) = match kind {
        TransportEngineKind::NativePreview => match diagnostic.health {
            TransportHealth::Online => ("Native runtime active", TransportActivityLevel::Success),
            TransportHealth::Degraded => ("Native runtime warmup", TransportActivityLevel::Info),
            TransportHealth::Offline => ("Native runtime offline", TransportActivityLevel::Warn),
        },
        TransportEngineKind::Mock => match diagnostic.health {
            TransportHealth::Online => ("Relay online", TransportActivityLevel::Success),
            TransportHealth::Degraded => ("Relay warming up", TransportActivityLevel::Info),
            TransportHealth::Offline => ("Relay offline", TransportActivityLevel::Warn),
        },
    };

    TransportActivityItem {
        id: format!("runtime-{}", diagnostic.circle_id),
        circle_id: diagnostic.circle_id.clone(),
        kind: TransportActivityKind::Runtime,
        level,
        title: title.into(),
        detail: format!(
            "{} via {} · {} peers · {} queued",
            protocol_label, engine_label, diagnostic.peer_count, diagnostic.queued_messages
        ),
        time: diagnostic.last_sync.clone(),
    }
}

fn build_action_activity_item(
    kind: &TransportEngineKind,
    diagnostic: &CircleTransportDiagnostic,
    action: &TransportCircleAction,
) -> TransportActivityItem {
    let protocol_label = protocol_label(&diagnostic.protocol);
    let (title, level) = match (kind, action) {
        (TransportEngineKind::NativePreview, TransportCircleAction::Connect) => {
            ("Native runtime booted", TransportActivityLevel::Success)
        }
        (TransportEngineKind::Mock, TransportCircleAction::Connect) => (
            "Relay handshake started",
            activity_level_from_health(&diagnostic.health),
        ),
        (TransportEngineKind::NativePreview, TransportCircleAction::Disconnect) => {
            ("Native runtime stopped", TransportActivityLevel::Warn)
        }
        (TransportEngineKind::Mock, TransportCircleAction::Disconnect) => {
            ("Relay disconnected", TransportActivityLevel::Warn)
        }
        (TransportEngineKind::NativePreview, TransportCircleAction::DiscoverPeers) => (
            "Native peer sweep finished",
            activity_level_from_health(&diagnostic.health),
        ),
        (TransportEngineKind::Mock, TransportCircleAction::DiscoverPeers) => (
            "Peer discovery sweep finished",
            activity_level_from_health(&diagnostic.health),
        ),
        (TransportEngineKind::NativePreview, TransportCircleAction::Sync) => (
            "Native relay checkpoint saved",
            activity_level_from_health(&diagnostic.health),
        ),
        (TransportEngineKind::Mock, TransportCircleAction::Sync) => (
            "Relay sync finished",
            activity_level_from_health(&diagnostic.health),
        ),
        (TransportEngineKind::NativePreview, TransportCircleAction::SyncSessions) => (
            "Native session merge committed",
            TransportActivityLevel::Success,
        ),
        (TransportEngineKind::Mock, TransportCircleAction::SyncSessions) => {
            ("Session merge committed", TransportActivityLevel::Success)
        }
    };

    TransportActivityItem {
        id: unique_activity_id(&diagnostic.circle_id, action),
        circle_id: diagnostic.circle_id.clone(),
        kind: activity_kind(action),
        level,
        title: title.into(),
        detail: format!(
            "{} · {} peers · {} queued · {}",
            protocol_label, diagnostic.peer_count, diagnostic.queued_messages, diagnostic.last_sync
        ),
        time: "now".into(),
    }
}

fn trim_transport_activity_items(
    activities: Vec<TransportActivityItem>,
) -> Vec<TransportActivityItem> {
    let mut per_circle_counts = HashMap::<String, u32>::new();

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

fn activity_kind(action: &TransportCircleAction) -> TransportActivityKind {
    match action {
        TransportCircleAction::Connect => TransportActivityKind::Connect,
        TransportCircleAction::Disconnect => TransportActivityKind::Disconnect,
        TransportCircleAction::Sync => TransportActivityKind::Sync,
        TransportCircleAction::DiscoverPeers => TransportActivityKind::DiscoverPeers,
        TransportCircleAction::SyncSessions => TransportActivityKind::SyncSessions,
    }
}

fn activity_level_from_health(health: &TransportHealth) -> TransportActivityLevel {
    match health {
        TransportHealth::Online => TransportActivityLevel::Success,
        TransportHealth::Degraded => TransportActivityLevel::Info,
        TransportHealth::Offline => TransportActivityLevel::Warn,
    }
}

fn protocol_label(protocol: &crate::domain::transport::RelayProtocol) -> &'static str {
    match protocol {
        crate::domain::transport::RelayProtocol::Websocket => "websocket relay",
        crate::domain::transport::RelayProtocol::Mesh => "mesh relay",
        crate::domain::transport::RelayProtocol::Invite => "invite relay",
    }
}

fn engine_label(kind: &TransportEngineKind) -> &'static str {
    match kind {
        TransportEngineKind::Mock => "mock engine",
        TransportEngineKind::NativePreview => "native preview engine",
    }
}

fn apply_session_sync(
    seed: &mut ChatDomainSeed,
    circle_id: &str,
    use_tor_network: bool,
    experimental_transport: bool,
) {
    let mut primary_session_id = None;

    for session in &mut seed.sessions {
        if session.circle_id != circle_id || session.archived.unwrap_or(false) {
            continue;
        }

        if primary_session_id.is_none() {
            primary_session_id = Some(session.id.clone());
        }

        session.unread_count = None;
        session.time = "synced".into();
    }

    if let Some(session_id) = primary_session_id {
        let system_message = MessageItem {
            id: unique_system_message_id(&session_id),
            kind: MessageKind::System,
            author: MessageAuthor::System,
            body: if use_tor_network {
                "Session sync completed through the privacy relay path.".into()
            } else if experimental_transport {
                "Experimental transport sync completed for this circle.".into()
            } else {
                "Session sync completed across discovered relay peers.".into()
            },
            time: "now".into(),
            meta: None,
        };

        seed.message_store
            .entry(session_id.clone())
            .or_default()
            .push(system_message.clone());

        if let Some(session) = seed
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.subtitle = system_message.body;
        }
    }
}

fn peer_presence(contact: &ContactItem) -> PeerPresence {
    if contact.blocked.unwrap_or(false) {
        return PeerPresence::Offline;
    }

    if contact.online.unwrap_or(false) {
        return PeerPresence::Online;
    }

    PeerPresence::Idle
}

fn peer_last_seen(contact: &ContactItem) -> String {
    if contact.blocked.unwrap_or(false) {
        return "blocked".into();
    }

    if contact.online.unwrap_or(false) {
        return "now".into();
    }

    "recently".into()
}

fn session_sync_state(
    session: &SessionItem,
    diagnostic: &CircleTransportDiagnostic,
    pending_messages: u32,
) -> SessionSyncState {
    if pending_messages > 0 {
        return SessionSyncState::Pending;
    }

    if matches!(diagnostic.health, TransportHealth::Degraded) {
        return SessionSyncState::Syncing;
    }

    if matches!(session.kind, SessionKind::Group) && session.members.unwrap_or(0) >= 10 {
        return SessionSyncState::Conflict;
    }

    SessionSyncState::Idle
}

fn unique_system_message_id(session_id: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{session_id}-sync-{millis}")
}

fn unique_activity_id(circle_id: &str, action: &TransportCircleAction) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    let action_label = match action {
        TransportCircleAction::Connect => "connect",
        TransportCircleAction::Disconnect => "disconnect",
        TransportCircleAction::Sync => "sync",
        TransportCircleAction::DiscoverPeers => "discover",
        TransportCircleAction::SyncSessions => "session-sync",
    };

    format!("{circle_id}-{action_label}-{millis}")
}
