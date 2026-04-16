use crate::domain::chat::CircleItem;
use crate::domain::transport::{
    CircleTransportDiagnostic, DiscoveredPeer, SessionSyncItem, TransportCircleAction,
};

#[derive(Debug, Clone, Copy)]
pub struct TransportRuntimeOptions {
    pub use_tor_network: bool,
    pub experimental_transport: bool,
}

pub trait TransportAdapter {
    fn route_label(&self) -> &'static str;
    fn build_diagnostic(
        &self,
        circle: &CircleItem,
        session_count: u32,
    ) -> CircleTransportDiagnostic;
    fn apply_circle_action(
        &self,
        circle: &mut CircleItem,
        action: &TransportCircleAction,
        runtime: TransportRuntimeOptions,
    );
    fn apply_cache_action(
        &self,
        peers: &mut [DiscoveredPeer],
        session_sync: &mut [SessionSyncItem],
        action: &TransportCircleAction,
        circle_id: &str,
    );
}
