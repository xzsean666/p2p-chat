mod app;
mod commands;
mod domain;
mod infra;

use crate::commands::{
    add_chat_circle, apply_chat_session_action, apply_transport_circle_action, bootstrap_status,
    load_chat_seed, load_seed_circles, load_seed_contacts, load_seed_groups,
    load_seed_message_store, load_seed_sessions, load_shell_state, load_transport_snapshot,
    remove_chat_circle, save_chat_domain_seed, save_shell_state, send_chat_message,
    start_direct_conversation, toggle_chat_contact_block, update_chat_circle,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap_status,
            load_chat_seed,
            load_seed_circles,
            load_seed_contacts,
            load_seed_sessions,
            load_seed_groups,
            load_seed_message_store,
            save_chat_domain_seed,
            send_chat_message,
            start_direct_conversation,
            apply_chat_session_action,
            toggle_chat_contact_block,
            add_chat_circle,
            update_chat_circle,
            remove_chat_circle,
            load_transport_snapshot,
            apply_transport_circle_action,
            load_shell_state,
            save_shell_state
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
