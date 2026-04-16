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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageKind {
    Text,
    File,
    Audio,
    System,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CircleCreateMode {
    Invite,
    Private,
    Custom,
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
pub struct ContactItem {
    pub id: String,
    pub name: String,
    pub initials: String,
    pub handle: String,
    pub pubkey: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct SendMessageInput {
    pub session_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConversationInput {
    pub circle_id: String,
    pub contact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartConversationResult {
    pub seed: ChatDomainSeed,
    pub session_id: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedShellState {
    pub is_authenticated: bool,
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
