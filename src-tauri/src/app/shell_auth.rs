use crate::app::auth_access;
use crate::domain::chat::{
    AuthRuntimeBindingSummary, AuthRuntimeState, AuthRuntimeSummary, AuthSessionSummary,
    ChatDomainSeed, LoginAccessInput, LoginAccessKind, LoginMethod, PersistedShellState,
    ShellStateSnapshot, SignedNostrEvent, UpdateAuthRuntimeInput,
};
use crate::infra::auth_runtime_binding_store::{self, StoredAuthRuntimeBinding};
use crate::infra::auth_runtime_client_store::{self, StoredAuthRuntimeClient};
use crate::infra::auth_runtime_credential_store::{self, StoredAuthRuntimeCredential};
use crate::infra::auth_runtime_state_store::{self, StoredAuthRuntimeState};
use crate::infra::shell_state_store;
use nostr_connect::prelude::{
    Event as NostrEvent, EventBuilder as NostrEventBuilder, Keys as NostrKeys, NostrConnect,
    NostrConnectURI, NostrSigner, PublicKey as NostrPublicKey, Tag as NostrTag,
    Timestamp as NostrTimestamp, ToBech32,
};
use serde_json::Value;
use std::future::Future;
use std::time::Duration;

const REMOTE_SIGNER_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(4);

pub fn deserialize_shell_state_snapshot(value: Value) -> Option<ShellStateSnapshot> {
    serde_json::from_value::<ShellStateSnapshot>(value.clone())
        .ok()
        .map(hydrate_shell_snapshot)
        .or_else(|| {
            serde_json::from_value::<PersistedShellState>(value)
                .ok()
                .map(ShellStateSnapshot::from)
                .map(hydrate_shell_snapshot)
        })
}

pub fn load_saved_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ShellStateSnapshot, String> {
    let shell = read_persisted_shell_snapshot(app_handle)?;

    hydrate_shell_snapshot_from_native_stores(app_handle, shell)
}

pub fn sync_saved_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ShellStateSnapshot, String> {
    let persisted_shell = read_persisted_shell_snapshot(app_handle)?;
    let hydrated_shell =
        hydrate_shell_snapshot_from_native_stores(app_handle, persisted_shell.clone())?;
    let hydrated_shell = sync_pending_remote_auth_runtime(app_handle, hydrated_shell)?;

    if !same_shell_snapshot(&persisted_shell, &hydrated_shell) {
        let shell_state =
            serde_json::to_value(&hydrated_shell).map_err(|error| error.to_string())?;
        shell_state_store::save(app_handle, shell_state)?;
    }

    Ok(hydrated_shell)
}

pub fn hydrate_shell_snapshot(mut shell: ShellStateSnapshot) -> ShellStateSnapshot {
    if shell.auth_runtime.is_none() {
        shell.auth_runtime = shell
            .auth_session
            .as_ref()
            .map(derive_auth_runtime_from_session);
    }

    shell
}

pub fn persist_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    runtime: &AuthRuntimeSummary,
) -> Result<AuthRuntimeSummary, String> {
    let credential_persisted_in_native_store =
        auth_runtime_credential_persisted_in_native_store(app_handle, auth_session)?;
    let binding_persisted_in_native_store =
        auth_runtime_binding_persisted_in_native_store(app_handle, auth_session)?;
    let runtime = AuthRuntimeSummary {
        persisted_in_native_store: true,
        ..normalize_auth_runtime_summary(
            auth_session,
            runtime,
            credential_persisted_in_native_store,
            binding_persisted_in_native_store,
        )?
    };
    let stored_runtime = StoredAuthRuntimeState {
        login_method: auth_session.login_method.clone(),
        access_kind: auth_session.access.kind.clone(),
        label: runtime.label.clone(),
        logged_in_at: auth_session.logged_in_at.clone(),
        state: runtime.state.clone(),
        pubkey: runtime.pubkey.clone(),
        error: runtime.error.clone(),
        updated_at: runtime.updated_at.clone(),
    };
    auth_runtime_state_store::save(app_handle, &stored_runtime)?;
    Ok(runtime)
}

pub fn clear_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<(), String> {
    auth_runtime_state_store::clear(app_handle)
}

pub fn clear_auth_runtime_client<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<(), String> {
    auth_runtime_client_store::clear(app_handle)
}

pub fn sign_remote_auth_runtime_text_note<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    content: &str,
    created_at: u64,
    tags: &[Vec<String>],
) -> Result<SignedNostrEvent, String> {
    let binding = auth_runtime_binding_store::load(app_handle)?
        .filter(|binding| stored_auth_runtime_binding_matches_session(binding, auth_session))
        .ok_or_else(missing_remote_auth_runtime_binding_error)?;

    let result = match binding.access_kind {
        LoginAccessKind::Bunker => sign_remote_bunker_auth_runtime_text_note(
            app_handle,
            auth_session,
            &binding,
            content,
            created_at,
            tags,
        ),
        LoginAccessKind::NostrConnect => Err(unsupported_nostrconnect_client_uri_error()),
        _ => Err("Stored remote signer binding is not supported.".into()),
    };

    match result {
        Ok(signed_note) => {
            persist_observed_auth_runtime(
                app_handle,
                auth_session,
                Some(AuthRuntimeState::Connected),
                None,
                None,
            )?;
            Ok(signed_note)
        }
        Err(error) => {
            persist_observed_auth_runtime(
                app_handle,
                auth_session,
                Some(AuthRuntimeState::Connected),
                Some(error.clone()),
                None,
            )?;
            Err(error)
        }
    }
}

pub fn derive_auth_runtime_from_session(auth_session: &AuthSessionSummary) -> AuthRuntimeSummary {
    let (state, error) = match auth_session.login_method {
        LoginMethod::QuickStart => (AuthRuntimeState::LocalProfile, None),
        LoginMethod::ExistingAccount => match auth_session.access.kind {
            LoginAccessKind::Nsec | LoginAccessKind::HexKey => (AuthRuntimeState::Connected, None),
            LoginAccessKind::Npub => (
                AuthRuntimeState::Failed,
                Some("Read-only npub import cannot sign messages yet.".into()),
            ),
            LoginAccessKind::Bunker => (
                AuthRuntimeState::Pending,
                Some(
                    "Remote bunker handoff is stored, but signer handshake is not implemented yet."
                        .into(),
                ),
            ),
            LoginAccessKind::LocalProfile | LoginAccessKind::NostrConnect => (
                AuthRuntimeState::Failed,
                Some("Unsupported existing-account auth runtime input.".into()),
            ),
        },
        LoginMethod::Signer => match auth_session.access.kind {
            LoginAccessKind::Bunker | LoginAccessKind::NostrConnect => (
                AuthRuntimeState::Pending,
                Some("Remote signer handshake is not implemented yet.".into()),
            ),
            LoginAccessKind::LocalProfile
            | LoginAccessKind::Nsec
            | LoginAccessKind::Npub
            | LoginAccessKind::HexKey => (
                AuthRuntimeState::Failed,
                Some("Unsupported signer auth runtime input.".into()),
            ),
        },
    };

    let send_blocked_reason = default_send_blocked_reason_for_state(
        &auth_session.access.kind,
        state.clone(),
        error.as_deref(),
    );
    let can_send_messages = send_blocked_reason.is_none();

    AuthRuntimeSummary {
        state,
        login_method: auth_session.login_method.clone(),
        access_kind: auth_session.access.kind.clone(),
        label: auth_session.access.label.clone(),
        pubkey: auth_session.access.pubkey.clone(),
        error,
        can_send_messages,
        send_blocked_reason,
        persisted_in_native_store: false,
        credential_persisted_in_native_store: false,
        updated_at: auth_session.logged_in_at.clone(),
    }
}

pub fn auth_runtime_send_block_reason(shell: &ShellStateSnapshot) -> Option<String> {
    if !shell.is_authenticated {
        return None;
    }

    let runtime = shell.auth_runtime.clone().or_else(|| {
        shell
            .auth_session
            .as_ref()
            .map(derive_auth_runtime_from_session)
    })?;

    runtime.send_blocked_reason
}

pub fn update_auth_runtime(
    shell: &ShellStateSnapshot,
    input: &UpdateAuthRuntimeInput,
) -> Result<AuthRuntimeSummary, String> {
    if !shell.is_authenticated {
        return Err("auth runtime update requires an authenticated shell".into());
    }

    let auth_session = shell.auth_session.as_ref().ok_or_else(|| {
        "auth runtime update requires an authenticated session summary".to_string()
    })?;
    let base_runtime = shell
        .auth_runtime
        .clone()
        .unwrap_or_else(|| derive_auth_runtime_from_session(auth_session));
    let binding_persisted_in_native_store = shell
        .auth_runtime_binding
        .as_ref()
        .filter(|binding| auth_runtime_binding_matches_session(binding, auth_session))
        .is_some_and(|binding| binding.persisted_in_native_store);

    validate_requested_auth_runtime_state(auth_session, &input.state)?;

    let next_error = match input.state {
        AuthRuntimeState::LocalProfile | AuthRuntimeState::Connected => None,
        AuthRuntimeState::Pending | AuthRuntimeState::Failed => {
            normalized_non_empty(input.error.as_deref())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    if same_auth_runtime_state(&base_runtime.state, &input.state) {
                        base_runtime.error.clone()
                    } else {
                        default_auth_runtime_error(auth_session, &input.state)
                    }
                })
        }
    };

    normalize_auth_runtime_summary(
        auth_session,
        &AuthRuntimeSummary {
            state: input.state.clone(),
            login_method: auth_session.login_method.clone(),
            access_kind: auth_session.access.kind.clone(),
            label: normalized_non_empty(input.label.as_deref())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| base_runtime.label.clone()),
            pubkey: base_runtime
                .pubkey
                .clone()
                .or_else(|| auth_session.access.pubkey.clone()),
            error: next_error,
            can_send_messages: base_runtime.can_send_messages,
            send_blocked_reason: base_runtime.send_blocked_reason.clone(),
            persisted_in_native_store: base_runtime.persisted_in_native_store,
            credential_persisted_in_native_store: base_runtime.credential_persisted_in_native_store,
            updated_at: normalized_non_empty(input.updated_at.as_deref())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| auth_session.logged_in_at.clone()),
        },
        base_runtime.credential_persisted_in_native_store,
        binding_persisted_in_native_store,
    )
}

pub fn persist_auth_runtime_credential<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    access: &LoginAccessInput,
) -> Result<bool, String> {
    let Some(credential) = auth_access::resolve_auth_runtime_credential(access)? else {
        auth_runtime_credential_store::clear(app_handle)?;
        return Ok(false);
    };

    let stored_credential = StoredAuthRuntimeCredential {
        login_method: auth_session.login_method.clone(),
        access_kind: credential.access_kind,
        secret_key_hex: credential.secret_key_hex,
        pubkey: credential.pubkey,
        stored_at: auth_session.logged_in_at.clone(),
    };
    auth_runtime_credential_store::save(app_handle, &stored_credential)?;
    Ok(true)
}

pub fn clear_auth_runtime_credential<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<(), String> {
    auth_runtime_credential_store::clear(app_handle)
}

pub fn persist_auth_runtime_binding<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    access: &LoginAccessInput,
) -> Result<Option<AuthRuntimeBindingSummary>, String> {
    let Some(binding) = auth_access::resolve_auth_runtime_binding(access)? else {
        auth_runtime_client_store::clear(app_handle)?;
        auth_runtime_binding_store::clear(app_handle)?;
        return Ok(None);
    };
    auth_runtime_client_store::clear(app_handle)?;
    let raw_value = normalized_non_empty(access.value.as_deref())
        .expect("resolved auth runtime binding should keep its raw input");

    let stored_binding = StoredAuthRuntimeBinding {
        login_method: auth_session.login_method.clone(),
        access_kind: binding.access_kind.clone(),
        value: raw_value.to_string(),
        stored_at: auth_session.logged_in_at.clone(),
    };
    auth_runtime_binding_store::save(app_handle, &stored_binding)?;

    Ok(Some(build_auth_runtime_binding_summary(
        &stored_binding,
        true,
    )))
}

pub fn clear_auth_runtime_binding<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<(), String> {
    auth_runtime_client_store::clear(app_handle)?;
    auth_runtime_binding_store::clear(app_handle)
}

pub fn build_auth_runtime_binding_summary(
    binding: &StoredAuthRuntimeBinding,
    persisted_in_native_store: bool,
) -> AuthRuntimeBindingSummary {
    let resolved_binding = auth_access::resolve_auth_runtime_binding(&LoginAccessInput {
        kind: binding.access_kind.clone(),
        value: Some(binding.value.clone()),
    })
    .ok()
    .flatten();

    AuthRuntimeBindingSummary {
        access_kind: binding.access_kind.clone(),
        endpoint: resolved_binding
            .as_ref()
            .map(|binding| binding.endpoint.clone())
            .unwrap_or_else(|| auth_runtime_binding_endpoint(&binding.value)),
        connection_pubkey: resolved_binding
            .as_ref()
            .map(|binding| binding.connection_pubkey.clone()),
        relay_count: resolved_binding
            .as_ref()
            .map(|binding| binding.relay_count)
            .unwrap_or(0),
        has_secret: resolved_binding
            .as_ref()
            .map(|binding| binding.has_secret)
            .unwrap_or(false),
        requested_permissions: resolved_binding
            .as_ref()
            .map(|binding| binding.requested_permissions.clone())
            .unwrap_or_default(),
        client_name: resolved_binding
            .as_ref()
            .and_then(|binding| binding.client_name.clone()),
        persisted_in_native_store,
        updated_at: binding.stored_at.clone(),
    }
}

fn hydrate_shell_snapshot_from_native_stores<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    mut shell: ShellStateSnapshot,
) -> Result<ShellStateSnapshot, String> {
    if !shell.is_authenticated {
        shell.auth_runtime = None;
        shell.auth_runtime_binding = None;
        return Ok(shell);
    }

    let Some(auth_session) = shell.auth_session.as_ref() else {
        shell.auth_runtime = None;
        shell.auth_runtime_binding = None;
        return Ok(shell);
    };

    shell.auth_runtime = Some(resolve_auth_runtime_summary(
        app_handle,
        auth_session,
        shell.auth_runtime.as_ref(),
    )?);
    shell.auth_runtime_binding = resolve_auth_runtime_binding_summary(
        app_handle,
        auth_session,
        shell.auth_runtime_binding.as_ref(),
    )?;

    Ok(shell)
}

fn sync_pending_remote_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    mut shell: ShellStateSnapshot,
) -> Result<ShellStateSnapshot, String> {
    if !shell.is_authenticated {
        return Ok(shell);
    }

    let Some(auth_session) = shell.auth_session.clone() else {
        return Ok(shell);
    };
    let Some(runtime) = shell.auth_runtime.clone() else {
        return Ok(shell);
    };

    if !matches!(runtime.state, AuthRuntimeState::Pending)
        || !supports_auth_runtime_binding(&auth_session.access.kind)
    {
        return Ok(shell);
    }

    let Some(stored_binding) = auth_runtime_binding_store::load(app_handle)?
        .filter(|binding| stored_auth_runtime_binding_matches_session(binding, &auth_session))
    else {
        return Ok(shell);
    };

    let next_runtime =
        match handshake_remote_auth_runtime_binding(app_handle, &auth_session, &stored_binding) {
            Ok(handshake) => AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: auth_session.login_method.clone(),
                access_kind: auth_session.access.kind.clone(),
                label: runtime.label.clone(),
                pubkey: handshake.user_pubkey.or_else(|| {
                    runtime
                        .pubkey
                        .clone()
                        .or_else(|| auth_session.access.pubkey.clone())
                }),
                error: None,
                can_send_messages: runtime.can_send_messages,
                send_blocked_reason: runtime.send_blocked_reason.clone(),
                persisted_in_native_store: runtime.persisted_in_native_store,
                credential_persisted_in_native_store: runtime.credential_persisted_in_native_store,
                updated_at: auth_runtime_updated_at(&runtime, &auth_session),
            },
            Err(error) => AuthRuntimeSummary {
                state: AuthRuntimeState::Failed,
                login_method: auth_session.login_method.clone(),
                access_kind: auth_session.access.kind.clone(),
                label: runtime.label.clone(),
                pubkey: runtime
                    .pubkey
                    .clone()
                    .or_else(|| auth_session.access.pubkey.clone()),
                error: Some(error),
                can_send_messages: runtime.can_send_messages,
                send_blocked_reason: runtime.send_blocked_reason.clone(),
                persisted_in_native_store: runtime.persisted_in_native_store,
                credential_persisted_in_native_store: runtime.credential_persisted_in_native_store,
                updated_at: auth_runtime_updated_at(&runtime, &auth_session),
            },
        };
    let next_runtime = persist_auth_runtime(app_handle, &auth_session, &next_runtime)?;
    shell.auth_runtime = Some(next_runtime);

    Ok(shell)
}

fn validate_requested_auth_runtime_state(
    auth_session: &AuthSessionSummary,
    requested_state: &AuthRuntimeState,
) -> Result<(), String> {
    match auth_session.login_method {
        LoginMethod::QuickStart => {
            if !matches!(requested_state, AuthRuntimeState::LocalProfile) {
                return Err("quick start auth runtime is fixed to `localProfile`".into());
            }
        }
        LoginMethod::ExistingAccount | LoginMethod::Signer => {
            if matches!(requested_state, AuthRuntimeState::LocalProfile) {
                return Err("only quick start auth sessions can use `localProfile` runtime".into());
            }
        }
    }

    Ok(())
}

fn default_auth_runtime_error(
    auth_session: &AuthSessionSummary,
    state: &AuthRuntimeState,
) -> Option<String> {
    match state {
        AuthRuntimeState::LocalProfile | AuthRuntimeState::Connected => None,
        AuthRuntimeState::Pending => Some(match auth_session.access.kind {
            LoginAccessKind::Bunker | LoginAccessKind::NostrConnect => {
                "Remote signer handshake is not implemented yet.".into()
            }
            _ => "Account runtime is still waiting for a signer handshake.".into(),
        }),
        AuthRuntimeState::Failed => Some(match auth_session.access.kind {
            LoginAccessKind::Npub => "Read-only npub import cannot sign messages yet.".into(),
            LoginAccessKind::Bunker | LoginAccessKind::NostrConnect => {
                "Remote signer handshake failed or has not completed yet.".into()
            }
            _ => "This account runtime cannot send messages yet.".into(),
        }),
    }
}

fn resolve_auth_runtime_summary<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    shell_runtime: Option<&AuthRuntimeSummary>,
) -> Result<AuthRuntimeSummary, String> {
    let credential_persisted_in_native_store =
        auth_runtime_credential_persisted_in_native_store(app_handle, auth_session)?;
    let binding_persisted_in_native_store =
        auth_runtime_binding_persisted_in_native_store(app_handle, auth_session)?;
    if requires_local_auth_runtime_credential(&auth_session.access.kind)
        && !credential_persisted_in_native_store
    {
        return persist_auth_runtime(
            app_handle,
            auth_session,
            &AuthRuntimeSummary {
                state: AuthRuntimeState::Failed,
                login_method: auth_session.login_method.clone(),
                access_kind: auth_session.access.kind.clone(),
                label: auth_session.access.label.clone(),
                pubkey: auth_session.access.pubkey.clone(),
                error: Some(missing_local_auth_runtime_credential_error()),
                can_send_messages: false,
                send_blocked_reason: Some(missing_local_auth_runtime_credential_error()),
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: auth_session.logged_in_at.clone(),
            },
        );
    }
    if supports_auth_runtime_binding(&auth_session.access.kind)
        && !binding_persisted_in_native_store
    {
        return persist_auth_runtime(
            app_handle,
            auth_session,
            &AuthRuntimeSummary {
                state: AuthRuntimeState::Failed,
                login_method: auth_session.login_method.clone(),
                access_kind: auth_session.access.kind.clone(),
                label: auth_session.access.label.clone(),
                pubkey: auth_session.access.pubkey.clone(),
                error: Some(missing_remote_auth_runtime_binding_error()),
                can_send_messages: false,
                send_blocked_reason: Some(missing_remote_auth_runtime_binding_error()),
                persisted_in_native_store: false,
                credential_persisted_in_native_store: credential_persisted_in_native_store,
                updated_at: auth_session.logged_in_at.clone(),
            },
        );
    }

    if let Some(stored_runtime) = auth_runtime_state_store::load(app_handle)?
        .filter(|stored_runtime| stored_auth_runtime_matches_session(stored_runtime, auth_session))
    {
        return normalize_auth_runtime_summary(
            auth_session,
            &build_auth_runtime_from_stored_state(&stored_runtime),
            credential_persisted_in_native_store,
            binding_persisted_in_native_store,
        );
    }

    let fallback_runtime = shell_runtime
        .filter(|runtime| auth_runtime_matches_session(runtime, auth_session))
        .cloned()
        .map(|runtime| {
            normalize_auth_runtime_summary(
                auth_session,
                &runtime,
                credential_persisted_in_native_store,
                binding_persisted_in_native_store,
            )
        })
        .transpose()?
        .unwrap_or_else(|| derive_auth_runtime_from_session(auth_session));

    persist_auth_runtime(app_handle, auth_session, &fallback_runtime)
}

fn resolve_auth_runtime_binding_summary<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    shell_binding: Option<&AuthRuntimeBindingSummary>,
) -> Result<Option<AuthRuntimeBindingSummary>, String> {
    if let Some(stored_binding) =
        auth_runtime_binding_store::load(app_handle)?.filter(|stored_binding| {
            stored_auth_runtime_binding_matches_session(stored_binding, auth_session)
        })
    {
        return Ok(Some(build_auth_runtime_binding_summary(
            &stored_binding,
            true,
        )));
    }

    Ok(shell_binding
        .filter(|binding| auth_runtime_binding_matches_session(binding, auth_session))
        .cloned()
        .map(|mut binding| {
            binding.persisted_in_native_store = false;
            binding
        }))
}

fn normalize_auth_runtime_summary(
    auth_session: &AuthSessionSummary,
    runtime: &AuthRuntimeSummary,
    credential_persisted_in_native_store: bool,
    binding_persisted_in_native_store: bool,
) -> Result<AuthRuntimeSummary, String> {
    let state = if requires_local_auth_runtime_credential(&auth_session.access.kind)
        && !credential_persisted_in_native_store
    {
        AuthRuntimeState::Failed
    } else if supports_auth_runtime_binding(&auth_session.access.kind)
        && !binding_persisted_in_native_store
    {
        AuthRuntimeState::Failed
    } else {
        runtime.state.clone()
    };

    validate_requested_auth_runtime_state(auth_session, &state)?;

    let error = match state {
        AuthRuntimeState::LocalProfile => None,
        AuthRuntimeState::Connected => {
            normalized_non_empty(runtime.error.as_deref()).map(ToOwned::to_owned)
        }
        AuthRuntimeState::Pending => normalized_non_empty(runtime.error.as_deref())
            .map(ToOwned::to_owned)
            .or_else(|| default_auth_runtime_error(auth_session, &state)),
        AuthRuntimeState::Failed => {
            if requires_local_auth_runtime_credential(&auth_session.access.kind)
                && !credential_persisted_in_native_store
            {
                Some(missing_local_auth_runtime_credential_error())
            } else if supports_auth_runtime_binding(&auth_session.access.kind)
                && !binding_persisted_in_native_store
            {
                Some(missing_remote_auth_runtime_binding_error())
            } else {
                normalized_non_empty(runtime.error.as_deref())
                    .map(ToOwned::to_owned)
                    .or_else(|| default_auth_runtime_error(auth_session, &state))
            }
        }
    };
    let send_blocked_reason = default_send_blocked_reason_for_state(
        &auth_session.access.kind,
        state.clone(),
        error.as_deref(),
    );

    Ok(AuthRuntimeSummary {
        state,
        login_method: auth_session.login_method.clone(),
        access_kind: auth_session.access.kind.clone(),
        label: normalized_non_empty(Some(runtime.label.as_str()))
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| auth_session.access.label.clone()),
        pubkey: runtime
            .pubkey
            .clone()
            .or_else(|| auth_session.access.pubkey.clone()),
        error,
        can_send_messages: send_blocked_reason.is_none(),
        send_blocked_reason,
        persisted_in_native_store: runtime.persisted_in_native_store,
        credential_persisted_in_native_store,
        updated_at: normalized_non_empty(Some(runtime.updated_at.as_str()))
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| auth_session.logged_in_at.clone()),
    })
}

fn build_auth_runtime_from_stored_state(
    stored_runtime: &StoredAuthRuntimeState,
) -> AuthRuntimeSummary {
    AuthRuntimeSummary {
        state: stored_runtime.state.clone(),
        login_method: stored_runtime.login_method.clone(),
        access_kind: stored_runtime.access_kind.clone(),
        label: stored_runtime.label.clone(),
        pubkey: stored_runtime.pubkey.clone(),
        error: stored_runtime.error.clone(),
        can_send_messages: default_send_blocked_reason_for_state(
            &stored_runtime.access_kind,
            stored_runtime.state.clone(),
            stored_runtime.error.as_deref(),
        )
        .is_none(),
        send_blocked_reason: default_send_blocked_reason_for_state(
            &stored_runtime.access_kind,
            stored_runtime.state.clone(),
            stored_runtime.error.as_deref(),
        ),
        persisted_in_native_store: true,
        credential_persisted_in_native_store: false,
        updated_at: stored_runtime.updated_at.clone(),
    }
}

fn auth_runtime_credential_persisted_in_native_store<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
) -> Result<bool, String> {
    Ok(
        auth_runtime_credential_store::load(app_handle)?.is_some_and(|credential| {
            stored_auth_runtime_credential_matches_session(&credential, auth_session)
        }),
    )
}

fn auth_runtime_binding_persisted_in_native_store<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
) -> Result<bool, String> {
    Ok(auth_runtime_binding_store::load(app_handle)?
        .is_some_and(|binding| stored_auth_runtime_binding_matches_session(&binding, auth_session)))
}

fn requires_local_auth_runtime_credential(access_kind: &LoginAccessKind) -> bool {
    matches!(access_kind, LoginAccessKind::Nsec | LoginAccessKind::HexKey)
}

fn missing_local_auth_runtime_credential_error() -> String {
    "Validated local account key is missing from native credential store.".into()
}

fn missing_remote_auth_runtime_binding_error() -> String {
    "Stored remote signer binding is missing from native binding store.".into()
}

fn default_send_blocked_reason_for_state(
    access_kind: &LoginAccessKind,
    state: AuthRuntimeState,
    error: Option<&str>,
) -> Option<String> {
    match state {
        AuthRuntimeState::LocalProfile => None,
        AuthRuntimeState::Connected => {
            if remote_signer_send_support_is_missing(access_kind) {
                Some(remote_signer_send_support_missing_error())
            } else {
                None
            }
        }
        AuthRuntimeState::Pending => Some(
            normalized_non_empty(error)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    "Account runtime is still waiting for a signer handshake.".into()
                }),
        ),
        AuthRuntimeState::Failed => Some(
            normalized_non_empty(error)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| "This account runtime cannot send messages yet.".into()),
        ),
    }
}

fn remote_signer_send_support_is_missing(access_kind: &LoginAccessKind) -> bool {
    matches!(access_kind, LoginAccessKind::NostrConnect)
}

fn remote_signer_send_support_missing_error() -> String {
    "Remote signer is connected, but message send still requires remote event-signing support."
        .into()
}

fn normalized_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn auth_runtime_updated_at(
    runtime: &AuthRuntimeSummary,
    auth_session: &AuthSessionSummary,
) -> String {
    normalized_non_empty(Some(runtime.updated_at.as_str()))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| auth_session.logged_in_at.clone())
}

fn current_auth_runtime_timestamp() -> String {
    NostrTimestamp::now().to_human_datetime()
}

fn persist_observed_auth_runtime<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    next_state: Option<AuthRuntimeState>,
    next_error: Option<String>,
    next_pubkey: Option<String>,
) -> Result<AuthRuntimeSummary, String> {
    let shell = read_persisted_shell_snapshot(app_handle)?;
    let shell_runtime = shell
        .auth_session
        .as_ref()
        .filter(|current_session| same_auth_session(current_session, auth_session))
        .and_then(|_| shell.auth_runtime.as_ref());
    let base_runtime = resolve_auth_runtime_summary(app_handle, auth_session, shell_runtime)?;
    let next_runtime = persist_auth_runtime(
        app_handle,
        auth_session,
        &AuthRuntimeSummary {
            state: next_state.unwrap_or_else(|| base_runtime.state.clone()),
            login_method: auth_session.login_method.clone(),
            access_kind: auth_session.access.kind.clone(),
            label: base_runtime.label.clone(),
            pubkey: next_pubkey
                .or_else(|| base_runtime.pubkey.clone())
                .or_else(|| auth_session.access.pubkey.clone()),
            error: next_error,
            can_send_messages: base_runtime.can_send_messages,
            send_blocked_reason: base_runtime.send_blocked_reason.clone(),
            persisted_in_native_store: base_runtime.persisted_in_native_store,
            credential_persisted_in_native_store: base_runtime.credential_persisted_in_native_store,
            updated_at: current_auth_runtime_timestamp(),
        },
    )?;

    persist_auth_runtime_into_current_shell(app_handle, auth_session, &next_runtime)?;
    Ok(next_runtime)
}

fn persist_auth_runtime_into_current_shell<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    runtime: &AuthRuntimeSummary,
) -> Result<(), String> {
    let mut shell = read_persisted_shell_snapshot(app_handle)?;
    if !shell
        .auth_session
        .as_ref()
        .is_some_and(|current_session| same_auth_session(current_session, auth_session))
    {
        return Ok(());
    }

    shell.auth_runtime = Some(runtime.clone());
    let shell_state = serde_json::to_value(&shell).map_err(|error| error.to_string())?;
    shell_state_store::save(app_handle, shell_state)
}

fn read_persisted_shell_snapshot<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<ShellStateSnapshot, String> {
    Ok(shell_state_store::load(app_handle)?
        .and_then(deserialize_shell_state_snapshot)
        .unwrap_or_else(|| ShellStateSnapshot::from(ChatDomainSeed::default())))
}

fn auth_runtime_binding_endpoint(raw_value: &str) -> String {
    extract_query_param(raw_value, "relay")
        .or_else(|| extract_query_param(raw_value, "url"))
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let remainder = raw_value
                .split_once("://")
                .map(|(_, value)| value)
                .unwrap_or(raw_value)
                .trim();
            let without_query = remainder
                .split(['?', '#'])
                .next()
                .unwrap_or(remainder)
                .trim();
            let endpoint = without_query
                .split('/')
                .next()
                .unwrap_or(without_query)
                .trim();
            truncate_binding_endpoint(endpoint)
        })
}

fn extract_query_param<'a>(raw_value: &'a str, key: &str) -> Option<&'a str> {
    let query = raw_value.split_once('?')?.1;

    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(name, value)| {
            if name.eq_ignore_ascii_case(key) {
                Some(value)
            } else {
                None
            }
        })
}

fn truncate_binding_endpoint(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= 48 {
        return trimmed.to_string();
    }

    let head = trimmed.chars().take(48).collect::<String>();
    format!("{head}...")
}

struct RemoteAuthRuntimeHandshake {
    user_pubkey: Option<String>,
}

fn handshake_remote_auth_runtime_binding<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    binding: &StoredAuthRuntimeBinding,
) -> Result<RemoteAuthRuntimeHandshake, String> {
    match binding.access_kind {
        LoginAccessKind::Bunker => {
            handshake_remote_bunker_auth_runtime_binding(app_handle, auth_session, binding)
        }
        LoginAccessKind::NostrConnect => Err(unsupported_nostrconnect_client_uri_error()),
        _ => Err("Stored remote signer binding is not supported.".into()),
    }
}

fn handshake_remote_bunker_auth_runtime_binding<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    binding: &StoredAuthRuntimeBinding,
) -> Result<RemoteAuthRuntimeHandshake, String> {
    let client = load_or_create_auth_runtime_client(app_handle, auth_session)?;
    let client_keys = auth_runtime_client_keys(&client)?;
    let uri = NostrConnectURI::parse(&binding.value)
        .map_err(|error| format!("Stored bunker URI is invalid: {error}"))?;

    if !uri.is_bunker() {
        return Err("Stored remote signer binding is not a bunker URI.".into());
    }

    let user_pubkey = block_on_remote_auth_runtime(async move {
        let connect = NostrConnect::new(uri, client_keys, REMOTE_SIGNER_HANDSHAKE_TIMEOUT, None)
            .map_err(|error| format!("failed to initialize bunker handshake: {error}"))?;
        let user_pubkey = connect
            .get_public_key()
            .await
            .map_err(|error| format!("Remote bunker handshake failed: {error}"));
        connect.shutdown().await;

        let user_pubkey = user_pubkey?;
        user_pubkey
            .to_bech32()
            .map_err(|error| format!("failed to encode remote bunker pubkey: {error}"))
    })?;

    Ok(RemoteAuthRuntimeHandshake {
        user_pubkey: Some(user_pubkey),
    })
}

fn sign_remote_bunker_auth_runtime_text_note<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
    binding: &StoredAuthRuntimeBinding,
    content: &str,
    created_at: u64,
    tags: &[Vec<String>],
) -> Result<SignedNostrEvent, String> {
    let client = load_or_create_auth_runtime_client(app_handle, auth_session)?;
    let client_keys = auth_runtime_client_keys(&client)?;
    let uri = NostrConnectURI::parse(&binding.value)
        .map_err(|error| format!("Stored bunker URI is invalid: {error}"))?;

    if !uri.is_bunker() {
        return Err("Stored remote signer binding is not a bunker URI.".into());
    }

    let content = content.to_string();
    let tags = nostr_text_note_public_key_tags(tags);
    block_on_remote_auth_runtime(async move {
        let connect = NostrConnect::new(uri, client_keys, REMOTE_SIGNER_HANDSHAKE_TIMEOUT, None)
            .map_err(|error| format!("failed to initialize bunker signer client: {error}"))?;
        let signed_event = NostrEventBuilder::text_note(content)
            .tags(tags)
            .custom_created_at(NostrTimestamp::from_secs(created_at))
            .sign(&connect)
            .await
            .map_err(|error| format!("Remote bunker sign_event failed: {error}"));
        connect.shutdown().await;

        Ok(build_signed_nostr_event_from_remote_event(signed_event?))
    })
}

fn block_on_remote_auth_runtime<F, T>(future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, String>>,
{
    tauri::async_runtime::block_on(future)
}

fn load_or_create_auth_runtime_client<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    auth_session: &AuthSessionSummary,
) -> Result<StoredAuthRuntimeClient, String> {
    if let Some(stored_client) = auth_runtime_client_store::load(app_handle)?
        .filter(|client| stored_auth_runtime_client_matches_session(client, auth_session))
    {
        auth_runtime_client_keys(&stored_client)?;
        return Ok(stored_client);
    }

    let client_keys = NostrKeys::generate();
    let stored_client = StoredAuthRuntimeClient {
        login_method: auth_session.login_method.clone(),
        access_kind: auth_session.access.kind.clone(),
        public_key: client_keys.public_key().to_hex(),
        secret_key_hex: client_keys.secret_key().to_secret_hex(),
        stored_at: auth_session.logged_in_at.clone(),
    };
    auth_runtime_client_store::save(app_handle, &stored_client)?;
    Ok(stored_client)
}

fn auth_runtime_client_keys(client: &StoredAuthRuntimeClient) -> Result<NostrKeys, String> {
    let client_keys = NostrKeys::parse(&client.secret_key_hex)
        .map_err(|error| format!("Stored remote signer client secret is invalid: {error}"))?;
    if client_keys.public_key().to_hex() != client.public_key {
        return Err(
            "Stored remote signer client keypair does not match its persisted public key.".into(),
        );
    }

    Ok(client_keys)
}

fn build_signed_nostr_event_from_remote_event(event: NostrEvent) -> SignedNostrEvent {
    SignedNostrEvent {
        event_id: event.id.to_hex(),
        pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_secs(),
        kind: event.kind.as_u16() as u32,
        tags: event
            .tags
            .to_vec()
            .into_iter()
            .map(|tag| tag.to_vec())
            .collect(),
        content: event.content,
        signature: event.sig.to_string(),
    }
}

fn nostr_text_note_public_key_tags(tags: &[Vec<String>]) -> Vec<NostrTag> {
    tags.iter()
        .filter_map(|tag| {
            let value = tag.get(1)?;
            if tag.first().map(String::as_str) != Some("p") {
                return None;
            }

            NostrPublicKey::parse(value.trim())
                .ok()
                .map(NostrTag::public_key)
        })
        .collect()
}

fn unsupported_nostrconnect_client_uri_error() -> String {
    "Pasted nostrConnect client URIs are not supported yet; use a bunker:// handoff until desktop-generated client URIs are implemented.".into()
}

fn same_auth_runtime_state(left: &AuthRuntimeState, right: &AuthRuntimeState) -> bool {
    matches!(
        (left, right),
        (
            AuthRuntimeState::LocalProfile,
            AuthRuntimeState::LocalProfile
        ) | (AuthRuntimeState::Pending, AuthRuntimeState::Pending)
            | (AuthRuntimeState::Connected, AuthRuntimeState::Connected)
            | (AuthRuntimeState::Failed, AuthRuntimeState::Failed)
    )
}

fn auth_runtime_matches_session(
    runtime: &AuthRuntimeSummary,
    auth_session: &AuthSessionSummary,
) -> bool {
    same_login_method(&runtime.login_method, &auth_session.login_method)
        && same_login_access_kind(&runtime.access_kind, &auth_session.access.kind)
}

fn stored_auth_runtime_matches_session(
    runtime: &StoredAuthRuntimeState,
    auth_session: &AuthSessionSummary,
) -> bool {
    same_login_method(&runtime.login_method, &auth_session.login_method)
        && same_login_access_kind(&runtime.access_kind, &auth_session.access.kind)
        && runtime.logged_in_at == auth_session.logged_in_at
}

fn auth_runtime_binding_matches_session(
    binding: &AuthRuntimeBindingSummary,
    auth_session: &AuthSessionSummary,
) -> bool {
    supports_auth_runtime_binding(&auth_session.access.kind)
        && same_login_access_kind(&binding.access_kind, &auth_session.access.kind)
}

fn stored_auth_runtime_binding_matches_session(
    binding: &StoredAuthRuntimeBinding,
    auth_session: &AuthSessionSummary,
) -> bool {
    supports_auth_runtime_binding(&auth_session.access.kind)
        && same_login_method(&binding.login_method, &auth_session.login_method)
        && same_login_access_kind(&binding.access_kind, &auth_session.access.kind)
        && binding.stored_at == auth_session.logged_in_at
}

fn stored_auth_runtime_credential_matches_session(
    credential: &StoredAuthRuntimeCredential,
    auth_session: &AuthSessionSummary,
) -> bool {
    requires_local_auth_runtime_credential(&auth_session.access.kind)
        && same_login_method(&credential.login_method, &auth_session.login_method)
        && same_login_access_kind(&credential.access_kind, &auth_session.access.kind)
        && credential.stored_at == auth_session.logged_in_at
        && auth_session
            .access
            .pubkey
            .as_ref()
            .map_or(true, |pubkey| pubkey == &credential.pubkey)
}

fn stored_auth_runtime_client_matches_session(
    client: &StoredAuthRuntimeClient,
    auth_session: &AuthSessionSummary,
) -> bool {
    supports_auth_runtime_binding(&auth_session.access.kind)
        && same_login_method(&client.login_method, &auth_session.login_method)
        && same_login_access_kind(&client.access_kind, &auth_session.access.kind)
        && client.stored_at == auth_session.logged_in_at
}

fn same_auth_session(left: &AuthSessionSummary, right: &AuthSessionSummary) -> bool {
    same_login_method(&left.login_method, &right.login_method)
        && same_login_access_kind(&left.access.kind, &right.access.kind)
        && left.logged_in_at == right.logged_in_at
}

fn supports_auth_runtime_binding(access_kind: &LoginAccessKind) -> bool {
    matches!(
        access_kind,
        LoginAccessKind::Bunker | LoginAccessKind::NostrConnect
    )
}

fn same_login_method(left: &LoginMethod, right: &LoginMethod) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn same_login_access_kind(left: &LoginAccessKind, right: &LoginAccessKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn same_shell_snapshot(left: &ShellStateSnapshot, right: &ShellStateSnapshot) -> bool {
    serde_json::to_value(left).ok() == serde_json::to_value(right).ok()
}
