mod app;
mod commands;
mod domain;
mod infra;
#[cfg(test)]
mod test_support;

use crate::commands::{
    add_chat_circle, apply_chat_session_action, apply_transport_circle_action, bootstrap_status,
    create_group_conversation, load_chat_domain_overview, load_chat_session_message_updates,
    load_chat_session_messages, load_chat_sessions_overview, load_chat_shell_snapshot,
    load_transport_snapshot, merge_remote_delivery_receipts, merge_remote_session_messages,
    remove_chat_circle, retry_chat_message_delivery, save_chat_shell_snapshot, send_chat_message,
    start_direct_conversation, start_lookup_conversation, start_self_conversation,
    toggle_chat_contact_block, update_chat_circle, update_chat_group_members,
    update_chat_group_name, update_chat_message_delivery_status, update_chat_session_draft,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            bootstrap_status,
            load_chat_shell_snapshot,
            load_chat_domain_overview,
            load_chat_session_message_updates,
            load_chat_session_messages,
            load_chat_sessions_overview,
            save_chat_shell_snapshot,
            send_chat_message,
            update_chat_session_draft,
            update_chat_message_delivery_status,
            retry_chat_message_delivery,
            merge_remote_session_messages,
            merge_remote_delivery_receipts,
            start_direct_conversation,
            start_self_conversation,
            create_group_conversation,
            start_lookup_conversation,
            apply_chat_session_action,
            toggle_chat_contact_block,
            update_chat_group_name,
            update_chat_group_members,
            add_chat_circle,
            update_chat_circle,
            remove_chat_circle,
            load_transport_snapshot,
            apply_transport_circle_action
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
