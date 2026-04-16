use crate::domain::chat::{
    ChatDomainSeed, CircleItem, ContactItem, GroupProfile, MessageItem, PersistedShellState,
    SessionItem,
};
use std::collections::HashMap;

pub trait ChatRepository {
    fn load_chat_seed(&self) -> Result<PersistedShellState, String>;
    fn load_seed_circles(&self) -> Result<Vec<CircleItem>, String>;
    fn load_seed_contacts(&self) -> Result<Vec<ContactItem>, String>;
    fn load_seed_sessions(&self) -> Result<Vec<SessionItem>, String>;
    fn load_seed_groups(&self) -> Result<Vec<GroupProfile>, String>;
    fn load_seed_message_store(&self) -> Result<HashMap<String, Vec<MessageItem>>, String>;
    fn save_chat_domain_seed(&self, seed: ChatDomainSeed) -> Result<(), String>;
}
