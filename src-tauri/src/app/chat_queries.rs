use crate::domain::chat::{
    default_message_page_size, ChatDomainOverview, ChatDomainSeed, ChatSessionMessageUpdates,
    ChatSessionMessagesPage, ChatShellSnapshot, LoadSessionMessageUpdatesInput,
    LoadSessionMessagesInput, PersistedShellState, ShellStateSnapshot,
};
use crate::infra::shell_state_store;
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use serde_json::Value;

pub fn load_chat_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ChatShellSnapshot, String> {
    let repository = SqliteChatRepository::new(app_handle);
    let persisted_shell = shell_state_store::load(app_handle)?
        .and_then(deserialize_shell_state_snapshot)
        .map(|shell| {
            let selected_session_id = shell.selected_session_id.clone();
            (shell, Some(selected_session_id))
        });
    let domain = repository.load_domain_seed_preview(
        persisted_shell
            .as_ref()
            .and_then(|(_, selected_session_id)| selected_session_id.as_deref()),
        default_message_page_size(),
    )?;
    let default_shell = ShellStateSnapshot::from(ChatDomainSeed::default());
    let shell = persisted_shell
        .map(|(shell, _)| shell)
        .unwrap_or(default_shell);
    let domain = if shell.is_authenticated {
        domain
    } else {
        ChatDomainSeed::default()
    };

    Ok(ChatShellSnapshot { domain, shell })
}

pub fn save_chat_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    snapshot: ChatShellSnapshot,
) -> Result<(), String> {
    let shell_state = serde_json::to_value(snapshot.shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)
}

pub fn load_chat_session_messages<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: LoadSessionMessagesInput,
) -> Result<ChatSessionMessagesPage, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_session_messages_page(input)
}

pub fn load_chat_session_message_updates<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: LoadSessionMessageUpdatesInput,
) -> Result<ChatSessionMessageUpdates, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_session_message_updates(input)
}

pub fn load_chat_sessions_overview<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Vec<crate::domain::chat::SessionItem>, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_sessions_overview()
}

pub fn load_chat_domain_overview<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ChatDomainOverview, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_domain_overview()
}

fn deserialize_shell_state_snapshot(value: Value) -> Option<ShellStateSnapshot> {
    serde_json::from_value::<ShellStateSnapshot>(value.clone())
        .ok()
        .or_else(|| {
            serde_json::from_value::<PersistedShellState>(value)
                .ok()
                .map(ShellStateSnapshot::from)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        AdvancedPreferences, AppPreferences, AuthSessionSummary, LanguagePreference,
        LoginAccessKind, LoginAccessSummary, LoginCircleSelectionMode, LoginMethod, MessageItem,
        NotificationPreferences, PersistedShellState, TextSizePreference, ThemePreference,
        UserProfile,
    };
    use std::path::PathBuf;
    use std::sync::MutexGuard;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestAppGuard {
        _env_guard: MutexGuard<'static, ()>,
        app: tauri::App<tauri::test::MockRuntime>,
        config_root: PathBuf,
        previous_xdg_config_home: Option<String>,
    }

    impl Drop for TestAppGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous_xdg_config_home {
                std::env::set_var("XDG_CONFIG_HOME", previous);
            } else {
                std::env::remove_var("XDG_CONFIG_HOME");
            }

            let _ = std::fs::remove_dir_all(&self.config_root);
        }
    }

    fn test_app() -> TestAppGuard {
        let env_guard = crate::test_support::env_lock()
            .lock()
            .expect("env lock poisoned");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let config_root = std::env::temp_dir().join(format!("p2p-chat-tauri-test-{unique}"));
        std::fs::create_dir_all(&config_root).expect("failed to create test config root");

        let previous_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
        std::env::set_var("XDG_CONFIG_HOME", &config_root);

        let app = tauri::test::mock_app();
        TestAppGuard {
            _env_guard: env_guard,
            app,
            config_root,
            previous_xdg_config_home,
        }
    }

    #[test]
    fn save_chat_shell_snapshot_persists_shell_state_without_overwriting_domain() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: None,
                user_profile: crate::domain::chat::default_user_profile(),
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let mut snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load initial shell snapshot");
        let original_circle_name = snapshot.domain.circles[0].name.clone();
        let original_session_subtitle = snapshot.domain.sessions[0].subtitle.clone();
        let original_message_count =
            snapshot.domain.message_store[&snapshot.domain.sessions[0].id].len();
        snapshot.domain.circles[0].name = "Audit Circle".into();
        snapshot.domain.sessions[0].subtitle = "Snapshot persisted through unified save".into();
        snapshot
            .domain
            .message_store
            .entry(snapshot.domain.sessions[0].id.clone())
            .or_default()
            .push(MessageItem {
                id: "snapshot-test-message".into(),
                kind: crate::domain::chat::MessageKind::System,
                author: crate::domain::chat::MessageAuthor::System,
                body: "snapshot write boundary".into(),
                time: "now".into(),
                meta: None,
                delivery_status: None,
                remote_id: None,
                sync_source: Some(crate::domain::chat::MessageSyncSource::System),
                acked_at: None,
            });
        snapshot.shell.is_authenticated = true;
        snapshot.shell.auth_session = Some(AuthSessionSummary {
            login_method: LoginMethod::ExistingAccount,
            access: LoginAccessSummary {
                kind: LoginAccessKind::Nsec,
                label: "nsec1...runner".into(),
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: "2026-04-18T10:00:00Z".into(),
        });
        snapshot.shell.user_profile = UserProfile {
            name: "Audit Runner".into(),
            handle: "@auditrunner".into(),
            initials: "AR".into(),
            status: "QA".into(),
        };
        snapshot.shell.app_preferences = AppPreferences {
            theme: ThemePreference::Ink,
            language: LanguagePreference::ZhCn,
            text_size: TextSizePreference::Large,
        };
        snapshot.shell.notification_preferences = NotificationPreferences {
            allow_send: false,
            allow_receive: true,
            show_badge: false,
            archive_summary: false,
            mentions_only: true,
        };
        snapshot.shell.advanced_preferences = AdvancedPreferences {
            show_message_info: true,
            use_tor_network: true,
            relay_diagnostics: false,
            experimental_transport: true,
        };
        snapshot.shell.active_circle_id = snapshot.domain.circles[0].id.clone();
        snapshot.shell.selected_session_id = snapshot.domain.sessions[0].id.clone();

        save_chat_shell_snapshot(app_handle, snapshot.clone())
            .expect("failed to save chat shell snapshot");

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload chat shell snapshot");
        assert_eq!(reloaded.domain.circles[0].name, original_circle_name);
        assert_eq!(
            reloaded.domain.sessions[0].subtitle,
            original_session_subtitle
        );
        assert_eq!(
            reloaded.domain.message_store[&snapshot.domain.sessions[0].id].len(),
            original_message_count
        );
        assert!(reloaded.shell.is_authenticated);
        assert!(matches!(
            reloaded
                .shell
                .auth_session
                .as_ref()
                .map(|session| &session.login_method),
            Some(LoginMethod::ExistingAccount)
        ));
        assert_eq!(reloaded.shell.user_profile.handle, "@auditrunner");
        assert!(matches!(
            reloaded.shell.app_preferences.theme,
            ThemePreference::Ink
        ));
        assert!(matches!(
            reloaded.shell.app_preferences.language,
            LanguagePreference::ZhCn
        ));
        assert!(matches!(
            reloaded.shell.app_preferences.text_size,
            TextSizePreference::Large
        ));
        assert!(reloaded.shell.advanced_preferences.experimental_transport);
        assert!(reloaded.shell.advanced_preferences.use_tor_network);
        assert_eq!(
            reloaded.shell.selected_session_id,
            snapshot.domain.sessions[0].id
        );
    }

    #[test]
    fn load_chat_shell_snapshot_accepts_legacy_persisted_shell_state_shape() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let persisted_state = PersistedShellState {
            is_authenticated: true,
            auth_session: None,
            user_profile: UserProfile {
                name: "Legacy User".into(),
                handle: "@legacy".into(),
                initials: "LU".into(),
                status: "Legacy".into(),
            },
            circles: vec![],
            app_preferences: AppPreferences {
                theme: ThemePreference::Light,
                language: LanguagePreference::ZhCn,
                text_size: TextSizePreference::Compact,
            },
            notification_preferences: NotificationPreferences {
                allow_send: false,
                allow_receive: true,
                show_badge: false,
                archive_summary: false,
                mentions_only: true,
            },
            advanced_preferences: AdvancedPreferences {
                show_message_info: true,
                use_tor_network: true,
                relay_diagnostics: false,
                experimental_transport: true,
            },
            active_circle_id: "legacy-circle".into(),
            selected_session_id: "legacy-session".into(),
            sessions: vec![],
            contacts: vec![],
            groups: vec![],
            message_store: Default::default(),
        };

        shell_state_store::save(
            app_handle,
            serde_json::to_value(&persisted_state).expect("failed to encode legacy shell state"),
        )
        .expect("failed to seed shell state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");
        assert!(snapshot.shell.is_authenticated);
        assert_eq!(snapshot.shell.user_profile.name, "Legacy User");
        assert!(matches!(
            snapshot.shell.app_preferences.theme,
            ThemePreference::Light
        ));
        assert!(matches!(
            snapshot.shell.app_preferences.language,
            LanguagePreference::ZhCn
        ));
        assert!(matches!(
            snapshot.shell.app_preferences.text_size,
            TextSizePreference::Compact
        ));
        assert_eq!(snapshot.shell.active_circle_id, "legacy-circle");
        assert_eq!(snapshot.shell.selected_session_id, "legacy-session");
        assert!(!snapshot.domain.circles.is_empty());
        assert!(!snapshot.domain.sessions.is_empty());
    }

    #[test]
    fn load_chat_shell_snapshot_only_preloads_selected_session_messages() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: None,
                user_profile: crate::domain::chat::default_user_profile(),
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");

        assert_eq!(snapshot.domain.message_store.len(), 1);
        assert!(snapshot.domain.message_store.contains_key("alice"));
        assert_eq!(snapshot.domain.message_store["alice"].len(), 6);
    }

    #[test]
    fn load_chat_shell_snapshot_hides_domain_preview_when_logged_out() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: false,
                auth_session: None,
                user_profile: crate::domain::chat::default_user_profile(),
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: String::new(),
                selected_session_id: String::new(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");

        assert!(snapshot.domain.circles.is_empty());
        assert!(snapshot.domain.sessions.is_empty());
        assert!(snapshot.domain.message_store.is_empty());
    }

    #[test]
    fn load_chat_session_messages_returns_paginated_history() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let first_page = load_chat_session_messages(
            app_handle,
            LoadSessionMessagesInput {
                session_id: "alice".into(),
                before_message_id: None,
                limit: 2,
            },
        )
        .expect("failed to load first page");

        assert!(first_page.has_more);
        assert_eq!(
            first_page
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alice-4", "alice-5"]
        );
        assert_eq!(
            first_page.next_before_message_id.as_deref(),
            Some("alice-4")
        );

        let second_page = load_chat_session_messages(
            app_handle,
            LoadSessionMessagesInput {
                session_id: "alice".into(),
                before_message_id: first_page.next_before_message_id.clone(),
                limit: 2,
            },
        )
        .expect("failed to load second page");

        assert!(second_page.has_more);
        assert_eq!(
            second_page
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alice-2", "alice-3"]
        );
        assert_eq!(
            second_page.next_before_message_id.as_deref(),
            Some("alice-2")
        );

        let final_page = load_chat_session_messages(
            app_handle,
            LoadSessionMessagesInput {
                session_id: "alice".into(),
                before_message_id: second_page.next_before_message_id.clone(),
                limit: 2,
            },
        )
        .expect("failed to load final page");

        assert!(!final_page.has_more);
        assert_eq!(
            final_page
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alice-system", "alice-1"]
        );
        assert_eq!(final_page.next_before_message_id, None);
    }

    #[test]
    fn load_chat_session_message_updates_returns_newer_messages_incrementally() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let first_updates = load_chat_session_message_updates(
            app_handle,
            LoadSessionMessageUpdatesInput {
                session_id: "alice".into(),
                after_message_id: Some("alice-2".into()),
                limit: 2,
            },
        )
        .expect("failed to load first updates");

        assert!(first_updates.has_more);
        assert_eq!(
            first_updates
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alice-3", "alice-4"]
        );
        assert_eq!(
            first_updates.next_after_message_id.as_deref(),
            Some("alice-4")
        );

        let second_updates = load_chat_session_message_updates(
            app_handle,
            LoadSessionMessageUpdatesInput {
                session_id: "alice".into(),
                after_message_id: first_updates.next_after_message_id.clone(),
                limit: 2,
            },
        )
        .expect("failed to load second updates");

        assert!(!second_updates.has_more);
        assert_eq!(
            second_updates
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["alice-5"]
        );
        assert_eq!(second_updates.next_after_message_id, None);
    }

    #[test]
    fn load_chat_sessions_overview_reflects_session_order_changes() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        crate::app::chat_mutations::send_message(
            app_handle,
            crate::domain::chat::SendMessageInput {
                session_id: "mika".into(),
                body: "Bring this thread to the top".into(),
            },
        )
        .expect("failed to send message");

        let sessions =
            load_chat_sessions_overview(app_handle).expect("failed to load session overview");

        assert_eq!(
            sessions.first().map(|session| session.id.as_str()),
            Some("mika")
        );
        assert_eq!(
            sessions
                .iter()
                .find(|session| session.id == "mika")
                .map(|session| session.subtitle.as_str()),
            Some("Bring this thread to the top")
        );
    }

    #[test]
    fn load_chat_domain_overview_reflects_contact_and_group_changes() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        crate::app::chat_mutations::toggle_contact_block(app_handle, "alice-contact".into())
            .expect("failed to toggle contact block");
        crate::app::chat_mutations::create_group_conversation(
            app_handle,
            crate::domain::chat::CreateGroupConversationInput {
                circle_id: "main-circle".into(),
                name: "Overview Sync Crew".into(),
                member_contact_ids: vec!["alice-contact".into(), "mika-contact".into()],
            },
        )
        .expect("failed to create group conversation");

        let overview =
            load_chat_domain_overview(app_handle).expect("failed to load domain overview");

        assert_eq!(
            overview
                .contacts
                .iter()
                .find(|contact| contact.id == "alice-contact")
                .and_then(|contact| contact.blocked),
            Some(true)
        );
        assert!(overview
            .groups
            .iter()
            .any(|group| group.name == "Overview Sync Crew"));
    }
}
