#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionKind {
    Direct,
    Group,
    #[serde(rename = "self")]
    SelfChat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Text,
    Image,
    Video,
    File,
    Audio,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMediaKind {
    File,
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageAuthor {
    Me,
    Peer,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageDeliveryStatus {
    Sending,
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageSyncSource {
    Local,
    Relay,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircleType {
    Default,
    Paid,
    Bitchat,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircleStatus {
    Open,
    Connecting,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatSessionAction {
    Pin,
    Mute,
    Archive,
    Delete,
    Unarchive,
    #[serde(rename = "clearUnread")]
    ClearUnread,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircleCreateMode {
    Invite,
    Private,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginMethod {
    QuickStart,
    ExistingAccount,
    Signer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginCircleSelectionMode {
    Existing,
    Invite,
    Custom,
    Restore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginAccessKind {
    LocalProfile,
    Nsec,
    Npub,
    HexKey,
    Bunker,
    NostrConnect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupRole {
    Admin,
    Member,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    System,
    Light,
    Ink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LanguagePreference {
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSizePreference {
    Compact,
    Default,
    Large,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub name: String,
    pub handle: String,
    pub initials: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginAccessSummary {
    pub kind: LoginAccessKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCircleSelectionInput {
    pub mode: LoginCircleSelectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relays: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginAccessInput {
    pub kind: LoginAccessKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCompletionInput {
    pub method: LoginMethod,
    pub access: LoginAccessInput,
    pub user_profile: UserProfile,
    pub circle_selection: LoginCircleSelectionInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logged_in_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionSummary {
    pub login_method: LoginMethod,
    pub access: LoginAccessSummary,
    pub circle_selection_mode: LoginCircleSelectionMode,
    pub logged_in_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthRuntimeState {
    LocalProfile,
    Pending,
    Connected,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRuntimeSummary {
    pub state: AuthRuntimeState,
    pub login_method: LoginMethod,
    pub access_kind: LoginAccessKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub can_send_messages: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_blocked_reason: Option<String>,
    #[serde(default)]
    pub persisted_in_native_store: bool,
    #[serde(default)]
    pub credential_persisted_in_native_store: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRuntimeBindingSummary {
    pub access_kind: LoginAccessKind,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_pubkey: Option<String>,
    #[serde(default)]
    pub relay_count: u32,
    #[serde(default)]
    pub has_secret: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_permissions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    pub persisted_in_native_store: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRuntimeClientUriSummary {
    pub uri: String,
    pub public_key: String,
    #[serde(default)]
    pub relay_count: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relays: Vec<String>,
    pub client_name: String,
    pub stored_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalAccountSecretSummary {
    pub login_method: LoginMethod,
    pub access_kind: LoginAccessKind,
    pub pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ethereum_address: Option<String>,
    pub nsec: String,
    pub hex_key: String,
    pub stored_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAuthRuntimeInput {
    pub state: AuthRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircleItem {
    pub id: String,
    pub name: String,
    pub relay: String,
    #[serde(rename = "type")]
    pub circle_type: CircleType,
    pub status: CircleStatus,
    pub latency: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreCircleInput {
    pub name: String,
    pub relay: String,
    #[serde(rename = "type")]
    pub circle_type: CircleType,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateContactRemarkInput {
    pub contact_id: String,
    pub remark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorableCircleEntry {
    pub name: String,
    pub relay: String,
    #[serde(rename = "type")]
    pub circle_type: CircleType,
    pub description: String,
    pub archived_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactItem {
    pub id: String,
    pub name: String,
    pub initials: String,
    pub handle: String,
    pub pubkey: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ethereum_address: Option<String>,
    pub subtitle: String,
    pub bio: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionItem {
    pub id: String,
    pub circle_id: String,
    pub name: String,
    pub initials: String,
    pub subtitle: String,
    pub time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unread_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    pub kind: SessionKind,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedNostrEvent {
    pub event_id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageReplyPreview {
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    pub author: MessageAuthor,
    pub author_label: String,
    pub kind: MessageKind,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageItem {
    pub id: String,
    pub kind: MessageKind,
    pub author: MessageAuthor,
    pub body: String,
    pub time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_status: Option<MessageDeliveryStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_source: Option<MessageSyncSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_nostr_event: Option<SignedNostrEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageReplyPreview>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    pub contact_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<GroupRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupProfile {
    pub session_id: String,
    pub name: String,
    pub description: String,
    pub members: Vec<GroupMember>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChatDomainSeed {
    pub circles: Vec<CircleItem>,
    pub contacts: Vec<ContactItem>,
    pub sessions: Vec<SessionItem>,
    pub groups: Vec<GroupProfile>,
    pub message_store: HashMap<String, Vec<MessageItem>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatDomainOverview {
    pub circles: Vec<CircleItem>,
    pub contacts: Vec<ContactItem>,
    pub sessions: Vec<SessionItem>,
    pub groups: Vec<GroupProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    pub session_id: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendFileMessageInput {
    pub session_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendImageMessageInput {
    pub session_id: String,
    pub name: String,
    pub meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendVideoMessageInput {
    pub session_id: String,
    pub name: String,
    pub meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreChatMediaAssetInput {
    pub kind: ChatMediaKind,
    pub name: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredChatMediaAsset {
    pub local_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupChatMediaAssetsResult {
    pub removed_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheChatMessageMediaInput {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedChatMessageMediaResult {
    pub seed: ChatDomainSeed,
    pub local_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChatMessageMediaRemoteUrlInput {
    pub session_id: String,
    pub message_id: String,
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedChatMessageMediaRemoteUrlResult {
    pub seed: ChatDomainSeed,
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionDraftInput {
    pub session_id: String,
    pub draft: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMessageDeliveryStatusInput {
    pub session_id: String,
    pub message_id: String,
    pub delivery_status: MessageDeliveryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryMessageDeliveryInput {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRemoteMessagesInput {
    pub session_id: String,
    pub messages: Vec<MessageItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDeliveryReceipt {
    pub remote_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub delivery_status: MessageDeliveryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRemoteDeliveryReceiptsInput {
    pub session_id: String,
    pub receipts: Vec<RemoteDeliveryReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConversationInput {
    pub circle_id: String,
    pub contact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSelfConversationInput {
    pub circle_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConversationResult {
    pub seed: ChatDomainSeed,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupConversationInput {
    pub circle_id: String,
    pub name: String,
    pub member_contact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartLookupConversationInput {
    pub circle_id: String,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionActionInput {
    pub session_id: String,
    pub action: ChatSessionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCircleInput {
    pub mode: CircleCreateMode,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invite_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddCircleResult {
    pub seed: ChatDomainSeed,
    pub circle_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCircleInput {
    pub circle_id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupNameInput {
    pub session_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupMembersInput {
    pub session_id: String,
    pub member_contact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    pub theme: ThemePreference,
    pub language: LanguagePreference,
    pub text_size: TextSizePreference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPreferences {
    pub allow_send: bool,
    pub allow_receive: bool,
    pub show_badge: bool,
    pub archive_summary: bool,
    pub mentions_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedPreferences {
    pub show_message_info: bool,
    pub use_tor_network: bool,
    pub relay_diagnostics: bool,
    pub experimental_transport: bool,
    #[serde(default = "default_media_upload_driver")]
    pub media_upload_driver: String,
    #[serde(default)]
    pub media_upload_endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellStateSnapshot {
    pub is_authenticated: bool,
    #[serde(default)]
    pub auth_session: Option<AuthSessionSummary>,
    #[serde(default)]
    pub auth_runtime: Option<AuthRuntimeSummary>,
    #[serde(default)]
    pub auth_runtime_binding: Option<AuthRuntimeBindingSummary>,
    #[serde(default = "default_user_profile")]
    pub user_profile: UserProfile,
    #[serde(default)]
    pub restorable_circles: Vec<RestorableCircleEntry>,
    pub app_preferences: AppPreferences,
    pub notification_preferences: NotificationPreferences,
    pub advanced_preferences: AdvancedPreferences,
    pub active_circle_id: String,
    pub selected_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatShellSnapshot {
    pub domain: ChatDomainSeed,
    pub shell: ShellStateSnapshot,
}

pub fn default_message_page_size() -> u32 {
    30
}

pub fn default_message_update_limit() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionMessagesInput {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_message_id: Option<String>,
    #[serde(default = "default_message_page_size")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionMessagesPage {
    pub session_id: String,
    pub messages: Vec<MessageItem>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadSessionMessageUpdatesInput {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_message_id: Option<String>,
    #[serde(default = "default_message_update_limit")]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionMessageUpdates {
    pub session_id: String,
    pub messages: Vec<MessageItem>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_after_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedShellState {
    pub is_authenticated: bool,
    #[serde(default)]
    pub auth_session: Option<AuthSessionSummary>,
    #[serde(default)]
    pub auth_runtime: Option<AuthRuntimeSummary>,
    #[serde(default)]
    pub auth_runtime_binding: Option<AuthRuntimeBindingSummary>,
    #[serde(default = "default_user_profile")]
    pub user_profile: UserProfile,
    #[serde(default)]
    pub restorable_circles: Vec<RestorableCircleEntry>,
    pub circles: Vec<CircleItem>,
    pub app_preferences: AppPreferences,
    pub notification_preferences: NotificationPreferences,
    pub advanced_preferences: AdvancedPreferences,
    pub active_circle_id: String,
    pub selected_session_id: String,
    pub sessions: Vec<SessionItem>,
    pub contacts: Vec<ContactItem>,
    pub groups: Vec<GroupProfile>,
    pub message_store: HashMap<String, Vec<MessageItem>>,
}

pub fn default_app_preferences() -> AppPreferences {
    AppPreferences {
        theme: ThemePreference::System,
        language: LanguagePreference::En,
        text_size: TextSizePreference::Default,
    }
}

pub fn default_notification_preferences() -> NotificationPreferences {
    NotificationPreferences {
        allow_send: true,
        allow_receive: false,
        show_badge: true,
        archive_summary: true,
        mentions_only: false,
    }
}

pub fn default_advanced_preferences() -> AdvancedPreferences {
    AdvancedPreferences {
        show_message_info: false,
        use_tor_network: false,
        relay_diagnostics: true,
        experimental_transport: true,
        media_upload_driver: default_media_upload_driver(),
        media_upload_endpoint: String::new(),
    }
}

pub fn default_media_upload_driver() -> String {
    "auto".into()
}

pub fn default_user_profile() -> UserProfile {
    UserProfile {
        name: "Sean Chen".into(),
        handle: "@seanchen".into(),
        initials: "SC".into(),
        status: "Circle owner".into(),
    }
}

impl From<ChatDomainSeed> for PersistedShellState {
    fn from(seed: ChatDomainSeed) -> Self {
        let active_circle_id = seed
            .circles
            .first()
            .map(|circle| circle.id.clone())
            .unwrap_or_default();
        let selected_session_id = seed
            .sessions
            .first()
            .map(|session| session.id.clone())
            .unwrap_or_default();

        Self {
            is_authenticated: false,
            auth_session: None,
            auth_runtime: None,
            auth_runtime_binding: None,
            user_profile: default_user_profile(),
            restorable_circles: vec![],
            circles: seed.circles,
            app_preferences: default_app_preferences(),
            notification_preferences: default_notification_preferences(),
            advanced_preferences: default_advanced_preferences(),
            active_circle_id,
            selected_session_id,
            sessions: seed.sessions,
            contacts: seed.contacts,
            groups: seed.groups,
            message_store: seed.message_store,
        }
    }
}

impl From<PersistedShellState> for ShellStateSnapshot {
    fn from(state: PersistedShellState) -> Self {
        Self {
            is_authenticated: state.is_authenticated,
            auth_session: state.auth_session,
            auth_runtime: state.auth_runtime,
            auth_runtime_binding: state.auth_runtime_binding,
            user_profile: state.user_profile,
            restorable_circles: state.restorable_circles,
            app_preferences: state.app_preferences,
            notification_preferences: state.notification_preferences,
            advanced_preferences: state.advanced_preferences,
            active_circle_id: state.active_circle_id,
            selected_session_id: state.selected_session_id,
        }
    }
}

impl From<ChatDomainSeed> for ShellStateSnapshot {
    fn from(seed: ChatDomainSeed) -> Self {
        PersistedShellState::from(seed).into()
    }
}

impl From<PersistedShellState> for ChatShellSnapshot {
    fn from(state: PersistedShellState) -> Self {
        Self {
            domain: state.clone().into(),
            shell: state.into(),
        }
    }
}

impl From<PersistedShellState> for ChatDomainSeed {
    fn from(state: PersistedShellState) -> Self {
        Self {
            circles: state.circles,
            contacts: state.contacts,
            sessions: state.sessions,
            groups: state.groups,
            message_store: state.message_store,
        }
    }
}
