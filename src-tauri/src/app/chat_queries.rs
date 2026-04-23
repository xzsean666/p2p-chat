use crate::app::{auth_access, chat_mutations, shell_auth};
use crate::domain::chat::{
    default_message_page_size, AddCircleInput, AuthRuntimeClientUriSummary, ChatDomainOverview,
    ChatDomainSeed, ChatSessionMessageUpdates, ChatSessionMessagesPage, ChatShellSnapshot,
    LoadSessionMessageUpdatesInput, LoadSessionMessagesInput, LocalAccountSecretSummary,
    LoginAccessKind, LoginCircleSelectionInput, LoginCircleSelectionMode, LoginCompletionInput,
    LoginMethod, RestorableCircleEntry, RestoreCircleInput, ShellStateSnapshot,
    UpdateAuthRuntimeInput,
};
use crate::domain::chat_repository::ChatRepository;
use crate::infra::shell_state_store;
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

pub fn load_chat_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ChatShellSnapshot, String> {
    let repository = SqliteChatRepository::new(app_handle);
    let mut shell = load_saved_shell_snapshot(app_handle)?;
    if shell.is_authenticated && !shell.advanced_preferences.experimental_transport {
        shell.advanced_preferences.experimental_transport = true;
    }
    let domain = repository.load_domain_seed_preview(
        normalized_non_empty(Some(&shell.selected_session_id)),
        default_message_page_size(),
    )?;
    let domain = if shell.is_authenticated {
        domain
    } else {
        ChatDomainSeed::default()
    };

    Ok(ChatShellSnapshot { domain, shell })
}

pub fn sync_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ShellStateSnapshot, String> {
    shell_auth::sync_saved_shell_snapshot(app_handle)
}

pub fn load_local_account_secret_summary<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<LocalAccountSecretSummary>, String> {
    let shell = load_saved_shell_snapshot(app_handle)?;
    let Some(auth_session) = shell.auth_session.as_ref() else {
        return Ok(None);
    };

    shell_auth::load_local_account_secret_summary(app_handle, auth_session)
}

pub fn save_chat_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    snapshot: ChatShellSnapshot,
) -> Result<(), String> {
    let shell_state = serde_json::to_value(snapshot.shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)
}

pub fn bootstrap_auth_session<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: LoginCompletionInput,
) -> Result<ShellStateSnapshot, String> {
    let previous_shell = load_saved_shell_snapshot(app_handle)?;
    let resolved_input = resolve_login_input(app_handle, &previous_shell, &input)?;
    let mut shell = build_authenticated_shell(previous_shell, &resolved_input, true)?;
    let auth_session = shell
        .auth_session
        .clone()
        .ok_or_else(|| "authenticated shell is missing auth session summary".to_string())?;
    shell_auth::persist_auth_runtime_credential(app_handle, &auth_session, &resolved_input.access)?;
    shell.auth_runtime_binding = shell_auth::persist_auth_runtime_binding(
        app_handle,
        &auth_session,
        &resolved_input.access,
    )?;
    if let Some(runtime) = shell.auth_runtime.clone() {
        shell.auth_runtime = Some(shell_auth::persist_auth_runtime(
            app_handle,
            &auth_session,
            &runtime,
        )?);
    }

    if matches!(
        resolved_input.circle_selection.mode,
        LoginCircleSelectionMode::Existing
    ) {
        if let Some(circle_id) =
            normalized_non_empty(resolved_input.circle_selection.circle_id.as_deref())
        {
            shell.active_circle_id = circle_id.to_string();
        }
    }

    let shell_state = serde_json::to_value(&shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)?;
    Ok(shell)
}

pub fn complete_login<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: LoginCompletionInput,
) -> Result<ChatShellSnapshot, String> {
    let previous_shell = load_saved_shell_snapshot(app_handle)?;
    let resolved_input = resolve_login_input(app_handle, &previous_shell, &input)?;
    let mut shell = build_authenticated_shell(previous_shell, &resolved_input, false)?;
    let auth_session = shell
        .auth_session
        .clone()
        .ok_or_else(|| "authenticated shell is missing auth session summary".to_string())?;
    shell_auth::persist_auth_runtime_credential(app_handle, &auth_session, &resolved_input.access)?;
    shell.auth_runtime_binding = shell_auth::persist_auth_runtime_binding(
        app_handle,
        &auth_session,
        &resolved_input.access,
    )?;
    if let Some(runtime) = shell.auth_runtime.clone() {
        shell.auth_runtime = Some(shell_auth::persist_auth_runtime(
            app_handle,
            &auth_session,
            &runtime,
        )?);
    }
    let (domain_seed, resolved_circle_id, next_restorable_circles) =
        resolve_login_circle_selection(app_handle, &resolved_input, &shell.restorable_circles)?;

    shell.restorable_circles = next_restorable_circles;
    shell.active_circle_id = resolve_active_circle_id(&domain_seed, resolved_circle_id.as_deref());
    shell.selected_session_id = resolve_selected_session_id(
        &domain_seed,
        &shell.active_circle_id,
        normalized_non_empty(Some(&shell.selected_session_id)),
    );

    let shell_state = serde_json::to_value(&shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)?;

    let repository = SqliteChatRepository::new(app_handle);
    let domain = repository.load_domain_seed_preview(
        normalized_non_empty(Some(&shell.selected_session_id)),
        default_message_page_size(),
    )?;

    Ok(ChatShellSnapshot { domain, shell })
}

pub fn logout_chat_session<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ChatShellSnapshot, String> {
    let previous_shell = load_saved_shell_snapshot(app_handle)?;
    shell_auth::clear_auth_runtime(app_handle)?;
    shell_auth::clear_auth_runtime_client(app_handle)?;
    shell_auth::clear_auth_runtime_credential(app_handle)?;
    shell_auth::clear_auth_runtime_binding(app_handle)?;
    let shell = build_logged_out_shell(previous_shell);
    let shell_state = serde_json::to_value(&shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)?;

    Ok(ChatShellSnapshot {
        domain: ChatDomainSeed::default(),
        shell,
    })
}

pub fn update_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: UpdateAuthRuntimeInput,
) -> Result<ShellStateSnapshot, String> {
    let mut shell = load_saved_shell_snapshot(app_handle)?;
    let auth_session = shell.auth_session.clone().ok_or_else(|| {
        "auth runtime update requires an authenticated session summary".to_string()
    })?;
    let runtime = shell_auth::update_auth_runtime(&shell, &input)?;
    let runtime = shell_auth::persist_auth_runtime(app_handle, &auth_session, &runtime)?;
    shell.auth_runtime = Some(runtime);
    let shell_state = serde_json::to_value(&shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)?;
    Ok(shell)
}

pub fn load_auth_runtime_client_uri<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<Option<AuthRuntimeClientUriSummary>, String> {
    let shell = load_saved_shell_snapshot(app_handle)?;
    let Some(auth_session) = shell.auth_session.as_ref() else {
        return Ok(None);
    };

    if !matches!(
        auth_session.access.kind,
        crate::domain::chat::LoginAccessKind::Bunker
            | crate::domain::chat::LoginAccessKind::NostrConnect
    ) {
        return Ok(None);
    }

    shell_auth::build_standard_nostrconnect_client_uri(app_handle, auth_session).map(Some)
}

pub fn load_pending_auth_runtime_client_uri<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<AuthRuntimeClientUriSummary, String> {
    shell_auth::load_pending_standard_nostrconnect_client_uri(app_handle)
}

pub fn await_pending_auth_runtime_client_pairing<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<String, String> {
    shell_auth::await_pending_standard_nostrconnect_client_pairing(app_handle)
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

fn load_saved_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ShellStateSnapshot, String> {
    shell_auth::load_saved_shell_snapshot(app_handle)
}

fn build_authenticated_shell(
    previous_shell: ShellStateSnapshot,
    input: &LoginCompletionInput,
    allow_incomplete_invite_selection: bool,
) -> Result<ShellStateSnapshot, String> {
    let user_profile = normalize_user_profile(&input.user_profile)?;
    validate_access_input(input)?;
    if !(allow_incomplete_invite_selection
        && matches!(
            input.circle_selection.mode,
            LoginCircleSelectionMode::Invite
        ))
    {
        validate_circle_selection(input, &previous_shell.restorable_circles)?;
    }

    let mut shell = previous_shell;
    shell.is_authenticated = true;
    let auth_session = build_auth_session_summary(input)?;
    shell.auth_session = Some(auth_session.clone());
    shell.auth_runtime = Some(build_auth_runtime_summary(&auth_session));
    shell.advanced_preferences.experimental_transport = true;
    shell.user_profile = user_profile;
    Ok(shell)
}

fn resolve_login_input<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    previous_shell: &ShellStateSnapshot,
    input: &LoginCompletionInput,
) -> Result<LoginCompletionInput, String> {
    if !matches!(input.method, LoginMethod::QuickStart) {
        return Ok(input.clone());
    }

    if !matches!(
        input.access.kind,
        LoginAccessKind::LocalProfile | LoginAccessKind::HexKey
    ) {
        return Err("quick start requires generated local key access".into());
    }

    if normalized_non_empty(input.access.value.as_deref()).is_some() {
        let mut resolved_input = input.clone();
        resolved_input.access.kind = LoginAccessKind::HexKey;
        return Ok(resolved_input);
    }

    if let Some(previous_auth_session) = previous_shell.auth_session.as_ref().filter(|session| {
        matches!(session.login_method, LoginMethod::QuickStart)
            && matches!(
                session.access.kind,
                LoginAccessKind::LocalProfile | LoginAccessKind::HexKey
            )
    }) {
        if let Some(credential) =
            shell_auth::load_auth_runtime_credential_for_session(app_handle, previous_auth_session)?
        {
            let mut resolved_input = input.clone();
            resolved_input.access.kind = LoginAccessKind::HexKey;
            resolved_input.access.value = Some(credential.secret_key_hex);
            resolved_input.logged_in_at = Some(previous_auth_session.logged_in_at.clone());
            return Ok(resolved_input);
        }
    }

    let mut resolved_input = input.clone();
    resolved_input.access.kind = LoginAccessKind::HexKey;
    resolved_input.access.value = Some(auth_access::generate_secret_key_hex());
    Ok(resolved_input)
}

fn build_logged_out_shell(previous_shell: ShellStateSnapshot) -> ShellStateSnapshot {
    ShellStateSnapshot {
        is_authenticated: false,
        auth_session: None,
        auth_runtime: None,
        auth_runtime_binding: None,
        user_profile: crate::domain::chat::default_user_profile(),
        restorable_circles: previous_shell.restorable_circles,
        app_preferences: previous_shell.app_preferences,
        notification_preferences: previous_shell.notification_preferences,
        advanced_preferences: previous_shell.advanced_preferences,
        active_circle_id: String::new(),
        selected_session_id: String::new(),
    }
}

fn validate_access_input(input: &LoginCompletionInput) -> Result<(), String> {
    auth_access::resolve_login_access_summary(input).map(|_| ())
}

fn validate_circle_selection(
    input: &LoginCompletionInput,
    restorable_circles: &[RestorableCircleEntry],
) -> Result<(), String> {
    match input.circle_selection.mode {
        LoginCircleSelectionMode::Existing => {
            if normalized_non_empty(input.circle_selection.circle_id.as_deref()).is_none() {
                return Err("existing circle selection requires a circle id".into());
            }
        }
        LoginCircleSelectionMode::Invite => {
            let invite_code = normalized_non_empty(input.circle_selection.invite_code.as_deref())
                .unwrap_or_default();
            if invite_code.len() < 6 {
                return Err(
                    "invite selection requires an invite code with at least 6 characters".into(),
                );
            }
        }
        LoginCircleSelectionMode::Custom => {
            let name =
                normalized_non_empty(input.circle_selection.name.as_deref()).unwrap_or_default();
            if name.len() < 2 {
                return Err("custom circle selection requires a name".into());
            }

            let relay =
                normalized_non_empty(input.circle_selection.relay.as_deref()).unwrap_or_default();
            if !relay_looks_valid(relay) {
                return Err("custom circle selection requires a relay-like endpoint".into());
            }
        }
        LoginCircleSelectionMode::Restore => {
            if matches!(input.method, LoginMethod::QuickStart) {
                return Err("quick start cannot use restore catalog selection".into());
            }

            if restorable_circles.is_empty() {
                return Err("restore selection requires at least one archived circle".into());
            }

            resolve_selected_restore_entries(&input.circle_selection, restorable_circles)?;
        }
    }

    Ok(())
}

fn resolve_selected_restore_entries(
    selection: &LoginCircleSelectionInput,
    restorable_circles: &[RestorableCircleEntry],
) -> Result<Vec<RestorableCircleEntry>, String> {
    let mut selected_entries = Vec::new();
    let selected_relays = selection
        .relays
        .as_ref()
        .map(|relays| {
            relays
                .iter()
                .filter_map(|relay| normalized_non_empty(Some(relay.as_str())).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if !selected_relays.is_empty() {
        for relay in selected_relays {
            let entry = restorable_circles
                .iter()
                .find(|entry| same_relay(&entry.relay, &relay))
                .cloned()
                .ok_or_else(|| {
                    "restore selection relay is not present in the local restore catalog"
                        .to_string()
                })?;
            if !selected_entries
                .iter()
                .any(|selected: &RestorableCircleEntry| same_relay(&selected.relay, &entry.relay))
            {
                selected_entries.push(entry);
            }
        }
    } else if let Some(relay) = normalized_non_empty(selection.relay.as_deref()) {
        let entry = restorable_circles
            .iter()
            .find(|entry| same_relay(&entry.relay, relay))
            .cloned()
            .ok_or_else(|| {
                "restore selection relay is not present in the local restore catalog".to_string()
            })?;
        selected_entries.push(entry);
    } else if let Some(entry) = restorable_circles.first().cloned() {
        selected_entries.push(entry);
    }

    if selected_entries.is_empty() {
        return Err("restore selection requires a local catalog entry".into());
    }

    Ok(selected_entries)
}

fn resolve_login_circle_selection<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: &LoginCompletionInput,
    restorable_circles: &[RestorableCircleEntry],
) -> Result<(ChatDomainSeed, Option<String>, Vec<RestorableCircleEntry>), String> {
    match input.circle_selection.mode {
        LoginCircleSelectionMode::Existing => {
            let repository = SqliteChatRepository::new(app_handle);
            let domain_seed = repository.load_domain_seed()?;
            let circle_id = normalized_non_empty(input.circle_selection.circle_id.as_deref())
                .ok_or_else(|| "existing circle selection requires a circle id".to_string())?;

            if !domain_seed
                .circles
                .iter()
                .any(|circle| circle.id == circle_id)
            {
                return Err(format!("existing circle not found: {circle_id}"));
            }

            Ok((
                domain_seed,
                Some(circle_id.to_string()),
                restorable_circles.to_vec(),
            ))
        }
        LoginCircleSelectionMode::Invite => {
            let result = chat_mutations::add_circle(
                app_handle,
                AddCircleInput {
                    mode: crate::domain::chat::CircleCreateMode::Invite,
                    name: input
                        .circle_selection
                        .name
                        .clone()
                        .unwrap_or_else(|| "Invite Circle".into()),
                    relay: None,
                    invite_code: Some(
                        input
                            .circle_selection
                            .invite_code
                            .clone()
                            .unwrap_or_default(),
                    ),
                },
            )?;
            let next_restorable_circles = remove_restorable_circle_for_circle(
                restorable_circles,
                &result.seed,
                &result.circle_id,
            );
            Ok((result.seed, Some(result.circle_id), next_restorable_circles))
        }
        LoginCircleSelectionMode::Custom => {
            let result = chat_mutations::add_circle(
                app_handle,
                AddCircleInput {
                    mode: crate::domain::chat::CircleCreateMode::Custom,
                    name: input
                        .circle_selection
                        .name
                        .clone()
                        .unwrap_or_else(|| "Custom Relay".into()),
                    relay: Some(input.circle_selection.relay.clone().unwrap_or_default()),
                    invite_code: None,
                },
            )?;
            let next_restorable_circles = remove_restorable_circle_for_circle(
                restorable_circles,
                &result.seed,
                &result.circle_id,
            );
            Ok((result.seed, Some(result.circle_id), next_restorable_circles))
        }
        LoginCircleSelectionMode::Restore => {
            let selected_entries =
                resolve_selected_restore_entries(&input.circle_selection, restorable_circles)?;
            let mut final_seed = None;
            let mut primary_circle_id = None;
            let mut next_restorable_circles = restorable_circles.to_vec();

            for entry in selected_entries {
                let result = chat_mutations::restore_circle(
                    app_handle,
                    RestoreCircleInput {
                        name: entry.name.clone(),
                        relay: entry.relay.clone(),
                        circle_type: entry.circle_type.clone(),
                        description: entry.description.clone(),
                    },
                )?;

                if primary_circle_id.is_none() {
                    primary_circle_id = Some(result.circle_id.clone());
                }

                next_restorable_circles =
                    remove_restorable_circle_by_relay(&next_restorable_circles, &entry.relay);
                final_seed = Some(result.seed);
            }

            Ok((
                final_seed.ok_or_else(|| {
                    "restore selection requires a local catalog entry".to_string()
                })?,
                primary_circle_id,
                next_restorable_circles,
            ))
        }
    }
}

fn build_auth_session_summary(
    input: &LoginCompletionInput,
) -> Result<crate::domain::chat::AuthSessionSummary, String> {
    Ok(crate::domain::chat::AuthSessionSummary {
        login_method: input.method.clone(),
        access: auth_access::resolve_login_access_summary(input)?,
        circle_selection_mode: input.circle_selection.mode.clone(),
        logged_in_at: input
            .logged_in_at
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(current_login_timestamp),
    })
}

fn build_auth_runtime_summary(
    auth_session: &crate::domain::chat::AuthSessionSummary,
) -> crate::domain::chat::AuthRuntimeSummary {
    shell_auth::derive_auth_runtime_from_session(auth_session)
}

fn normalize_user_profile(
    user_profile: &crate::domain::chat::UserProfile,
) -> Result<crate::domain::chat::UserProfile, String> {
    let name = user_profile.name.trim();
    if name.chars().count() < 2 {
        return Err("user profile name must contain at least 2 characters".into());
    }

    let handle_body = user_profile
        .handle
        .trim()
        .trim_start_matches('@')
        .chars()
        .filter(|char| char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    if handle_body.len() < 3 {
        return Err("user profile handle must contain at least 3 normalized characters".into());
    }

    let initials = normalize_initials(&user_profile.initials, name);
    let status = normalized_non_empty(Some(&user_profile.status))
        .unwrap_or("Circle member")
        .to_string();

    Ok(crate::domain::chat::UserProfile {
        name: name.to_string(),
        handle: format!("@{handle_body}"),
        initials,
        status,
    })
}

fn normalize_initials(initials: &str, name: &str) -> String {
    let normalized = initials
        .trim()
        .chars()
        .filter(|char| char.is_ascii_alphanumeric())
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    if !normalized.is_empty() {
        return normalized;
    }

    let derived = name
        .split_whitespace()
        .filter_map(|token| token.chars().next())
        .take(2)
        .collect::<String>()
        .to_ascii_uppercase();
    if !derived.is_empty() {
        return derived;
    }

    "XC".into()
}

fn normalized_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn resolve_public_relay_shortcut(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "0xchat" => Some("wss://relay.0xchat.com"),
        "damus" => Some("wss://relay.damus.io"),
        "nos" => Some("wss://nos.lol"),
        "primal" => Some("wss://relay.primal.net"),
        "yabu" => Some("wss://yabu.me"),
        "nostrband" => Some("wss://relay.nostr.band"),
        _ => None,
    }
}

fn relay_looks_valid(value: &str) -> bool {
    let normalized = value.trim();
    if normalized.is_empty() {
        return false;
    }

    let candidate = resolve_public_relay_shortcut(normalized)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if normalized.contains("://") {
                normalized.to_string()
            } else {
                format!("wss://{normalized}")
            }
        });
    let Ok(parsed) = Url::parse(&candidate) else {
        return false;
    };

    matches!(parsed.scheme(), "ws" | "wss") && parsed.host_str().is_some()
}

fn same_relay(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn current_login_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn remove_restorable_circle_for_circle(
    restorable_circles: &[RestorableCircleEntry],
    domain_seed: &ChatDomainSeed,
    circle_id: &str,
) -> Vec<RestorableCircleEntry> {
    let relay = domain_seed
        .circles
        .iter()
        .find(|circle| circle.id == circle_id)
        .map(|circle| circle.relay.as_str());

    if let Some(relay) = relay {
        return remove_restorable_circle_by_relay(restorable_circles, relay);
    }

    restorable_circles.to_vec()
}

fn remove_restorable_circle_by_relay(
    restorable_circles: &[RestorableCircleEntry],
    relay: &str,
) -> Vec<RestorableCircleEntry> {
    restorable_circles
        .iter()
        .filter(|entry| !same_relay(&entry.relay, relay))
        .cloned()
        .collect()
}

fn resolve_active_circle_id(
    domain_seed: &ChatDomainSeed,
    preferred_circle_id: Option<&str>,
) -> String {
    preferred_circle_id
        .filter(|circle_id| {
            domain_seed
                .circles
                .iter()
                .any(|circle| circle.id == *circle_id)
        })
        .map(ToOwned::to_owned)
        .or_else(|| domain_seed.circles.first().map(|circle| circle.id.clone()))
        .unwrap_or_default()
}

fn resolve_selected_session_id(
    domain_seed: &ChatDomainSeed,
    active_circle_id: &str,
    preferred_session_id: Option<&str>,
) -> String {
    if active_circle_id.trim().is_empty() {
        return String::new();
    }

    let visible_sessions = domain_seed
        .sessions
        .iter()
        .filter(|session| {
            session.circle_id == active_circle_id && !session.archived.unwrap_or(false)
        })
        .collect::<Vec<_>>();

    preferred_session_id
        .filter(|session_id| {
            visible_sessions
                .iter()
                .any(|session| session.id == *session_id)
        })
        .map(ToOwned::to_owned)
        .or_else(|| visible_sessions.first().map(|session| session.id.clone()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{
        AdvancedPreferences, AppPreferences, AuthRuntimeBindingSummary, AuthRuntimeState,
        AuthRuntimeSummary, AuthSessionSummary, LanguagePreference, LoginAccessInput,
        LoginAccessKind, LoginAccessSummary, LoginCircleSelectionInput, LoginCircleSelectionMode,
        LoginCompletionInput, LoginMethod, MessageItem, NotificationPreferences,
        PersistedShellState, RestorableCircleEntry, TextSizePreference, ThemePreference,
        UpdateAuthRuntimeInput, UserProfile,
    };
    use crate::infra::{
        auth_runtime_binding_store, auth_runtime_client_store, auth_runtime_credential_store,
        auth_runtime_state_store, pending_auth_runtime_client_store,
    };
    use nostr_connect::prelude::{
        nip44, EventBuilder as NostrEventBuilder, JsonUtil, Keys as NostrKeys, NostrConnectMessage,
        NostrConnectRequest, NostrConnectResponse, PublicKey as NostrPublicKey, ResponseResult,
        ToBech32,
    };
    use secp256k1::{Secp256k1, SecretKey};
    use serde_json::json;
    use std::path::PathBuf;
    use std::str::FromStr;
    use std::sync::MutexGuard;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use tungstenite::Message as WebSocketMessage;

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

    const VALID_HEX_SECRET_KEY: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    const VALID_TEST_NPUB: &str = "npub1fu64hh9hes90w2808n8tjc2ajp5yhddjef0ctx4s7zmsgp6cwx4qgy4eg9";
    const VALID_TEST_NPUB_LABEL: &str = "npub1fu64h...gy4eg9";
    const TEST_BUNKER_SIGNER_SECRET_KEY: &str =
        "2222222222222222222222222222222222222222222222222222222222222222";
    const TEST_BUNKER_USER_SECRET_KEY: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";
    const TEST_BUNKER_SHARED_SECRET: &str = "shared-secret";

    fn valid_binding_pubkey_hex() -> String {
        let secret_key =
            SecretKey::from_str(VALID_HEX_SECRET_KEY).expect("valid test secret key should parse");
        let secp = Secp256k1::new();
        let (pubkey, _) = secret_key.x_only_public_key(&secp);
        pubkey
            .serialize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn seed_auth_runtime_binding(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        login_method: LoginMethod,
        access_kind: LoginAccessKind,
        stored_at: &str,
    ) {
        let pubkey = valid_binding_pubkey_hex();
        let value = match access_kind {
            LoginAccessKind::Bunker => {
                format!("bunker://{pubkey}?relay=wss://relay.example.com")
            }
            LoginAccessKind::NostrConnect => format!(
                "nostrconnect://{pubkey}?relay=wss://relay.example.com&secret=shared-secret"
            ),
            _ => return,
        };

        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method,
                access_kind,
                value,
                stored_at: stored_at.into(),
            },
        )
        .expect("failed to seed auth runtime binding store");
    }

    fn seed_pending_auth_runtime_client(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        paired_bunker_uri: Option<String>,
    ) -> pending_auth_runtime_client_store::StoredPendingAuthRuntimeClient {
        seed_pending_auth_runtime_client_with_relays(
            app_handle,
            vec![
                "wss://relay.damus.io".into(),
                "wss://nos.lol".into(),
                "wss://relay.primal.net".into(),
            ],
            paired_bunker_uri,
        )
    }

    fn seed_pending_auth_runtime_client_with_relays(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        relays: Vec<String>,
        paired_bunker_uri: Option<String>,
    ) -> pending_auth_runtime_client_store::StoredPendingAuthRuntimeClient {
        let client_keys = NostrKeys::generate();
        let stored_client = pending_auth_runtime_client_store::StoredPendingAuthRuntimeClient {
            public_key: client_keys.public_key().to_hex(),
            secret_key_hex: client_keys.secret_key().to_secret_hex(),
            relays,
            client_name: "XChat Desktop".into(),
            stored_at: "2026-04-22T09:42:00Z".into(),
            paired_bunker_uri,
        };
        pending_auth_runtime_client_store::save(app_handle, &stored_client)
            .expect("failed to seed pending auth runtime client store");
        stored_client
    }

    fn signer_login_input(uri: &str, logged_in_at: &str) -> LoginCompletionInput {
        LoginCompletionInput {
            method: LoginMethod::Signer,
            access: LoginAccessInput {
                kind: LoginAccessKind::Bunker,
                value: Some(uri.into()),
            },
            user_profile: UserProfile {
                name: "Nora Blake".into(),
                handle: "@nora".into(),
                initials: "NB".into(),
                status: "Research".into(),
            },
            circle_selection: LoginCircleSelectionInput {
                mode: LoginCircleSelectionMode::Custom,
                circle_id: None,
                invite_code: None,
                name: Some("Public Relay".into()),
                relay: Some("wss://relay.damus.io".into()),
                relays: None,
            },
            logged_in_at: Some(logged_in_at.into()),
        }
    }

    fn bunker_signer_public_key_hex() -> String {
        NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
            .expect("test bunker signer secret should parse")
            .public_key()
            .to_hex()
    }

    fn bunker_user_public_key_npub() -> String {
        NostrKeys::parse(TEST_BUNKER_USER_SECRET_KEY)
            .expect("test bunker user secret should parse")
            .public_key()
            .to_bech32()
            .expect("test bunker user pubkey should encode")
    }

    fn spawn_bunker_handshake_relay_server() -> (String, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let handle = std::thread::spawn(move || {
            let signer_keys = NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
                .expect("test bunker signer secret should parse");
            let user_public_key = NostrKeys::parse(TEST_BUNKER_USER_SECRET_KEY)
                .expect("test bunker user secret should parse")
                .public_key();
            let (stream, _) = listener
                .accept()
                .expect("relay should accept one connection");
            let mut socket = tungstenite::accept(stream).expect("relay websocket handshake");
            socket
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("relay read timeout should be configurable");
            let mut subscription_id = None::<String>;

            loop {
                match socket.read() {
                    Ok(WebSocketMessage::Text(payload)) => {
                        let message: serde_json::Value =
                            serde_json::from_str(&payload).expect("relay payload should be json");
                        let Some(kind) = message.get(0).and_then(|value| value.as_str()) else {
                            continue;
                        };

                        match kind {
                            "REQ" => {
                                let req_id = message
                                    .get(1)
                                    .and_then(|value| value.as_str())
                                    .expect("REQ should include subscription id")
                                    .to_string();
                                subscription_id = Some(req_id.clone());
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!(["EOSE", req_id]).to_string().into(),
                                    ))
                                    .expect("relay should acknowledge subscription");
                            }
                            "EVENT" => {
                                let event = message
                                    .get(1)
                                    .cloned()
                                    .expect("EVENT should include event payload");
                                let sender_pubkey = NostrPublicKey::parse(
                                    event
                                        .get("pubkey")
                                        .and_then(|value| value.as_str())
                                        .expect("event should include sender pubkey"),
                                )
                                .expect("event pubkey should parse");
                                let plaintext = nip44::decrypt(
                                    signer_keys.secret_key(),
                                    &sender_pubkey,
                                    event["content"]
                                        .as_str()
                                        .expect("event content should be a string"),
                                )
                                .expect("relay should decrypt nip46 payload");
                                let message = NostrConnectMessage::from_json(plaintext)
                                    .expect("nip46 message should parse");
                                let request_id = message.id().to_string();
                                let request = message
                                    .to_request()
                                    .expect("relay should receive a request");
                                let should_close =
                                    matches!(request, NostrConnectRequest::GetPublicKey);
                                let response = match request {
                                    NostrConnectRequest::Connect {
                                        remote_signer_public_key,
                                        secret,
                                    } => {
                                        assert_eq!(
                                            remote_signer_public_key,
                                            signer_keys.public_key()
                                        );
                                        assert_eq!(
                                            secret.as_deref(),
                                            Some(TEST_BUNKER_SHARED_SECRET)
                                        );
                                        NostrConnectResponse::with_result(ResponseResult::Ack)
                                    }
                                    NostrConnectRequest::GetPublicKey => {
                                        NostrConnectResponse::with_result(
                                            ResponseResult::GetPublicKey(user_public_key),
                                        )
                                    }
                                    _ => NostrConnectResponse::with_error(
                                        "unsupported test bunker request",
                                    ),
                                };
                                let response_event = NostrEventBuilder::nostr_connect(
                                    &signer_keys,
                                    sender_pubkey,
                                    NostrConnectMessage::response(request_id, response),
                                )
                                .expect("relay response event should build")
                                .sign_with_keys(&signer_keys)
                                .expect("relay response event should sign");
                                let event_id = event
                                    .get("id")
                                    .and_then(|value| value.as_str())
                                    .expect("event should include id");
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!(["OK", event_id, true, ""]).to_string().into(),
                                    ))
                                    .expect("relay should ack client EVENT");
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!([
                                            "EVENT",
                                            subscription_id
                                                .clone()
                                                .expect("subscription should be registered"),
                                            serde_json::to_value(response_event)
                                                .expect("response event should serialize"),
                                        ])
                                        .to_string()
                                        .into(),
                                    ))
                                    .expect("relay should forward signer response");

                                if should_close {
                                    socket
                                        .close(None)
                                        .expect("relay should close the websocket");
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(WebSocketMessage::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => panic!("test bunker relay should not error: {error}"),
                }
            }
        });

        (format!("ws://{}", address), handle)
    }

    fn spawn_client_pairing_relay_server(
        client_public_key_hex: String,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let handle = std::thread::spawn(move || {
            let signer_keys = NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
                .expect("test bunker signer secret should parse");
            let user_public_key = NostrKeys::parse(TEST_BUNKER_USER_SECRET_KEY)
                .expect("test bunker user secret should parse")
                .public_key();
            let client_public_key = NostrPublicKey::parse(&client_public_key_hex)
                .expect("client public key should parse");
            let (stream, _) = listener
                .accept()
                .expect("relay should accept one connection");
            let mut socket = tungstenite::accept(stream).expect("relay websocket handshake");
            socket
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("relay read timeout should be configurable");
            let mut subscription_id = None::<String>;

            loop {
                match socket.read() {
                    Ok(WebSocketMessage::Text(payload)) => {
                        let message: serde_json::Value =
                            serde_json::from_str(&payload).expect("relay payload should be json");
                        let Some(kind) = message.get(0).and_then(|value| value.as_str()) else {
                            continue;
                        };

                        match kind {
                            "REQ" => {
                                let req_id = message
                                    .get(1)
                                    .and_then(|value| value.as_str())
                                    .expect("REQ should include subscription id")
                                    .to_string();
                                subscription_id = Some(req_id.clone());
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!(["EOSE", req_id.clone()]).to_string().into(),
                                    ))
                                    .expect("relay should acknowledge subscription");

                                let connect_ack = NostrEventBuilder::nostr_connect(
                                    &signer_keys,
                                    client_public_key,
                                    NostrConnectMessage::response(
                                        "connect-ack",
                                        NostrConnectResponse::new(Some(ResponseResult::Ack), None),
                                    ),
                                )
                                .expect("connect ack event should build")
                                .sign_with_keys(&signer_keys)
                                .expect("connect ack event should sign");
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!([
                                            "EVENT",
                                            subscription_id
                                                .clone()
                                                .expect("subscription should be registered"),
                                            serde_json::to_value(connect_ack)
                                                .expect("connect ack event should serialize"),
                                        ])
                                        .to_string()
                                        .into(),
                                    ))
                                    .expect("relay should send connect ack event");
                            }
                            "EVENT" => {
                                let event = message
                                    .get(1)
                                    .cloned()
                                    .expect("EVENT should include event payload");
                                let sender_pubkey = NostrPublicKey::parse(
                                    event
                                        .get("pubkey")
                                        .and_then(|value| value.as_str())
                                        .expect("event should include sender pubkey"),
                                )
                                .expect("event pubkey should parse");
                                let plaintext = nip44::decrypt(
                                    signer_keys.secret_key(),
                                    &sender_pubkey,
                                    event["content"]
                                        .as_str()
                                        .expect("event content should be a string"),
                                )
                                .expect("relay should decrypt nip46 payload");
                                let message = NostrConnectMessage::from_json(plaintext)
                                    .expect("nip46 message should parse");
                                let request_id = message.id().to_string();
                                let request = message
                                    .to_request()
                                    .expect("relay should receive a request");
                                let response = match request {
                                    NostrConnectRequest::GetPublicKey => {
                                        NostrConnectResponse::with_result(
                                            ResponseResult::GetPublicKey(user_public_key),
                                        )
                                    }
                                    _ => NostrConnectResponse::with_error(
                                        "unsupported test client pairing request",
                                    ),
                                };
                                let response_event = NostrEventBuilder::nostr_connect(
                                    &signer_keys,
                                    sender_pubkey,
                                    NostrConnectMessage::response(request_id, response),
                                )
                                .expect("relay response event should build")
                                .sign_with_keys(&signer_keys)
                                .expect("relay response event should sign");
                                let event_id = event
                                    .get("id")
                                    .and_then(|value| value.as_str())
                                    .expect("event should include id");
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!(["OK", event_id, true, ""]).to_string().into(),
                                    ))
                                    .expect("relay should ack client EVENT");
                                socket
                                    .send(WebSocketMessage::Text(
                                        json!([
                                            "EVENT",
                                            subscription_id
                                                .clone()
                                                .expect("subscription should be registered"),
                                            serde_json::to_value(response_event)
                                                .expect("response event should serialize"),
                                        ])
                                        .to_string()
                                        .into(),
                                    ))
                                    .expect("relay should forward signer response");
                                socket
                                    .close(None)
                                    .expect("relay should close the websocket");
                                break;
                            }
                            _ => {}
                        }
                    }
                    Ok(WebSocketMessage::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => panic!("test client pairing relay should not error: {error}"),
                }
            }
        });

        (format!("ws://{}", address), handle)
    }

    fn unreachable_test_relay_url() -> String {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        drop(listener);
        format!("ws://{}", address)
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
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
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
                signed_nostr_event: None,
                reply_to: None,
            });
        snapshot.shell.is_authenticated = true;
        snapshot.shell.auth_session = Some(AuthSessionSummary {
            login_method: LoginMethod::ExistingAccount,
            access: LoginAccessSummary {
                kind: LoginAccessKind::Nsec,
                label: "nsec1...runner".into(),
                pubkey: None,
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: "2026-04-18T10:00:00Z".into(),
        });
        snapshot.shell.auth_runtime = Some(AuthRuntimeSummary {
            state: AuthRuntimeState::Pending,
            login_method: LoginMethod::ExistingAccount,
            access_kind: LoginAccessKind::Nsec,
            label: "nsec1...runner".into(),
            pubkey: None,
            error: None,
            can_send_messages: false,
            send_blocked_reason: Some(
                "Account runtime is still waiting for a signer handshake.".into(),
            ),
            persisted_in_native_store: false,
            credential_persisted_in_native_store: false,
            updated_at: "2026-04-18T10:00:00Z".into(),
        });
        snapshot.shell.auth_runtime_binding = Some(AuthRuntimeBindingSummary {
            access_kind: LoginAccessKind::Bunker,
            endpoint: "wss://relay.example.com".into(),
            connection_pubkey: None,
            relay_count: 1,
            has_secret: false,
            requested_permissions: vec![],
            client_name: None,
            persisted_in_native_store: true,
            updated_at: "2026-04-18T10:00:00Z".into(),
        });
        auth_runtime_credential_store::save(
            app_handle,
            &auth_runtime_credential_store::StoredAuthRuntimeCredential {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::Nsec,
                secret_key_hex: VALID_HEX_SECRET_KEY.into(),
                pubkey: VALID_TEST_NPUB.into(),
                stored_at: "2026-04-18T10:00:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime credential store");
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
            media_upload_driver: "auto".into(),
            media_upload_endpoint: String::new(),
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
        assert!(matches!(
            reloaded
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Pending)
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
    fn bootstrap_auth_session_persists_validated_shell_state() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: false,
                auth_session: None,
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![RestorableCircleEntry {
                    name: "Archive Relay".into(),
                    relay: "wss://archive.example.local".into(),
                    circle_type: crate::domain::chat::CircleType::Paid,
                    description: "Recovered paid relay".into(),
                    archived_at: "2026-04-18T10:00:00Z".into(),
                }],
                app_preferences: AppPreferences {
                    theme: ThemePreference::Ink,
                    language: LanguagePreference::ZhCn,
                    text_size: TextSizePreference::Large,
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
                    media_upload_driver: "auto".into(),
                    media_upload_endpoint: String::new(),
                },
                active_circle_id: "".into(),
                selected_session_id: "".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let shell = bootstrap_auth_session(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::ExistingAccount,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: Some(VALID_HEX_SECRET_KEY.into()),
                },
                user_profile: UserProfile {
                    name: "  Nora Blake  ".into(),
                    handle: "@@Nora.Blake".into(),
                    initials: "nb".into(),
                    status: "Research lead".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Restore,
                    circle_id: None,
                    invite_code: None,
                    name: None,
                    relay: Some("wss://archive.example.local".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:00:00Z".into()),
            },
        )
        .expect("failed to bootstrap auth session");

        assert!(shell.is_authenticated);
        assert_eq!(shell.user_profile.name, "Nora Blake");
        assert_eq!(shell.user_profile.handle, "@nora.blake");
        assert_eq!(shell.user_profile.initials, "NB");
        assert_eq!(shell.user_profile.status, "Research lead");
        assert!(matches!(
            shell
                .auth_session
                .as_ref()
                .map(|session| &session.circle_selection_mode),
            Some(LoginCircleSelectionMode::Restore)
        ));
        assert_eq!(
            shell
                .auth_session
                .as_ref()
                .map(|session| session.logged_in_at.as_str()),
            Some("2026-04-19T08:00:00Z")
        );
        assert_eq!(
            shell
                .auth_session
                .as_ref()
                .map(|session| session.access.label.as_str()),
            Some(VALID_TEST_NPUB_LABEL)
        );
        assert!(matches!(
            shell.auth_runtime.as_ref().map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Connected)
        ));
        assert_eq!(
            shell
                .auth_runtime
                .as_ref()
                .map(|runtime| runtime.persisted_in_native_store),
            Some(true)
        );
        assert_eq!(
            shell
                .auth_runtime
                .as_ref()
                .map(|runtime| runtime.credential_persisted_in_native_store),
            Some(true)
        );
        assert!(shell.auth_runtime_binding.is_none());
        assert!(matches!(shell.app_preferences.theme, ThemePreference::Ink));
        assert!(shell.advanced_preferences.experimental_transport);
        assert_eq!(shell.restorable_circles.len(), 1);

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing stored auth runtime");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Connected));
        assert_eq!(stored_runtime.logged_in_at, "2026-04-19T08:00:00Z");
        assert_eq!(stored_runtime.label, VALID_TEST_NPUB_LABEL);

        let stored_credential = auth_runtime_credential_store::load(app_handle)
            .expect("failed to load auth runtime credential store")
            .expect("missing stored auth runtime credential");
        assert!(matches!(
            stored_credential.access_kind,
            LoginAccessKind::HexKey
        ));
        assert_eq!(stored_credential.secret_key_hex, VALID_HEX_SECRET_KEY);
        assert_eq!(stored_credential.pubkey, VALID_TEST_NPUB);
        assert_eq!(stored_credential.stored_at, "2026-04-19T08:00:00Z");

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload chat shell snapshot");
        assert!(reloaded.shell.is_authenticated);
        assert_eq!(reloaded.shell.user_profile.handle, "@nora.blake");
        assert_eq!(reloaded.shell.restorable_circles.len(), 1);
        assert_eq!(
            reloaded
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| runtime.credential_persisted_in_native_store),
            Some(true)
        );
    }

    #[test]
    fn bootstrap_auth_session_rejects_restore_selection_outside_catalog() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: false,
                auth_session: None,
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![RestorableCircleEntry {
                    name: "Archive Relay".into(),
                    relay: "wss://archive.example.local".into(),
                    circle_type: crate::domain::chat::CircleType::Paid,
                    description: "Recovered paid relay".into(),
                    archived_at: "2026-04-18T10:00:00Z".into(),
                }],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "".into(),
                selected_session_id: "".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let error = bootstrap_auth_session(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::ExistingAccount,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: Some(VALID_HEX_SECRET_KEY.into()),
                },
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Restore,
                    circle_id: None,
                    invite_code: None,
                    name: None,
                    relay: Some("wss://missing.example.local".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:00:00Z".into()),
            },
        )
        .expect_err("restore selection should reject relay outside the local catalog");

        assert!(error.contains("restore selection relay is not present"));
    }

    #[test]
    fn complete_login_accepts_public_relay_shortcut_for_custom_circle_selection() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::QuickStart,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: None,
                },
                user_profile: UserProfile {
                    name: "Sean Chen".into(),
                    handle: "@seanchen".into(),
                    initials: "SC".into(),
                    status: "Circle owner".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Custom,
                    circle_id: None,
                    invite_code: None,
                    name: Some("Community Circle".into()),
                    relay: Some("damus".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-22T09:00:00Z".into()),
            },
        )
        .expect("public relay shortcut should complete login");

        let active_circle = snapshot
            .domain
            .circles
            .iter()
            .find(|circle| circle.id == snapshot.shell.active_circle_id)
            .expect("active circle should exist");
        assert_eq!(active_circle.name, "Community Circle");
        assert_eq!(active_circle.relay, "wss://relay.damus.io");
    }

    #[test]
    fn complete_login_quick_start_persists_exportable_local_secret() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::QuickStart,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: None,
                },
                user_profile: UserProfile {
                    name: "Sean Chen".into(),
                    handle: "@seanchen".into(),
                    initials: "SC".into(),
                    status: "Circle owner".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Custom,
                    circle_id: None,
                    invite_code: None,
                    name: Some("Community Circle".into()),
                    relay: Some("damus".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-22T09:00:00Z".into()),
            },
        )
        .expect("quick start should complete login with a generated local secret");

        let auth_session = snapshot
            .shell
            .auth_session
            .as_ref()
            .expect("quick start auth session should be present");
        assert!(matches!(auth_session.login_method, LoginMethod::QuickStart));
        assert!(matches!(auth_session.access.kind, LoginAccessKind::HexKey));
        assert!(auth_session
            .access
            .pubkey
            .as_deref()
            .is_some_and(|pubkey| pubkey.starts_with("npub1")));

        let stored_credential = auth_runtime_credential_store::load(app_handle)
            .expect("failed to load auth runtime credential store")
            .expect("missing stored quick start auth runtime credential");
        assert!(matches!(
            stored_credential.access_kind,
            LoginAccessKind::HexKey
        ));
        assert_eq!(stored_credential.stored_at, "2026-04-22T09:00:00Z");
        assert_eq!(
            Some(stored_credential.pubkey.as_str()),
            auth_session.access.pubkey.as_deref()
        );

        let secret_summary = load_local_account_secret_summary(app_handle)
            .expect("failed to load local account secret summary")
            .expect("missing quick start local account secret summary");
        assert!(matches!(
            secret_summary.login_method,
            LoginMethod::QuickStart
        ));
        assert!(matches!(
            secret_summary.access_kind,
            LoginAccessKind::HexKey
        ));
        assert_eq!(secret_summary.pubkey, stored_credential.pubkey);
        assert_eq!(secret_summary.hex_key, stored_credential.secret_key_hex);
        assert!(secret_summary.nsec.starts_with("nsec1"));

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload chat shell snapshot");
        assert!(reloaded
            .shell
            .auth_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.credential_persisted_in_native_store));
        assert!(reloaded
            .shell
            .auth_runtime
            .as_ref()
            .is_some_and(|runtime| matches!(runtime.state, AuthRuntimeState::Connected)));
    }

    #[test]
    fn bootstrap_then_complete_login_reuses_quick_start_secret() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let bootstrapped_shell = bootstrap_auth_session(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::QuickStart,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: None,
                },
                user_profile: UserProfile {
                    name: "Sean Chen".into(),
                    handle: "@seanchen".into(),
                    initials: "SC".into(),
                    status: "Circle owner".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Invite,
                    circle_id: None,
                    invite_code: None,
                    name: None,
                    relay: None,
                    relays: None,
                },
                logged_in_at: Some("2026-04-22T09:00:00Z".into()),
            },
        )
        .expect("quick start bootstrap should succeed");

        let first_auth_session = bootstrapped_shell
            .auth_session
            .as_ref()
            .expect("bootstrapped auth session should be present");
        assert!(bootstrapped_shell
            .auth_runtime
            .as_ref()
            .is_some_and(|runtime| matches!(runtime.state, AuthRuntimeState::Connected)));
        let first_credential = auth_runtime_credential_store::load(app_handle)
            .expect("failed to load first quick start credential")
            .expect("missing first quick start credential");

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::QuickStart,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: None,
                },
                user_profile: UserProfile {
                    name: "Sean Chen".into(),
                    handle: "@seanchen".into(),
                    initials: "SC".into(),
                    status: "Circle owner".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Custom,
                    circle_id: None,
                    invite_code: None,
                    name: Some("Community Circle".into()),
                    relay: Some("damus".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-22T09:05:00Z".into()),
            },
        )
        .expect("quick start completion should reuse generated secret");

        let second_credential = auth_runtime_credential_store::load(app_handle)
            .expect("failed to load second quick start credential")
            .expect("missing second quick start credential");
        let second_auth_session = snapshot
            .shell
            .auth_session
            .as_ref()
            .expect("completed auth session should be present");

        assert_eq!(
            second_credential.secret_key_hex,
            first_credential.secret_key_hex
        );
        assert_eq!(
            second_auth_session.logged_in_at,
            first_auth_session.logged_in_at
        );
        assert!(matches!(
            second_auth_session.access.kind,
            LoginAccessKind::HexKey
        ));
        assert_eq!(
            second_auth_session.access.pubkey.as_deref(),
            Some(second_credential.pubkey.as_str())
        );
    }

    #[test]
    fn bootstrap_auth_session_marks_npub_import_as_failed_auth_runtime() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let shell = bootstrap_auth_session(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::ExistingAccount,
                access: LoginAccessInput {
                    kind: LoginAccessKind::Npub,
                    value: Some(VALID_TEST_NPUB.into()),
                },
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Existing,
                    circle_id: Some("main-circle".into()),
                    invite_code: None,
                    name: None,
                    relay: None,
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:30:00Z".into()),
            },
        )
        .expect("failed to bootstrap npub auth session");

        assert!(matches!(
            shell.auth_runtime.as_ref().map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Failed)
        ));
        assert_eq!(
            shell
                .auth_runtime
                .as_ref()
                .and_then(|runtime| runtime.error.as_deref()),
            Some("Read-only npub import cannot sign messages yet.")
        );
        assert!(shell.auth_runtime_binding.is_none());
    }

    #[test]
    fn bootstrap_auth_session_persists_remote_auth_runtime_binding() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let pubkey = valid_binding_pubkey_hex();

        let shell = bootstrap_auth_session(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::Signer,
                access: LoginAccessInput {
                    kind: LoginAccessKind::NostrConnect,
                    value: Some(format!(
                        "nostrconnect://{pubkey}?relay=wss://relay.example.com&secret=shared-secret&perms=sign_event,get_public_key&name=Desk%20Client"
                    )),
                },
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Existing,
                    circle_id: Some("main-circle".into()),
                    invite_code: None,
                    name: None,
                    relay: None,
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:30:00Z".into()),
            },
        )
        .expect("failed to bootstrap signer auth session");

        let binding = shell
            .auth_runtime_binding
            .expect("missing auth runtime binding summary");
        assert!(matches!(binding.access_kind, LoginAccessKind::NostrConnect));
        assert_eq!(binding.endpoint, "wss://relay.example.com");
        assert_eq!(binding.connection_pubkey.as_deref(), Some(pubkey.as_str()));
        assert_eq!(binding.relay_count, 1);
        assert!(binding.has_secret);
        assert_eq!(
            binding.requested_permissions,
            vec!["sign_event", "get_public_key"]
        );
        assert_eq!(binding.client_name.as_deref(), Some("Desk Client"));
        assert!(binding.persisted_in_native_store);

        let stored_binding = auth_runtime_binding_store::load(app_handle)
            .expect("failed to load stored auth runtime binding")
            .expect("missing stored auth runtime binding");
        assert!(matches!(
            stored_binding.access_kind,
            LoginAccessKind::NostrConnect
        ));
        assert_eq!(
            stored_binding.value,
            format!(
                "nostrconnect://{pubkey}?relay=wss://relay.example.com&secret=shared-secret&perms=sign_event,get_public_key&name=Desk%20Client"
            )
        );
    }

    #[test]
    fn complete_login_selects_existing_circle_and_first_visible_session() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::QuickStart,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: None,
                },
                user_profile: UserProfile {
                    name: "Sean Chen".into(),
                    handle: "@seanchen".into(),
                    initials: "SC".into(),
                    status: "Circle owner".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Existing,
                    circle_id: Some("main-circle".into()),
                    invite_code: None,
                    name: None,
                    relay: None,
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:00:00Z".into()),
            },
        )
        .expect("failed to complete login");

        assert!(snapshot.shell.is_authenticated);
        assert_eq!(snapshot.shell.active_circle_id, "main-circle");
        assert_eq!(snapshot.shell.selected_session_id, "alice");
        assert_eq!(snapshot.shell.user_profile.handle, "@seanchen");
        assert!(matches!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Connected)
        ));
        assert!(snapshot.domain.message_store.contains_key("alice"));
    }

    #[test]
    fn complete_login_restores_circle_and_removes_catalog_entry() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: false,
                auth_session: None,
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![RestorableCircleEntry {
                    name: "Archive Relay".into(),
                    relay: "wss://archive.example.local".into(),
                    circle_type: crate::domain::chat::CircleType::Paid,
                    description: "Recovered paid relay".into(),
                    archived_at: "2026-04-18T10:00:00Z".into(),
                }],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "".into(),
                selected_session_id: "".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::ExistingAccount,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: Some(VALID_HEX_SECRET_KEY.into()),
                },
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Restore,
                    circle_id: None,
                    invite_code: None,
                    name: None,
                    relay: Some("wss://archive.example.local".into()),
                    relays: None,
                },
                logged_in_at: Some("2026-04-19T08:00:00Z".into()),
            },
        )
        .expect("failed to complete login with restored circle");

        let restored_circle = snapshot
            .domain
            .circles
            .iter()
            .find(|circle| circle.relay == "wss://archive.example.local")
            .expect("missing restored circle");
        assert_eq!(snapshot.shell.active_circle_id, restored_circle.id);
        assert!(snapshot.shell.selected_session_id.is_empty());
        assert!(snapshot.shell.restorable_circles.is_empty());
        assert!(matches!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Connected)
        ));
    }

    #[test]
    fn complete_login_restores_multiple_circles_and_keeps_first_restored_active() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: false,
                auth_session: None,
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![
                    RestorableCircleEntry {
                        name: "Archive Relay".into(),
                        relay: "wss://archive.example.local".into(),
                        circle_type: crate::domain::chat::CircleType::Paid,
                        description: "Recovered paid relay".into(),
                        archived_at: "2026-04-18T10:00:00Z".into(),
                    },
                    RestorableCircleEntry {
                        name: "Friends Relay".into(),
                        relay: "wss://friends.example.local".into(),
                        circle_type: crate::domain::chat::CircleType::Custom,
                        description: "Recovered private relay".into(),
                        archived_at: "2026-04-18T12:00:00Z".into(),
                    },
                ],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "".into(),
                selected_session_id: "".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let snapshot = complete_login(
            app_handle,
            LoginCompletionInput {
                method: LoginMethod::ExistingAccount,
                access: LoginAccessInput {
                    kind: LoginAccessKind::HexKey,
                    value: Some(VALID_HEX_SECRET_KEY.into()),
                },
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                circle_selection: LoginCircleSelectionInput {
                    mode: LoginCircleSelectionMode::Restore,
                    circle_id: None,
                    invite_code: None,
                    name: None,
                    relay: Some("wss://archive.example.local".into()),
                    relays: Some(vec![
                        "wss://archive.example.local".into(),
                        "wss://friends.example.local".into(),
                    ]),
                },
                logged_in_at: Some("2026-04-19T08:00:00Z".into()),
            },
        )
        .expect("failed to complete login with restored circles");

        let archive_circle = snapshot
            .domain
            .circles
            .iter()
            .find(|circle| circle.relay == "wss://archive.example.local")
            .expect("missing archive restored circle");
        let friends_circle = snapshot
            .domain
            .circles
            .iter()
            .find(|circle| circle.relay == "wss://friends.example.local")
            .expect("missing friends restored circle");
        assert_eq!(snapshot.shell.active_circle_id, archive_circle.id);
        assert_ne!(archive_circle.id, friends_circle.id);
        assert!(snapshot.shell.selected_session_id.is_empty());
        assert!(snapshot.shell.restorable_circles.is_empty());
        assert!(matches!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Connected)
        ));
    }

    #[test]
    fn logout_chat_session_clears_auth_state_but_preserves_local_preferences() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::ExistingAccount,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::Nsec,
                        label: "nsec1sup...hell".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Restore,
                    logged_in_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::ExistingAccount,
                    access_kind: LoginAccessKind::Nsec,
                    label: "nsec1sup...hell".into(),
                    pubkey: None,
                    error: None,
                    can_send_messages: false,
                    send_blocked_reason: Some(
                        "Account runtime is still waiting for a signer handshake.".into(),
                    ),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::Bunker,
                    endpoint: "wss://archive.example.local".into(),
                    connection_pubkey: None,
                    relay_count: 1,
                    has_secret: false,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: true,
                    updated_at: "2026-04-19T08:00:00Z".into(),
                }),
                user_profile: UserProfile {
                    name: "Nora Blake".into(),
                    handle: "@nora".into(),
                    initials: "NB".into(),
                    status: "Research".into(),
                },
                restorable_circles: vec![RestorableCircleEntry {
                    name: "Archive Relay".into(),
                    relay: "wss://archive.example.local".into(),
                    circle_type: crate::domain::chat::CircleType::Paid,
                    description: "Recovered paid relay".into(),
                    archived_at: "2026-04-18T10:00:00Z".into(),
                }],
                app_preferences: AppPreferences {
                    theme: ThemePreference::Ink,
                    language: LanguagePreference::ZhCn,
                    text_size: TextSizePreference::Large,
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
                    media_upload_driver: "auto".into(),
                    media_upload_endpoint: String::new(),
                },
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        let pubkey = valid_binding_pubkey_hex();
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!("bunker://{pubkey}?relay=wss://archive.example.local"),
                stored_at: "2026-04-19T08:00:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime binding store");
        auth_runtime_credential_store::save(
            app_handle,
            &auth_runtime_credential_store::StoredAuthRuntimeCredential {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::Nsec,
                secret_key_hex: VALID_HEX_SECRET_KEY.into(),
                pubkey: VALID_TEST_NPUB.into(),
                stored_at: "2026-04-19T08:00:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime credential store");
        auth_runtime_client_store::save(
            app_handle,
            &auth_runtime_client_store::StoredAuthRuntimeClient {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                public_key: "3b7d3f7041e4675456f5f3d32d8f7d91a196e4d9576cb0b9f8430d48cae2f0d6"
                    .into(),
                secret_key_hex: "4444444444444444444444444444444444444444444444444444444444444444"
                    .into(),
                stored_at: "2026-04-19T08:00:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime client store");
        auth_runtime_state_store::save(
            app_handle,
            &auth_runtime_state_store::StoredAuthRuntimeState {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::Nsec,
                label: "nsec1sup...hell".into(),
                logged_in_at: "2026-04-19T08:00:00Z".into(),
                state: AuthRuntimeState::Pending,
                pubkey: None,
                error: Some("waiting for signer".into()),
                updated_at: "2026-04-19T08:15:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime state store");

        let snapshot = logout_chat_session(app_handle).expect("failed to logout chat session");

        assert!(!snapshot.shell.is_authenticated);
        assert!(snapshot.shell.auth_session.is_none());
        assert!(snapshot.shell.auth_runtime.is_none());
        assert!(snapshot.shell.auth_runtime_binding.is_none());
        assert_eq!(
            snapshot.shell.user_profile.handle,
            crate::domain::chat::default_user_profile().handle
        );
        assert_eq!(snapshot.shell.restorable_circles.len(), 1);
        assert!(matches!(
            snapshot.shell.app_preferences.theme,
            ThemePreference::Ink
        ));
        assert!(snapshot.shell.advanced_preferences.experimental_transport);
        assert!(snapshot.shell.active_circle_id.is_empty());
        assert!(snapshot.shell.selected_session_id.is_empty());
        assert!(snapshot.domain.circles.is_empty());
        assert!(snapshot.domain.sessions.is_empty());

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload chat shell snapshot");
        assert!(!reloaded.shell.is_authenticated);
        assert!(reloaded.shell.auth_session.is_none());
        assert!(reloaded.shell.auth_runtime_binding.is_none());
        assert_eq!(reloaded.shell.restorable_circles.len(), 1);
        assert!(matches!(
            reloaded.shell.app_preferences.language,
            LanguagePreference::ZhCn
        ));
        assert!(reloaded.domain.circles.is_empty());
        assert!(reloaded.domain.sessions.is_empty());
        assert!(auth_runtime_binding_store::load(app_handle)
            .expect("failed to load auth runtime binding store")
            .is_none());
        assert!(auth_runtime_credential_store::load(app_handle)
            .expect("failed to load auth runtime credential store")
            .is_none());
        assert!(auth_runtime_client_store::load(app_handle)
            .expect("failed to load auth runtime client store")
            .is_none());
        assert!(auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .is_none());
    }

    #[test]
    fn update_auth_runtime_promotes_pending_signer_to_connected_and_persists() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::NostrConnect,
                        label: "nostrconnect://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::NostrConnect,
                    label: "nostrconnect://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::NostrConnect,
                    endpoint: "wss://relay.example.com".into(),
                    connection_pubkey: None,
                    relay_count: 1,
                    has_secret: false,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: true,
                    updated_at: "2026-04-19T08:00:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        seed_auth_runtime_binding(
            app_handle,
            LoginMethod::Signer,
            LoginAccessKind::NostrConnect,
            "2026-04-19T08:00:00Z",
        );

        let shell = update_auth_runtime(
            app_handle,
            UpdateAuthRuntimeInput {
                state: AuthRuntimeState::Connected,
                error: None,
                updated_at: Some("2026-04-19T09:30:00Z".into()),
                label: Some("Remote Signer A".into()),
            },
        )
        .expect("failed to update auth runtime");

        let runtime = shell.auth_runtime.expect("missing updated auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert_eq!(runtime.label, "Remote Signer A");
        assert!(runtime.error.is_none());
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime.persisted_in_native_store);
        assert_eq!(runtime.updated_at, "2026-04-19T09:30:00Z");

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing stored auth runtime state");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Connected));
        assert_eq!(stored_runtime.label, "Remote Signer A");
        assert_eq!(stored_runtime.updated_at, "2026-04-19T09:30:00Z");

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload chat shell snapshot");
        let reloaded_runtime = reloaded
            .shell
            .auth_runtime
            .expect("missing reloaded updated auth runtime");
        assert!(matches!(
            reloaded_runtime.state,
            AuthRuntimeState::Connected
        ));
        assert_eq!(reloaded_runtime.label, "Remote Signer A");
        assert!(reloaded_runtime.error.is_none());
    }

    #[test]
    fn update_auth_runtime_rejects_invalid_quick_start_transition() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::QuickStart,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::LocalProfile,
                        label: "Quick Start".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::LocalProfile,
                    login_method: LoginMethod::QuickStart,
                    access_kind: LoginAccessKind::LocalProfile,
                    label: "Quick Start".into(),
                    pubkey: None,
                    error: None,
                    can_send_messages: true,
                    send_blocked_reason: None,
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T08:00:00Z".into(),
                }),
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        let error = update_auth_runtime(
            app_handle,
            UpdateAuthRuntimeInput {
                state: AuthRuntimeState::Failed,
                error: Some("force failure".into()),
                updated_at: Some("2026-04-19T09:30:00Z".into()),
                label: None,
            },
        )
        .expect_err("quick start runtime should reject non-local profile state");

        assert!(error.contains("quick start auth runtime is fixed"));
    }

    #[test]
    fn load_chat_shell_snapshot_accepts_legacy_persisted_shell_state_shape() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let persisted_state = PersistedShellState {
            is_authenticated: true,
            auth_session: None,
            auth_runtime: None,
            auth_runtime_binding: None,
            user_profile: UserProfile {
                name: "Legacy User".into(),
                handle: "@legacy".into(),
                initials: "LU".into(),
                status: "Legacy".into(),
            },
            restorable_circles: vec![],
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
                media_upload_driver: "auto".into(),
                media_upload_endpoint: String::new(),
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
    fn load_chat_shell_snapshot_backfills_auth_runtime_from_auth_session_when_missing() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::ExistingAccount,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::Npub,
                        label: VALID_TEST_NPUB_LABEL.into(),
                        pubkey: Some(VALID_TEST_NPUB.into()),
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:00:00Z".into(),
                }),
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
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

        assert!(matches!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Failed)
        ));
        assert_eq!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| runtime.persisted_in_native_store),
            Some(true)
        );
        assert_eq!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .map(|runtime| runtime.credential_persisted_in_native_store),
            Some(false)
        );
        assert_eq!(
            snapshot
                .shell
                .auth_runtime
                .as_ref()
                .and_then(|runtime| runtime.error.as_deref()),
            Some("Read-only npub import cannot sign messages yet.")
        );

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing bootstrapped auth runtime");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Failed));
        assert_eq!(stored_runtime.logged_in_at, "2026-04-19T11:00:00Z");
        assert_eq!(
            stored_runtime.error.as_deref(),
            Some("Read-only npub import cannot sign messages yet.")
        );
    }

    #[test]
    fn load_chat_shell_snapshot_generates_local_secret_for_legacy_quick_start_session() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::QuickStart,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::LocalProfile,
                        label: "Quick Start".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Invite,
                    logged_in_at: "2026-04-23T10:00:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::LocalProfile,
                    login_method: LoginMethod::QuickStart,
                    access_kind: LoginAccessKind::LocalProfile,
                    label: "Quick Start".into(),
                    pubkey: None,
                    error: None,
                    can_send_messages: true,
                    send_blocked_reason: None,
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-23T10:00:00Z".into(),
                }),
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed legacy quick start shell state");

        let snapshot = load_chat_shell_snapshot(app_handle)
            .expect("failed to load migrated chat shell snapshot");

        let auth_session = snapshot
            .shell
            .auth_session
            .as_ref()
            .expect("migrated quick start auth session should be present");
        assert!(matches!(auth_session.login_method, LoginMethod::QuickStart));
        assert!(matches!(
            auth_session.access.kind,
            LoginAccessKind::LocalProfile
        ));
        assert!(auth_session
            .access
            .pubkey
            .as_deref()
            .is_some_and(|pubkey| pubkey.starts_with("npub1")));
        assert!(snapshot
            .shell
            .auth_runtime
            .as_ref()
            .is_some_and(|runtime| matches!(runtime.state, AuthRuntimeState::LocalProfile)));
        assert!(snapshot
            .shell
            .auth_runtime
            .as_ref()
            .is_some_and(|runtime| runtime.credential_persisted_in_native_store));

        let stored_credential = auth_runtime_credential_store::load(app_handle)
            .expect("failed to load migrated quick start credential")
            .expect("missing migrated quick start credential");
        assert!(matches!(
            stored_credential.access_kind,
            LoginAccessKind::LocalProfile
        ));
        assert_eq!(stored_credential.stored_at, "2026-04-23T10:00:00Z");
        assert_eq!(
            auth_session.access.pubkey.as_deref(),
            Some(stored_credential.pubkey.as_str())
        );
    }

    #[test]
    fn load_chat_shell_snapshot_marks_local_secret_runtime_failed_when_native_credential_is_missing(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::ExistingAccount,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::HexKey,
                        label: VALID_TEST_NPUB_LABEL.into(),
                        pubkey: Some(VALID_TEST_NPUB.into()),
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:30:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Connected,
                    login_method: LoginMethod::ExistingAccount,
                    access_kind: LoginAccessKind::HexKey,
                    label: VALID_TEST_NPUB_LABEL.into(),
                    pubkey: Some(VALID_TEST_NPUB.into()),
                    error: None,
                    can_send_messages: true,
                    send_blocked_reason: None,
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:30:00Z".into(),
                }),
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        auth_runtime_state_store::save(
            app_handle,
            &auth_runtime_state_store::StoredAuthRuntimeState {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: VALID_TEST_NPUB_LABEL.into(),
                logged_in_at: "2026-04-19T11:30:00Z".into(),
                state: AuthRuntimeState::Connected,
                pubkey: Some(VALID_TEST_NPUB.into()),
                error: None,
                updated_at: "2026-04-19T11:30:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");
        let runtime = snapshot
            .shell
            .auth_runtime
            .expect("missing auth runtime summary");

        assert!(matches!(runtime.state, AuthRuntimeState::Failed));
        assert_eq!(
            runtime.error.as_deref(),
            Some("Validated local account key is missing from native credential store.")
        );
        assert!(!runtime.can_send_messages);
        assert_eq!(
            runtime.send_blocked_reason.as_deref(),
            Some("Validated local account key is missing from native credential store.")
        );
        assert!(runtime.persisted_in_native_store);
        assert!(!runtime.credential_persisted_in_native_store);

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing corrected auth runtime state");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Failed));
        assert_eq!(
            stored_runtime.error.as_deref(),
            Some("Validated local account key is missing from native credential store.")
        );
    }

    #[test]
    fn load_chat_shell_snapshot_marks_remote_signer_runtime_failed_when_native_binding_is_missing()
    {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::NostrConnect,
                        label: "nostrconnect://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:40:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Connected,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::NostrConnect,
                    label: "Remote Signer A".into(),
                    pubkey: None,
                    error: None,
                    can_send_messages: true,
                    send_blocked_reason: None,
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:42:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::NostrConnect,
                    endpoint: "wss://relay.example.com".into(),
                    connection_pubkey: None,
                    relay_count: 1,
                    has_secret: false,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:40:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        auth_runtime_state_store::save(
            app_handle,
            &auth_runtime_state_store::StoredAuthRuntimeState {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                label: "Remote Signer A".into(),
                logged_in_at: "2026-04-19T11:40:00Z".into(),
                state: AuthRuntimeState::Connected,
                pubkey: None,
                error: None,
                updated_at: "2026-04-19T11:42:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");
        let runtime = snapshot
            .shell
            .auth_runtime
            .expect("missing auth runtime summary");

        assert!(matches!(runtime.state, AuthRuntimeState::Failed));
        assert_eq!(
            runtime.error.as_deref(),
            Some("Stored remote signer binding is missing from native binding store.")
        );
        assert!(!runtime.can_send_messages);
        assert_eq!(
            runtime.send_blocked_reason.as_deref(),
            Some("Stored remote signer binding is missing from native binding store.")
        );
        assert!(runtime.persisted_in_native_store);

        let binding = snapshot
            .shell
            .auth_runtime_binding
            .expect("missing auth runtime binding summary");
        assert_eq!(binding.endpoint, "wss://relay.example.com");
        assert!(!binding.persisted_in_native_store);

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing corrected auth runtime state");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Failed));
        assert_eq!(
            stored_runtime.error.as_deref(),
            Some("Stored remote signer binding is missing from native binding store.")
        );
    }

    #[test]
    fn load_chat_shell_snapshot_prefers_native_auth_runtime_state_for_current_session() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::NostrConnect,
                        label: "nostrconnect://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:45:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::NostrConnect,
                    label: "nostrconnect://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:45:00Z".into(),
                }),
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        seed_auth_runtime_binding(
            app_handle,
            LoginMethod::Signer,
            LoginAccessKind::NostrConnect,
            "2026-04-19T11:45:00Z",
        );
        auth_runtime_state_store::save(
            app_handle,
            &auth_runtime_state_store::StoredAuthRuntimeState {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                label: "Remote Signer A".into(),
                logged_in_at: "2026-04-19T11:45:00Z".into(),
                state: AuthRuntimeState::Connected,
                pubkey: None,
                error: None,
                updated_at: "2026-04-19T11:50:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime state store");

        let snapshot =
            load_chat_shell_snapshot(app_handle).expect("failed to load chat shell snapshot");

        let runtime = snapshot
            .shell
            .auth_runtime
            .expect("missing auth runtime summary");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert_eq!(runtime.label, "Remote Signer A");
        assert!(runtime.error.is_none());
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime.persisted_in_native_store);
        assert_eq!(runtime.updated_at, "2026-04-19T11:50:00Z");
    }

    #[test]
    fn load_auth_runtime_client_uri_returns_standard_nostrconnect_client_uri_for_remote_signer() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored_at = "2026-04-19T11:56:00Z";
        let signer_pubkey = valid_binding_pubkey_hex();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::Bunker,
                        label: "bunker://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: stored_at.into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::Bunker,
                    label: "bunker://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: stored_at.into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::Bunker,
                    endpoint: "wss://relay.example.com".into(),
                    connection_pubkey: Some(signer_pubkey),
                    relay_count: 1,
                    has_secret: false,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: true,
                    updated_at: stored_at.into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        seed_auth_runtime_binding(
            app_handle,
            LoginMethod::Signer,
            LoginAccessKind::Bunker,
            stored_at,
        );

        let summary = load_auth_runtime_client_uri(app_handle)
            .expect("failed to load auth runtime client uri")
            .expect("missing auth runtime client uri summary");
        let repeated_summary = load_auth_runtime_client_uri(app_handle)
            .expect("failed to reload auth runtime client uri")
            .expect("missing reloaded auth runtime client uri summary");
        let stored_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to load stored auth runtime client")
            .expect("missing stored auth runtime client");

        assert_eq!(summary.public_key, stored_client.public_key);
        assert_eq!(summary.public_key, repeated_summary.public_key);
        assert_eq!(summary.uri, repeated_summary.uri);
        assert_eq!(summary.client_name, "XChat Desktop");
        assert_eq!(summary.relay_count, 1);
        assert_eq!(summary.stored_at, stored_at);
        assert_eq!(stored_client.stored_at, stored_at);
        assert_eq!(summary.relays.len(), 1);
        assert!(summary.uri.starts_with("nostrconnect://"));
        assert!(summary.relays[0].starts_with("wss://relay.example.com"));

        let parsed = Url::parse(&summary.uri).expect("client uri should parse");
        assert_eq!(parsed.scheme(), "nostrconnect");
        assert_eq!(parsed.host_str(), Some(stored_client.public_key.as_str()));

        let relay_values = parsed
            .query_pairs()
            .filter_map(|(key, value)| {
                if key == "relay" {
                    Some(value.into_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(relay_values, vec!["wss://relay.example.com".to_string()]);

        let metadata = parsed
            .query_pairs()
            .find_map(|(key, value)| {
                if key == "metadata" {
                    Some(value.into_owned())
                } else {
                    None
                }
            })
            .expect("client uri should include metadata");
        let metadata: serde_json::Value =
            serde_json::from_str(&metadata).expect("metadata should be valid json");
        assert_eq!(
            metadata.get("name").and_then(|value| value.as_str()),
            Some("XChat Desktop")
        );
    }

    #[test]
    fn load_pending_auth_runtime_client_uri_returns_standard_nostrconnect_client_uri() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let summary = load_pending_auth_runtime_client_uri(app_handle)
            .expect("failed to load pending auth runtime client uri");
        let repeated_summary = load_pending_auth_runtime_client_uri(app_handle)
            .expect("failed to reload pending auth runtime client uri");
        let stored_client = pending_auth_runtime_client_store::load(app_handle)
            .expect("failed to load pending auth runtime client")
            .expect("missing pending auth runtime client");

        assert_eq!(summary.public_key, stored_client.public_key);
        assert_eq!(summary.public_key, repeated_summary.public_key);
        assert_eq!(summary.uri, repeated_summary.uri);
        assert_eq!(summary.client_name, "XChat Desktop");
        assert_eq!(summary.stored_at, stored_client.stored_at);
        assert_eq!(summary.relay_count as usize, stored_client.relays.len());
        assert_eq!(summary.relays, stored_client.relays);

        let parsed = Url::parse(&summary.uri).expect("pending client uri should parse");
        assert_eq!(parsed.scheme(), "nostrconnect");
        assert_eq!(parsed.host_str(), Some(stored_client.public_key.as_str()));
    }

    #[test]
    fn await_pending_auth_runtime_client_pairing_resolves_and_persists_bunker_uri() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let client_keys = NostrKeys::generate();
        let client_public_key = client_keys.public_key().to_hex();
        let (relay_url, relay_handle) =
            spawn_client_pairing_relay_server(client_public_key.clone());

        pending_auth_runtime_client_store::save(
            app_handle,
            &pending_auth_runtime_client_store::StoredPendingAuthRuntimeClient {
                public_key: client_public_key.clone(),
                secret_key_hex: client_keys.secret_key().to_secret_hex(),
                relays: vec![relay_url.clone()],
                client_name: "XChat Desktop".into(),
                stored_at: "2026-04-22T09:44:00Z".into(),
                paired_bunker_uri: None,
            },
        )
        .expect("failed to seed pending auth runtime client store");

        let paired_bunker_uri = await_pending_auth_runtime_client_pairing(app_handle)
            .expect("failed to await pending auth runtime client pairing");
        relay_handle
            .join()
            .expect("client pairing relay thread should join");

        assert!(
            paired_bunker_uri.starts_with(&format!("bunker://{}", bunker_signer_public_key_hex()))
        );
        assert!(paired_bunker_uri.contains(&format!("relay={relay_url}")));

        let stored_client = pending_auth_runtime_client_store::load(app_handle)
            .expect("failed to reload pending auth runtime client")
            .expect("missing pending auth runtime client after pairing");
        assert_eq!(
            stored_client.paired_bunker_uri.as_deref(),
            Some(paired_bunker_uri.as_str())
        );
    }

    #[test]
    fn bootstrap_auth_session_claims_paired_pending_auth_runtime_client() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let paired_bunker_uri = format!(
            "bunker://{}?relay=wss://relay.example.com",
            valid_binding_pubkey_hex()
        );
        let pending_client =
            seed_pending_auth_runtime_client(app_handle, Some(paired_bunker_uri.clone()));

        let shell = bootstrap_auth_session(
            app_handle,
            signer_login_input(&paired_bunker_uri, "2026-04-22T10:01:00Z"),
        )
        .expect("failed to bootstrap auth session");

        let stored_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to load claimed auth runtime client")
            .expect("missing claimed auth runtime client");
        assert_eq!(stored_client.public_key, pending_client.public_key);
        assert_eq!(stored_client.secret_key_hex, pending_client.secret_key_hex);
        assert_eq!(stored_client.stored_at, "2026-04-22T10:01:00Z");
        assert!(pending_auth_runtime_client_store::load(app_handle)
            .expect("failed to load pending auth runtime client after bootstrap")
            .is_none());

        let stored_binding = auth_runtime_binding_store::load(app_handle)
            .expect("failed to load persisted auth runtime binding")
            .expect("missing persisted auth runtime binding");
        assert_eq!(stored_binding.value, paired_bunker_uri);

        let auth_session = shell
            .auth_session
            .expect("missing bootstrapped auth session");
        assert!(matches!(auth_session.access.kind, LoginAccessKind::Bunker));
    }

    #[test]
    fn sync_auth_runtime_promotes_pending_bunker_signer_when_handshake_succeeds() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (relay_url, relay_handle) = spawn_bunker_handshake_relay_server();
        let signer_pubkey = bunker_signer_public_key_hex();
        let user_npub = bunker_user_public_key_npub();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::Bunker,
                        label: "bunker://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:46:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::Bunker,
                    label: "bunker://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:46:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::Bunker,
                    endpoint: relay_url.clone(),
                    connection_pubkey: Some(signer_pubkey.clone()),
                    relay_count: 1,
                    has_secret: true,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:46:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: "2026-04-19T11:46:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime binding store");

        let shell = sync_auth_runtime(app_handle).expect("failed to sync auth runtime");
        let runtime = shell.auth_runtime.expect("missing synced auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert!(runtime.error.is_none());
        assert_eq!(runtime.pubkey.as_deref(), Some(user_npub.as_str()));
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime.persisted_in_native_store);

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing stored auth runtime state");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Connected));
        assert_eq!(stored_runtime.pubkey.as_deref(), Some(user_npub.as_str()));
        assert!(stored_runtime.error.is_none());

        let stored_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to load auth runtime client store")
            .expect("missing stored auth runtime client");
        assert!(matches!(stored_client.access_kind, LoginAccessKind::Bunker));
        assert_eq!(stored_client.stored_at, "2026-04-19T11:46:00Z");

        let reloaded =
            load_chat_shell_snapshot(app_handle).expect("failed to reload synced auth runtime");
        assert_eq!(
            reloaded
                .shell
                .auth_runtime
                .as_ref()
                .and_then(|runtime| runtime.pubkey.as_deref()),
            Some(user_npub.as_str())
        );

        relay_handle
            .join()
            .expect("test relay thread should finish cleanly");
    }

    #[test]
    fn sync_auth_runtime_promotes_pending_nostrconnect_signer_when_handshake_succeeds() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (relay_url, relay_handle) = spawn_bunker_handshake_relay_server();
        let signer_pubkey = bunker_signer_public_key_hex();
        let user_npub = bunker_user_public_key_npub();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::NostrConnect,
                        label: "nostrconnect://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:47:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::NostrConnect,
                    label: "nostrconnect://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:47:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::NostrConnect,
                    endpoint: relay_url.clone(),
                    connection_pubkey: Some(signer_pubkey.clone()),
                    relay_count: 1,
                    has_secret: true,
                    requested_permissions: vec!["sign_event".into()],
                    client_name: Some("Desk Client".into()),
                    persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:47:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                value: format!(
                    "nostrconnect://{signer_pubkey}?relay={relay_url}&secret=shared-secret&perms=sign_event&name=Desk%20Client"
                ),
                stored_at: "2026-04-19T11:47:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime binding store");

        let shell = sync_auth_runtime(app_handle).expect("failed to sync auth runtime");
        let runtime = shell.auth_runtime.expect("missing synced auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert!(runtime.error.is_none());
        assert_eq!(runtime.pubkey.as_deref(), Some(user_npub.as_str()));
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime.persisted_in_native_store);

        let stored_runtime = auth_runtime_state_store::load(app_handle)
            .expect("failed to load auth runtime state store")
            .expect("missing stored auth runtime state");
        assert!(matches!(stored_runtime.state, AuthRuntimeState::Connected));
        assert_eq!(stored_runtime.pubkey.as_deref(), Some(user_npub.as_str()));
        assert!(stored_runtime.error.is_none());

        let stored_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to load auth runtime client store")
            .expect("missing stored auth runtime client");
        assert!(matches!(
            stored_client.access_kind,
            LoginAccessKind::NostrConnect
        ));

        relay_handle
            .join()
            .expect("test relay thread should finish cleanly");
    }

    #[test]
    fn sync_auth_runtime_reuses_persisted_bunker_client_keys_across_retries() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let relay_url = unreachable_test_relay_url();
        let signer_pubkey = bunker_signer_public_key_hex();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::Bunker,
                        label: "bunker://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:48:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::Bunker,
                    label: "bunker://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:48:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::Bunker,
                    endpoint: relay_url.clone(),
                    connection_pubkey: Some(signer_pubkey.clone()),
                    relay_count: 1,
                    has_secret: true,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:48:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: "2026-04-19T11:48:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime binding store");

        let first_shell = sync_auth_runtime(app_handle).expect("failed to sync auth runtime");
        assert!(matches!(
            first_shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Failed)
        ));
        let first_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to load auth runtime client store")
            .expect("missing stored auth runtime client");

        let pending_shell = update_auth_runtime(
            app_handle,
            UpdateAuthRuntimeInput {
                state: AuthRuntimeState::Pending,
                error: Some("retry bunker handshake".into()),
                updated_at: Some("2026-04-19T11:49:00Z".into()),
                label: None,
            },
        )
        .expect("failed to restore pending auth runtime");
        assert!(matches!(
            pending_shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Pending)
        ));

        let second_shell = sync_auth_runtime(app_handle).expect("failed to resync auth runtime");
        assert!(matches!(
            second_shell
                .auth_runtime
                .as_ref()
                .map(|runtime| &runtime.state),
            Some(AuthRuntimeState::Failed)
        ));
        let second_client = auth_runtime_client_store::load(app_handle)
            .expect("failed to reload auth runtime client store")
            .expect("missing reloaded auth runtime client");

        assert_eq!(second_client.public_key, first_client.public_key);
        assert_eq!(second_client.secret_key_hex, first_client.secret_key_hex);
        assert_eq!(second_client.stored_at, first_client.stored_at);
    }

    #[test]
    fn sync_auth_runtime_persists_rehydrated_runtime_into_shell_state_store() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(ShellStateSnapshot {
                is_authenticated: true,
                auth_session: Some(AuthSessionSummary {
                    login_method: LoginMethod::Signer,
                    access: LoginAccessSummary {
                        kind: LoginAccessKind::NostrConnect,
                        label: "nostrconnect://signer.example".into(),
                        pubkey: None,
                    },
                    circle_selection_mode: LoginCircleSelectionMode::Existing,
                    logged_in_at: "2026-04-19T11:55:00Z".into(),
                }),
                auth_runtime: Some(AuthRuntimeSummary {
                    state: AuthRuntimeState::Pending,
                    login_method: LoginMethod::Signer,
                    access_kind: LoginAccessKind::NostrConnect,
                    label: "nostrconnect://signer.example".into(),
                    pubkey: None,
                    error: Some("Remote signer handshake is pending.".into()),
                    can_send_messages: false,
                    send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                    persisted_in_native_store: false,
                    credential_persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:55:00Z".into(),
                }),
                auth_runtime_binding: Some(AuthRuntimeBindingSummary {
                    access_kind: LoginAccessKind::NostrConnect,
                    endpoint: "wss://relay.example.com".into(),
                    connection_pubkey: None,
                    relay_count: 1,
                    has_secret: false,
                    requested_permissions: vec![],
                    client_name: None,
                    persisted_in_native_store: false,
                    updated_at: "2026-04-19T11:55:00Z".into(),
                }),
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
                app_preferences: crate::domain::chat::default_app_preferences(),
                notification_preferences: crate::domain::chat::default_notification_preferences(),
                advanced_preferences: crate::domain::chat::default_advanced_preferences(),
                active_circle_id: "main-circle".into(),
                selected_session_id: "alice".into(),
            })
            .expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");
        let pubkey = valid_binding_pubkey_hex();
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                value: format!(
                    "nostrconnect://{pubkey}?relay=wss://relay.example.com&secret=shared-secret"
                ),
                stored_at: "2026-04-19T11:55:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime binding store");
        auth_runtime_state_store::save(
            app_handle,
            &auth_runtime_state_store::StoredAuthRuntimeState {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                label: "Remote Signer A".into(),
                logged_in_at: "2026-04-19T11:55:00Z".into(),
                state: AuthRuntimeState::Connected,
                pubkey: None,
                error: None,
                updated_at: "2026-04-19T11:58:00Z".into(),
            },
        )
        .expect("failed to seed auth runtime state store");

        let shell = sync_auth_runtime(app_handle).expect("failed to sync auth runtime");
        let runtime = shell.auth_runtime.expect("missing synced auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert_eq!(runtime.label, "Remote Signer A");
        assert!(runtime.error.is_none());
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime.persisted_in_native_store);
        assert_eq!(runtime.updated_at, "2026-04-19T11:58:00Z");

        let persisted_shell = shell_state_store::load(app_handle)
            .expect("failed to load persisted shell state")
            .and_then(crate::app::shell_auth::deserialize_shell_state_snapshot)
            .expect("missing persisted shell snapshot");
        let persisted_runtime = persisted_shell
            .auth_runtime
            .expect("missing persisted auth runtime");
        assert!(matches!(
            persisted_runtime.state,
            AuthRuntimeState::Connected
        ));
        assert_eq!(persisted_runtime.label, "Remote Signer A");
        assert!(persisted_runtime.can_send_messages);
        assert!(persisted_runtime.persisted_in_native_store);

        let persisted_binding = persisted_shell
            .auth_runtime_binding
            .expect("missing persisted auth runtime binding");
        assert!(persisted_binding.persisted_in_native_store);
        assert_eq!(persisted_binding.endpoint, "wss://relay.example.com");
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
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
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
                auth_runtime: None,
                auth_runtime_binding: None,
                user_profile: crate::domain::chat::default_user_profile(),
                restorable_circles: vec![],
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
                reply_to_message_id: None,
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
