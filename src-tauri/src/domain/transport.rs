use crate::domain::chat::ChatDomainSeed;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportHealth {
    Online,
    Degraded,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayProtocol {
    Websocket,
    Mesh,
    Invite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerPresence {
    Online,
    Idle,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionSyncState {
    Idle,
    Syncing,
    Pending,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportActivityKind {
    Runtime,
    Connect,
    Disconnect,
    Sync,
    DiscoverPeers,
    SyncSessions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportActivityLevel {
    Info,
    Success,
    Warn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportEngineKind {
    Mock,
    NativePreview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportCapabilities {
    pub supports_mesh: bool,
    pub supports_paid_relays: bool,
    pub supports_tor: bool,
    pub experimental_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircleTransportDiagnostic {
    pub circle_id: String,
    pub relay: String,
    pub protocol: RelayProtocol,
    pub health: TransportHealth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    pub peer_count: u32,
    pub queued_messages: u32,
    pub last_sync: String,
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeer {
    pub circle_id: String,
    pub contact_id: String,
    pub name: String,
    pub handle: String,
    pub presence: PeerPresence,
    pub route: String,
    pub shared_sessions: u32,
    pub last_seen: String,
    pub blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncItem {
    pub circle_id: String,
    pub session_id: String,
    pub session_name: String,
    pub state: SessionSyncState,
    pub pending_messages: u32,
    pub source: String,
    pub last_merge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportActivityItem {
    pub id: String,
    pub circle_id: String,
    pub kind: TransportActivityKind,
    pub level: TransportActivityLevel,
    pub title: String,
    pub detail: String,
    pub time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportSnapshot {
    pub engine: TransportEngineKind,
    pub status: TransportHealth,
    pub active_circle_id: String,
    pub relay_count: u32,
    pub connected_relays: u32,
    pub queued_messages: u32,
    pub capabilities: TransportCapabilities,
    pub diagnostics: Vec<CircleTransportDiagnostic>,
    pub peers: Vec<DiscoveredPeer>,
    pub session_sync: Vec<SessionSyncItem>,
    pub activities: Vec<TransportActivityItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportSnapshotInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_circle_id: Option<String>,
    pub use_tor_network: bool,
    pub experimental_transport: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportCircleAction {
    Connect,
    Disconnect,
    Sync,
    DiscoverPeers,
    SyncSessions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportCircleActionInput {
    pub circle_id: String,
    pub action: TransportCircleAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_circle_id: Option<String>,
    pub use_tor_network: bool,
    pub experimental_transport: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMutationResult {
    pub seed: ChatDomainSeed,
    pub snapshot: TransportSnapshot,
}

pub trait TransportService {
    fn load_snapshot(&self, input: TransportSnapshotInput) -> Result<TransportSnapshot, String>;
    fn apply_circle_action(
        &self,
        input: TransportCircleActionInput,
    ) -> Result<TransportMutationResult, String>;
}
