use crate::app::{chat_mutations, chat_queries};
use crate::domain::chat::{
    AddCircleInput, AddCircleResult, ChatDomainOverview, ChatDomainSeed, ChatSessionMessageUpdates,
    ChatSessionMessagesPage, ChatShellSnapshot, CreateGroupConversationInput,
    LoadSessionMessageUpdatesInput, LoadSessionMessagesInput, MergeRemoteDeliveryReceiptsInput,
    MergeRemoteMessagesInput, RetryMessageDeliveryInput, SendMessageInput, SessionActionInput,
    StartConversationInput, StartConversationResult, StartLookupConversationInput,
    StartSelfConversationInput, UpdateCircleInput, UpdateGroupMembersInput, UpdateGroupNameInput,
    UpdateMessageDeliveryStatusInput, UpdateSessionDraftInput,
};

#[tauri::command]
pub fn load_chat_shell_snapshot(app_handle: tauri::AppHandle) -> Result<ChatShellSnapshot, String> {
    chat_queries::load_chat_shell_snapshot(&app_handle)
}

#[tauri::command]
pub fn save_chat_shell_snapshot(
    app_handle: tauri::AppHandle,
    snapshot: ChatShellSnapshot,
) -> Result<(), String> {
    chat_queries::save_chat_shell_snapshot(&app_handle, snapshot)
}

#[tauri::command]
pub fn load_chat_session_messages(
    app_handle: tauri::AppHandle,
    input: LoadSessionMessagesInput,
) -> Result<ChatSessionMessagesPage, String> {
    chat_queries::load_chat_session_messages(&app_handle, input)
}

#[tauri::command]
pub fn load_chat_session_message_updates(
    app_handle: tauri::AppHandle,
    input: LoadSessionMessageUpdatesInput,
) -> Result<ChatSessionMessageUpdates, String> {
    chat_queries::load_chat_session_message_updates(&app_handle, input)
}

#[tauri::command]
pub fn load_chat_sessions_overview(
    app_handle: tauri::AppHandle,
) -> Result<Vec<crate::domain::chat::SessionItem>, String> {
    chat_queries::load_chat_sessions_overview(&app_handle)
}

#[tauri::command]
pub fn load_chat_domain_overview(
    app_handle: tauri::AppHandle,
) -> Result<ChatDomainOverview, String> {
    chat_queries::load_chat_domain_overview(&app_handle)
}

#[tauri::command]
pub fn send_chat_message(
    app_handle: tauri::AppHandle,
    input: SendMessageInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::send_message(&app_handle, input)
}

#[tauri::command]
pub fn update_chat_session_draft(
    app_handle: tauri::AppHandle,
    input: UpdateSessionDraftInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::update_session_draft(&app_handle, input)
}

#[tauri::command]
pub fn update_chat_message_delivery_status(
    app_handle: tauri::AppHandle,
    input: UpdateMessageDeliveryStatusInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::update_message_delivery_status(&app_handle, input)
}

#[tauri::command]
pub fn retry_chat_message_delivery(
    app_handle: tauri::AppHandle,
    input: RetryMessageDeliveryInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::retry_message_delivery(&app_handle, input)
}

#[tauri::command]
pub fn merge_remote_session_messages(
    app_handle: tauri::AppHandle,
    input: MergeRemoteMessagesInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::merge_remote_messages(&app_handle, input)
}

#[tauri::command]
pub fn merge_remote_delivery_receipts(
    app_handle: tauri::AppHandle,
    input: MergeRemoteDeliveryReceiptsInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::merge_remote_delivery_receipts(&app_handle, input)
}

#[tauri::command]
pub fn start_direct_conversation(
    app_handle: tauri::AppHandle,
    input: StartConversationInput,
) -> Result<StartConversationResult, String> {
    chat_mutations::start_conversation(&app_handle, input)
}

#[tauri::command]
pub fn start_self_conversation(
    app_handle: tauri::AppHandle,
    input: StartSelfConversationInput,
) -> Result<StartConversationResult, String> {
    chat_mutations::start_self_conversation(&app_handle, input)
}

#[tauri::command]
pub fn create_group_conversation(
    app_handle: tauri::AppHandle,
    input: CreateGroupConversationInput,
) -> Result<StartConversationResult, String> {
    chat_mutations::create_group_conversation(&app_handle, input)
}

#[tauri::command]
pub fn start_lookup_conversation(
    app_handle: tauri::AppHandle,
    input: StartLookupConversationInput,
) -> Result<StartConversationResult, String> {
    chat_mutations::start_lookup_conversation(&app_handle, input)
}

#[tauri::command]
pub fn apply_chat_session_action(
    app_handle: tauri::AppHandle,
    input: SessionActionInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::apply_session_action(&app_handle, input)
}

#[tauri::command]
pub fn toggle_chat_contact_block(
    app_handle: tauri::AppHandle,
    contact_id: String,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::toggle_contact_block(&app_handle, contact_id)
}

#[tauri::command]
pub fn update_chat_group_name(
    app_handle: tauri::AppHandle,
    input: UpdateGroupNameInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::update_group_name(&app_handle, input)
}

#[tauri::command]
pub fn update_chat_group_members(
    app_handle: tauri::AppHandle,
    input: UpdateGroupMembersInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::update_group_members(&app_handle, input)
}

#[tauri::command]
pub fn add_chat_circle(
    app_handle: tauri::AppHandle,
    input: AddCircleInput,
) -> Result<AddCircleResult, String> {
    chat_mutations::add_circle(&app_handle, input)
}

#[tauri::command]
pub fn update_chat_circle(
    app_handle: tauri::AppHandle,
    input: UpdateCircleInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::update_circle(&app_handle, input)
}

#[tauri::command]
pub fn remove_chat_circle(
    app_handle: tauri::AppHandle,
    circle_id: String,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::remove_circle(&app_handle, circle_id)
}
