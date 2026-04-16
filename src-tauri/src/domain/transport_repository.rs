use crate::domain::transport::{DiscoveredPeer, SessionSyncItem, TransportActivityItem};

#[derive(Debug, Clone, Default)]
pub struct TransportCache {
    pub peers: Vec<DiscoveredPeer>,
    pub session_sync: Vec<SessionSyncItem>,
    pub activities: Vec<TransportActivityItem>,
}

pub trait TransportRepository {
    fn load_transport_cache(&self) -> Result<TransportCache, String>;
    fn save_transport_cache(&self, cache: TransportCache) -> Result<(), String>;
}
