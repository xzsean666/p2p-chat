use crate::app::{chat_mutations, chat_queries};
use crate::domain::chat::{
    AddCircleInput, AddCircleResult, ChatDomainSeed, CircleItem, ContactItem, GroupProfile,
    MessageItem, PersistedShellState, SendMessageInput, SessionActionInput, SessionItem,
    StartConversationInput, StartConversationResult, UpdateCircleInput,
};
use std::collections::HashMap;

#[tauri::command]
pub fn load_chat_seed(app_handle: tauri::AppHandle) -> Result<PersistedShellState, String> {
    chat_queries::load_chat_seed(&app_handle)
}

#[tauri::command]
pub fn load_seed_circles(app_handle: tauri::AppHandle) -> Result<Vec<CircleItem>, String> {
    chat_queries::load_seed_circles(&app_handle)
}

#[tauri::command]
pub fn load_seed_contacts(app_handle: tauri::AppHandle) -> Result<Vec<ContactItem>, String> {
    chat_queries::load_seed_contacts(&app_handle)
}

#[tauri::command]
pub fn load_seed_sessions(app_handle: tauri::AppHandle) -> Result<Vec<SessionItem>, String> {
    chat_queries::load_seed_sessions(&app_handle)
}

#[tauri::command]
pub fn load_seed_groups(app_handle: tauri::AppHandle) -> Result<Vec<GroupProfile>, String> {
    chat_queries::load_seed_groups(&app_handle)
}

#[tauri::command]
pub fn load_seed_message_store(
    app_handle: tauri::AppHandle,
) -> Result<HashMap<String, Vec<MessageItem>>, String> {
    chat_queries::load_seed_message_store(&app_handle)
}

#[tauri::command]
pub fn save_chat_domain_seed(
    app_handle: tauri::AppHandle,
    seed: ChatDomainSeed,
) -> Result<(), String> {
    chat_queries::save_chat_domain_seed(&app_handle, seed)
}

#[tauri::command]
pub fn send_chat_message(
    app_handle: tauri::AppHandle,
    input: SendMessageInput,
) -> Result<ChatDomainSeed, String> {
    chat_mutations::send_message(&app_handle, input)
}

#[tauri::command]
pub fn start_direct_conversation(
    app_handle: tauri::AppHandle,
    input: StartConversationInput,
) -> Result<StartConversationResult, String> {
    chat_mutations::start_conversation(&app_handle, input)
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
