use crate::domain::chat::{
    ChatDomainSeed, CircleItem, ContactItem, GroupProfile, MessageItem, PersistedShellState,
    SessionItem,
};
use crate::domain::chat_repository::ChatRepository;
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use std::collections::HashMap;

pub fn load_chat_seed(app_handle: &tauri::AppHandle) -> Result<PersistedShellState, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_chat_seed()
}

pub fn load_seed_circles(app_handle: &tauri::AppHandle) -> Result<Vec<CircleItem>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_seed_circles()
}

pub fn load_seed_contacts(app_handle: &tauri::AppHandle) -> Result<Vec<ContactItem>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_seed_contacts()
}

pub fn load_seed_sessions(app_handle: &tauri::AppHandle) -> Result<Vec<SessionItem>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_seed_sessions()
}

pub fn load_seed_groups(app_handle: &tauri::AppHandle) -> Result<Vec<GroupProfile>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_seed_groups()
}

pub fn load_seed_message_store(
    app_handle: &tauri::AppHandle,
) -> Result<HashMap<String, Vec<MessageItem>>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_seed_message_store()
}

pub fn save_chat_domain_seed(
    app_handle: &tauri::AppHandle,
    seed: ChatDomainSeed,
) -> Result<(), String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.save_chat_domain_seed(seed)
}
