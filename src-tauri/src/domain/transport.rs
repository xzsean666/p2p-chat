use crate::domain::chat::{
    ChatDomainSeed, MergeRemoteDeliveryReceiptsInput, MergeRemoteMessagesInput, MessageKind,
    SignedNostrEvent,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeState {
    Inactive,
    Starting,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeDesiredState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeRecoveryPolicy {
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeQueueState {
    Idle,
    Queued,
    Backoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportRuntimeAdapterKind {
    Embedded,
    LocalCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeLaunchStatus {
    Embedded,
    Ready,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportRuntimeLaunchResult {
    Spawned,
    Reused,
    Failed,
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
pub struct TransportRuntimeSession {
    pub circle_id: String,
    pub driver: String,
    pub adapter_kind: TransportRuntimeAdapterKind,
    pub launch_status: TransportRuntimeLaunchStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub launch_arguments: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_launch_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_launch_result: Option<TransportRuntimeLaunchResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_launch_pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_launch_at: Option<String>,
    pub desired_state: TransportRuntimeDesiredState,
    pub recovery_policy: TransportRuntimeRecoveryPolicy,
    pub queue_state: TransportRuntimeQueueState,
    pub restart_attempts: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_in: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_failure_at: Option<String>,
    pub state: TransportRuntimeState,
    pub generation: u32,
    pub state_since: String,
    pub session_label: String,
    pub endpoint: String,
    pub last_event: String,
    pub last_event_at: String,
}

#[derive(Debug, Clone)]
pub struct TransportRuntimeRegistryEntry {
    pub circle_id: String,
    pub driver: String,
    pub adapter_kind: TransportRuntimeAdapterKind,
    pub launch_status: TransportRuntimeLaunchStatus,
    pub launch_command: Option<String>,
    pub launch_arguments: Vec<String>,
    pub resolved_launch_command: Option<String>,
    pub launch_error: Option<String>,
    pub last_launch_result: Option<TransportRuntimeLaunchResult>,
    pub last_launch_pid: Option<u32>,
    pub last_launch_at: Option<String>,
    pub desired_state: TransportRuntimeDesiredState,
    pub recovery_policy: TransportRuntimeRecoveryPolicy,
    pub queue_state: TransportRuntimeQueueState,
    pub restart_attempts: u32,
    pub next_retry_in: Option<String>,
    pub next_retry_at_ms: Option<u64>,
    pub last_failure_reason: Option<String>,
    pub last_failure_at: Option<String>,
    pub state: TransportRuntimeState,
    pub generation: u32,
    pub state_since: String,
    pub session_label: String,
    pub endpoint: String,
    pub last_event: String,
    pub last_event_at: String,
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
    pub runtime_sessions: Vec<TransportRuntimeSession>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_since_created_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportMutationResult {
    pub seed: ChatDomainSeed,
    pub snapshot: TransportSnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct TransportChatEffects {
    pub remote_message_merges: Vec<MergeRemoteMessagesInput>,
    pub remote_delivery_receipt_merges: Vec<MergeRemoteDeliveryReceiptsInput>,
    pub clear_unread_session_ids: Vec<String>,
}

impl TransportChatEffects {
    pub fn is_empty(&self) -> bool {
        self.remote_message_merges.is_empty()
            && self.remote_delivery_receipt_merges.is_empty()
            && self.clear_unread_session_ids.is_empty()
    }

    pub fn append(&mut self, other: Self) {
        self.remote_message_merges
            .extend(other.remote_message_merges);
        self.remote_delivery_receipt_merges
            .extend(other.remote_delivery_receipt_merges);
        self.clear_unread_session_ids
            .extend(other.clear_unread_session_ids);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CirclePeerPresenceUpdate {
    pub circle_id: String,
    pub presence: PeerPresence,
    pub last_seen: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircleSessionSyncUpdate {
    pub circle_id: String,
    pub state: SessionSyncState,
    pub last_merge: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportRelaySyncFilter {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tagged_pubkeys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportRuntimeOutboundMessage {
    pub session_id: String,
    pub message_id: String,
    pub remote_id: String,
    pub signed_nostr_event: SignedNostrEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportRuntimeOutboundMedia {
    pub session_id: String,
    pub message_id: String,
    pub remote_id: String,
    pub kind: MessageKind,
    pub name: String,
    pub label: String,
    pub local_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub remote_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportOutboundDispatch {
    pub circle_id: String,
    pub session_id: String,
    pub message_id: String,
    pub remote_id: String,
    pub event_id: String,
    pub runtime_generation: u32,
    pub request_id: String,
    pub dispatched_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportOutboundMediaDispatch {
    pub circle_id: String,
    pub session_id: String,
    pub message_id: String,
    pub remote_id: String,
    pub local_path: String,
    pub runtime_generation: u32,
    pub request_id: String,
    pub dispatched_at: String,
}

#[derive(Debug, Clone, Default)]
pub struct TransportRuntimeCacheEffects {
    pub peer_presence_updates: Vec<CirclePeerPresenceUpdate>,
    pub session_sync_updates: Vec<CircleSessionSyncUpdate>,
    pub activities_append: Vec<TransportActivityItem>,
}

impl TransportRuntimeCacheEffects {
    pub fn is_empty(&self) -> bool {
        self.peer_presence_updates.is_empty()
            && self.session_sync_updates.is_empty()
            && self.activities_append.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransportRuntimeEffects {
    pub chat_effects: TransportChatEffects,
    pub cache_effects: TransportRuntimeCacheEffects,
}

impl TransportRuntimeEffects {
    pub fn is_empty(&self) -> bool {
        self.chat_effects.is_empty() && self.cache_effects.is_empty()
    }

    pub fn push_runtime_output_event(&mut self, event: TransportRuntimeOutputEvent) {
        match event {
            TransportRuntimeOutputEvent::MergeRemoteMessages(payload) => {
                self.chat_effects.remote_message_merges.push(payload);
            }
            TransportRuntimeOutputEvent::MergeRemoteDeliveryReceipts(payload) => {
                self.chat_effects
                    .remote_delivery_receipt_merges
                    .push(payload);
            }
            TransportRuntimeOutputEvent::ClearUnread { session_id } => {
                self.chat_effects.clear_unread_session_ids.push(session_id);
            }
            TransportRuntimeOutputEvent::SetCirclePeerPresence(payload) => {
                self.cache_effects.peer_presence_updates.push(payload);
            }
            TransportRuntimeOutputEvent::SetCircleSessionSyncState(payload) => {
                self.cache_effects.session_sync_updates.push(payload);
            }
            TransportRuntimeOutputEvent::AppendActivity { activity } => {
                self.cache_effects.activities_append.push(activity);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "camelCase")]
pub enum TransportRuntimeOutputEvent {
    MergeRemoteMessages(MergeRemoteMessagesInput),
    MergeRemoteDeliveryReceipts(MergeRemoteDeliveryReceiptsInput),
    ClearUnread {
        #[serde(rename = "sessionId")]
        session_id: String,
    },
    SetCirclePeerPresence(CirclePeerPresenceUpdate),
    SetCircleSessionSyncState(CircleSessionSyncUpdate),
    AppendActivity {
        activity: TransportActivityItem,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "camelCase")]
pub enum TransportRuntimeInputEvent {
    ApplyCircleAction(TransportRuntimeActionRequest),
    PublishOutboundMessages(TransportRuntimePublishRequest),
}

#[derive(Debug, Clone)]
pub struct TransportRuntimeBackgroundSyncRequest {
    pub circle_id: String,
    pub sync_since_created_at: Option<u64>,
    pub relay_sync_filters: Vec<TransportRelaySyncFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportRuntimeActionRequest {
    pub request_id: String,
    pub circle_id: String,
    pub action: TransportCircleAction,
    #[serde(default, skip_serializing_if = "is_false")]
    pub background: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub session_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread_session_ids: Vec<String>,
    pub peer_count: u32,
    pub session_sync_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_since_created_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relay_sync_filters: Vec<TransportRelaySyncFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound_messages: Vec<TransportRuntimeOutboundMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound_media_messages: Vec<TransportRuntimeOutboundMedia>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportRuntimePublishRequest {
    pub request_id: String,
    pub circle_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound_messages: Vec<TransportRuntimeOutboundMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outbound_media_messages: Vec<TransportRuntimeOutboundMedia>,
}

pub trait TransportService {
    fn load_snapshot(&self, input: TransportSnapshotInput) -> Result<TransportSnapshot, String>;
    fn apply_circle_action(
        &self,
        input: TransportCircleActionInput,
    ) -> Result<TransportMutationResult, String>;
}
