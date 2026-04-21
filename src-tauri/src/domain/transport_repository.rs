use crate::domain::transport::{
    DiscoveredPeer, SessionSyncItem, TransportActivityItem, TransportOutboundDispatch,
    TransportOutboundMediaDispatch, TransportRuntimeRegistryEntry, TransportRuntimeSession,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRelaySyncCursor {
    pub circle_id: String,
    pub last_created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRelayBackgroundSyncMarker {
    pub circle_id: String,
    pub last_requested_at_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct TransportCache {
    pub peers: Vec<DiscoveredPeer>,
    pub session_sync: Vec<SessionSyncItem>,
    pub activities: Vec<TransportActivityItem>,
    pub outbound_dispatches: Vec<TransportOutboundDispatch>,
    pub outbound_media_dispatches: Vec<TransportOutboundMediaDispatch>,
    pub relay_sync_cursors: Vec<TransportRelaySyncCursor>,
    pub relay_background_sync_markers: Vec<TransportRelayBackgroundSyncMarker>,
    pub runtime_registry: Vec<TransportRuntimeRegistryEntry>,
    pub runtime_sessions: Vec<TransportRuntimeSession>,
}

pub trait TransportRepository {
    fn load_transport_cache(&self) -> Result<TransportCache, String>;
    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String>;
}
