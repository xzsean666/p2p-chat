use crate::domain::chat::{CircleItem, CircleStatus, CircleType};
use crate::domain::transport::{
    CircleTransportDiagnostic, DiscoveredPeer, PeerPresence, RelayProtocol, SessionSyncItem,
    SessionSyncState, TransportCircleAction, TransportHealth,
};
use crate::domain::transport_adapter::{TransportAdapter, TransportRuntimeOptions};

struct AdapterBehavior {
    protocol: RelayProtocol,
    route_label: &'static str,
    base_latency_ms: u32,
    open_peer_multiplier: u32,
    open_peer_floor: u32,
    connecting_peer_multiplier: u32,
    connecting_peer_floor: u32,
    connecting_queue_floor: u32,
    connecting_queue_cap: u32,
    open_last_sync: &'static str,
    connecting_last_sync: &'static str,
    connect_cache_label: &'static str,
    discover_cache_label: &'static str,
    sync_cache_label: &'static str,
    connect_opens_immediately: bool,
    discover_opens_immediately: bool,
}

pub fn protocol_for_relay(relay: &str) -> RelayProtocol {
    if relay.starts_with("mesh://") {
        RelayProtocol::Mesh
    } else if relay.starts_with("invite://") {
        RelayProtocol::Invite
    } else {
        RelayProtocol::Websocket
    }
}

pub fn adapter_for_relay(relay: &str) -> &'static dyn TransportAdapter {
    adapter_for_protocol(&protocol_for_relay(relay))
}

pub fn adapter_for_protocol(protocol: &RelayProtocol) -> &'static dyn TransportAdapter {
    match protocol {
        RelayProtocol::Mesh => &MESH_ADAPTER,
        RelayProtocol::Invite => &INVITE_ADAPTER,
        RelayProtocol::Websocket => &WEBSOCKET_ADAPTER,
    }
}

static WEBSOCKET_ADAPTER: WebsocketTransportAdapter = WebsocketTransportAdapter;
static MESH_ADAPTER: MeshTransportAdapter = MeshTransportAdapter;
static INVITE_ADAPTER: InviteTransportAdapter = InviteTransportAdapter;

pub struct WebsocketTransportAdapter;
pub struct MeshTransportAdapter;
pub struct InviteTransportAdapter;

impl TransportAdapter for WebsocketTransportAdapter {
    fn route_label(&self) -> &'static str {
        websocket_behavior().route_label
    }

    fn build_diagnostic(
        &self,
        circle: &CircleItem,
        session_count: u32,
    ) -> CircleTransportDiagnostic {
        build_diagnostic(circle, session_count, &websocket_behavior())
    }

    fn apply_circle_action(
        &self,
        circle: &mut CircleItem,
        action: &TransportCircleAction,
        runtime: TransportRuntimeOptions,
    ) {
        apply_circle_action(circle, action, runtime, &websocket_behavior());
    }

    fn apply_cache_action(
        &self,
        peers: &mut [DiscoveredPeer],
        session_sync: &mut [SessionSyncItem],
        action: &TransportCircleAction,
        circle_id: &str,
    ) {
        apply_cache_action(
            peers,
            session_sync,
            action,
            circle_id,
            &websocket_behavior(),
        );
    }
}

impl TransportAdapter for MeshTransportAdapter {
    fn route_label(&self) -> &'static str {
        mesh_behavior().route_label
    }

    fn build_diagnostic(
        &self,
        circle: &CircleItem,
        session_count: u32,
    ) -> CircleTransportDiagnostic {
        build_diagnostic(circle, session_count, &mesh_behavior())
    }

    fn apply_circle_action(
        &self,
        circle: &mut CircleItem,
        action: &TransportCircleAction,
        runtime: TransportRuntimeOptions,
    ) {
        apply_circle_action(circle, action, runtime, &mesh_behavior());
    }

    fn apply_cache_action(
        &self,
        peers: &mut [DiscoveredPeer],
        session_sync: &mut [SessionSyncItem],
        action: &TransportCircleAction,
        circle_id: &str,
    ) {
        apply_cache_action(peers, session_sync, action, circle_id, &mesh_behavior());
    }
}

impl TransportAdapter for InviteTransportAdapter {
    fn route_label(&self) -> &'static str {
        invite_behavior().route_label
    }

    fn build_diagnostic(
        &self,
        circle: &CircleItem,
        session_count: u32,
    ) -> CircleTransportDiagnostic {
        build_diagnostic(circle, session_count, &invite_behavior())
    }

    fn apply_circle_action(
        &self,
        circle: &mut CircleItem,
        action: &TransportCircleAction,
        runtime: TransportRuntimeOptions,
    ) {
        apply_circle_action(circle, action, runtime, &invite_behavior());
    }

    fn apply_cache_action(
        &self,
        peers: &mut [DiscoveredPeer],
        session_sync: &mut [SessionSyncItem],
        action: &TransportCircleAction,
        circle_id: &str,
    ) {
        apply_cache_action(peers, session_sync, action, circle_id, &invite_behavior());
    }
}

fn websocket_behavior() -> AdapterBehavior {
    AdapterBehavior {
        protocol: RelayProtocol::Websocket,
        route_label: "direct relay",
        base_latency_ms: 42,
        open_peer_multiplier: 2,
        open_peer_floor: 3,
        connecting_peer_multiplier: 1,
        connecting_peer_floor: 1,
        connecting_queue_floor: 1,
        connecting_queue_cap: 3,
        open_last_sync: "relay heartbeat",
        connecting_last_sync: "pending websocket handshake",
        connect_cache_label: "pending relay handshake",
        discover_cache_label: "relay peers refreshed",
        sync_cache_label: "relay sync complete",
        connect_opens_immediately: false,
        discover_opens_immediately: false,
    }
}

fn mesh_behavior() -> AdapterBehavior {
    AdapterBehavior {
        protocol: RelayProtocol::Mesh,
        route_label: "mesh hop",
        base_latency_ms: 18,
        open_peer_multiplier: 3,
        open_peer_floor: 4,
        connecting_peer_multiplier: 2,
        connecting_peer_floor: 2,
        connecting_queue_floor: 1,
        connecting_queue_cap: 2,
        open_last_sync: "mesh gossip active",
        connecting_last_sync: "mesh route discovery",
        connect_cache_label: "mesh route warmup",
        discover_cache_label: "mesh neighbors discovered",
        sync_cache_label: "mesh gossip synced",
        connect_opens_immediately: true,
        discover_opens_immediately: true,
    }
}

fn invite_behavior() -> AdapterBehavior {
    AdapterBehavior {
        protocol: RelayProtocol::Invite,
        route_label: "invite handoff",
        base_latency_ms: 72,
        open_peer_multiplier: 1,
        open_peer_floor: 2,
        connecting_peer_multiplier: 1,
        connecting_peer_floor: 1,
        connecting_queue_floor: 2,
        connecting_queue_cap: 4,
        open_last_sync: "invite relay ready",
        connecting_last_sync: "invite verification",
        connect_cache_label: "invite tunnel pending",
        discover_cache_label: "invite contacts resolved",
        sync_cache_label: "invite relay synced",
        connect_opens_immediately: false,
        discover_opens_immediately: false,
    }
}

fn build_diagnostic(
    circle: &CircleItem,
    session_count: u32,
    behavior: &AdapterBehavior,
) -> CircleTransportDiagnostic {
    CircleTransportDiagnostic {
        circle_id: circle.id.clone(),
        relay: circle.relay.clone(),
        protocol: behavior.protocol.clone(),
        health: transport_health(&circle.status),
        latency_ms: parse_latency_ms(&circle.latency),
        peer_count: match circle.status {
            CircleStatus::Open => (session_count.saturating_mul(behavior.open_peer_multiplier))
                .max(behavior.open_peer_floor),
            CircleStatus::Connecting => (session_count
                .saturating_mul(behavior.connecting_peer_multiplier))
            .max(behavior.connecting_peer_floor),
            CircleStatus::Closed => 0,
        },
        queued_messages: match circle.status {
            CircleStatus::Open => 0,
            CircleStatus::Connecting => session_count
                .min(behavior.connecting_queue_cap)
                .max(behavior.connecting_queue_floor),
            CircleStatus::Closed => 0,
        },
        last_sync: match circle.status {
            CircleStatus::Open => behavior.open_last_sync.into(),
            CircleStatus::Connecting => behavior.connecting_last_sync.into(),
            CircleStatus::Closed => "offline".into(),
        },
        reachable: !matches!(circle.status, CircleStatus::Closed),
    }
}

fn apply_circle_action(
    circle: &mut CircleItem,
    action: &TransportCircleAction,
    runtime: TransportRuntimeOptions,
    behavior: &AdapterBehavior,
) {
    match action {
        TransportCircleAction::Connect => {
            if behavior.connect_opens_immediately {
                set_circle_open(circle, runtime, behavior.base_latency_ms);
            } else {
                circle.status = CircleStatus::Connecting;
                circle.latency = "--".into();
            }
        }
        TransportCircleAction::Disconnect => {
            circle.status = CircleStatus::Closed;
            circle.latency = "--".into();
        }
        TransportCircleAction::Sync | TransportCircleAction::SyncSessions => {
            set_circle_open(circle, runtime, behavior.base_latency_ms);
        }
        TransportCircleAction::DiscoverPeers => {
            if behavior.discover_opens_immediately {
                set_circle_open(circle, runtime, behavior.base_latency_ms);
            } else if matches!(circle.status, CircleStatus::Closed) {
                circle.status = CircleStatus::Connecting;
                circle.latency = "--".into();
            }
        }
    }
}

fn apply_cache_action(
    peers: &mut [DiscoveredPeer],
    session_sync: &mut [SessionSyncItem],
    action: &TransportCircleAction,
    circle_id: &str,
    behavior: &AdapterBehavior,
) {
    match action {
        TransportCircleAction::Connect => {
            for peer in peers.iter_mut() {
                if peer.circle_id == circle_id && !peer.blocked {
                    if matches!(peer.presence, PeerPresence::Offline) {
                        peer.presence = PeerPresence::Idle;
                    }
                    peer.last_seen = behavior.connect_cache_label.into();
                }
            }

            for item in session_sync.iter_mut() {
                if item.circle_id == circle_id {
                    if matches!(item.state, SessionSyncState::Idle) {
                        item.state = SessionSyncState::Syncing;
                    }

                    if item.last_merge != "pending merge" {
                        item.last_merge = behavior.connect_cache_label.into();
                    }
                }
            }
        }
        TransportCircleAction::Disconnect => {
            for peer in peers.iter_mut() {
                if peer.circle_id == circle_id && !peer.blocked {
                    peer.presence = PeerPresence::Offline;
                    peer.last_seen = "relay offline".into();
                }
            }

            for item in session_sync.iter_mut() {
                if item.circle_id == circle_id {
                    if matches!(item.state, SessionSyncState::Syncing) {
                        item.state = SessionSyncState::Idle;
                    }
                    item.last_merge = "offline".into();
                }
            }
        }
        TransportCircleAction::Sync => {
            for peer in peers.iter_mut() {
                if peer.circle_id == circle_id && !peer.blocked {
                    if matches!(peer.presence, PeerPresence::Offline) {
                        peer.presence = PeerPresence::Idle;
                    }
                    peer.last_seen = behavior.sync_cache_label.into();
                }
            }

            for item in session_sync.iter_mut() {
                if item.circle_id == circle_id {
                    if matches!(item.state, SessionSyncState::Syncing) {
                        item.state = SessionSyncState::Idle;
                    }

                    if item.last_merge != "pending merge"
                        && !matches!(item.state, SessionSyncState::Conflict)
                    {
                        item.last_merge = behavior.sync_cache_label.into();
                    }
                }
            }
        }
        TransportCircleAction::DiscoverPeers => {
            for peer in peers.iter_mut() {
                if peer.circle_id == circle_id && !peer.blocked {
                    if matches!(peer.presence, PeerPresence::Offline) {
                        peer.presence = PeerPresence::Idle;
                    }
                    peer.last_seen = behavior.discover_cache_label.into();
                }
            }
        }
        TransportCircleAction::SyncSessions => {
            for item in session_sync.iter_mut() {
                if item.circle_id == circle_id {
                    if !matches!(item.state, SessionSyncState::Conflict) {
                        item.state = SessionSyncState::Idle;
                    }
                    item.last_merge = behavior.sync_cache_label.into();
                }
            }
        }
    }
}

fn set_circle_open(
    circle: &mut CircleItem,
    runtime: TransportRuntimeOptions,
    base_latency_ms: u32,
) {
    circle.status = CircleStatus::Open;
    circle.latency = format!(
        "{} ms",
        base_latency_ms + circle_penalty_ms(&circle.circle_type) + runtime_adjustment_ms(runtime)
    );
}

fn circle_penalty_ms(circle_type: &CircleType) -> u32 {
    match circle_type {
        CircleType::Paid => 14,
        CircleType::Bitchat => 6,
        CircleType::Custom => 8,
        CircleType::Default => 0,
    }
}

fn runtime_adjustment_ms(runtime: TransportRuntimeOptions) -> u32 {
    let tor_penalty = if runtime.use_tor_network { 32 } else { 0 };
    let experimental_adjustment = if runtime.experimental_transport { 6 } else { 0 };

    tor_penalty + experimental_adjustment
}

fn transport_health(status: &CircleStatus) -> TransportHealth {
    match status {
        CircleStatus::Open => TransportHealth::Online,
        CircleStatus::Connecting => TransportHealth::Degraded,
        CircleStatus::Closed => TransportHealth::Offline,
    }
}

fn parse_latency_ms(value: &str) -> Option<u32> {
    let digits = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();

    if digits.is_empty() {
        None
    } else {
        digits.parse::<u32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_circle(relay: &str) -> CircleItem {
        CircleItem {
            id: "circle-1".into(),
            name: "Circle One".into(),
            relay: relay.into(),
            circle_type: CircleType::Default,
            status: CircleStatus::Closed,
            latency: "--".into(),
            description: "test circle".into(),
        }
    }

    #[test]
    fn selects_expected_adapter_route_label() {
        assert_eq!(
            adapter_for_relay("wss://relay.example.com").route_label(),
            "direct relay"
        );
        assert_eq!(
            adapter_for_relay("mesh://neighborhood").route_label(),
            "mesh hop"
        );
        assert_eq!(
            adapter_for_relay("invite://circle-code").route_label(),
            "invite handoff"
        );
    }

    #[test]
    fn mesh_connect_opens_immediately() {
        let adapter = adapter_for_relay("mesh://friends");
        let mut circle = sample_circle("mesh://friends");
        adapter.apply_circle_action(
            &mut circle,
            &TransportCircleAction::Connect,
            TransportRuntimeOptions {
                use_tor_network: false,
                experimental_transport: false,
            },
        );

        assert!(matches!(circle.status, CircleStatus::Open));
        assert_eq!(circle.latency, "18 ms");
    }

    #[test]
    fn websocket_diagnostic_caps_connecting_queue() {
        let adapter = adapter_for_relay("wss://relay.example.com");
        let mut circle = sample_circle("wss://relay.example.com");
        circle.status = CircleStatus::Connecting;
        let diagnostic = adapter.build_diagnostic(&circle, 10);

        assert!(matches!(diagnostic.protocol, RelayProtocol::Websocket));
        assert_eq!(diagnostic.peer_count, 10);
        assert_eq!(diagnostic.queued_messages, 3);
        assert_eq!(diagnostic.last_sync, "pending websocket handshake");
    }

    #[test]
    fn invite_cache_action_marks_pending_state() {
        let adapter = adapter_for_relay("invite://relay");
        let mut peers = vec![DiscoveredPeer {
            circle_id: "circle-1".into(),
            contact_id: "contact-1".into(),
            name: "Casey".into(),
            handle: "@casey".into(),
            presence: PeerPresence::Offline,
            route: "invite handoff".into(),
            shared_sessions: 1,
            last_seen: "offline".into(),
            blocked: false,
        }];
        let mut session_sync = vec![SessionSyncItem {
            circle_id: "circle-1".into(),
            session_id: "session-1".into(),
            session_name: "Casey".into(),
            state: SessionSyncState::Idle,
            pending_messages: 0,
            source: "invite handoff".into(),
            last_merge: "stale".into(),
        }];

        adapter.apply_cache_action(
            &mut peers,
            &mut session_sync,
            &TransportCircleAction::Connect,
            "circle-1",
        );

        assert!(matches!(peers[0].presence, PeerPresence::Idle));
        assert_eq!(peers[0].last_seen, "invite tunnel pending");
        assert!(matches!(session_sync[0].state, SessionSyncState::Syncing));
        assert_eq!(session_sync[0].last_merge, "invite tunnel pending");
    }
}
