use crate::domain::chat::{
    LoginAccessInput, LoginAccessKind, LoginAccessSummary, LoginCompletionInput, LoginMethod,
    SignedNostrEvent,
};
use bech32::{Bech32, Hrp};
use secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};
use sha2::{Digest, Sha256};
use std::convert::TryInto;
use std::str::FromStr;
use url::Url;

const BUNKER_SCHEME: &str = "bunker";
const NPUB_HRP: &str = "npub";
const NSEC_HRP: &str = "nsec";
const NOSTRCONNECT_SCHEME: &str = "nostrconnect";

#[derive(Debug, Clone)]
pub struct ResolvedAuthRuntimeCredential {
    pub access_kind: LoginAccessKind,
    pub secret_key_hex: String,
    pub pubkey: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedAuthRuntimeBinding {
    pub access_kind: LoginAccessKind,
    pub endpoint: String,
    pub connection_pubkey: String,
    pub relay_count: u32,
    pub has_secret: bool,
    pub requested_permissions: Vec<String>,
    pub client_name: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedAuthRuntimeBindingUri {
    endpoint: String,
    connection_pubkey: String,
    relay_count: u32,
    has_secret: bool,
    requested_permissions: Vec<String>,
    client_name: Option<String>,
}

pub fn resolve_login_access_summary(
    input: &LoginCompletionInput,
) -> Result<LoginAccessSummary, String> {
    match input.method {
        LoginMethod::QuickStart => resolve_quick_start_access(&input.access),
        LoginMethod::ExistingAccount => resolve_existing_account_access(&input.access),
        LoginMethod::Signer => resolve_signer_access(&input.access),
    }
}

pub fn resolve_auth_runtime_credential(
    access: &LoginAccessInput,
) -> Result<Option<ResolvedAuthRuntimeCredential>, String> {
    let secret_key = match access.kind {
        LoginAccessKind::Nsec => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account requires an `nsec` key when access kind is `nsec`".to_string()
            })?;
            if !value.to_ascii_lowercase().starts_with(NSEC_HRP) {
                return Err(
                    "existing account requires an `nsec` key when access kind is `nsec`".into(),
                );
            }

            decode_nsec_secret_key(value)?
        }
        LoginAccessKind::HexKey => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account hex key must be a valid 32-byte secp256k1 secret key".to_string()
            })?;
            SecretKey::from_str(value).map_err(|_| {
                "existing account hex key must be a valid 32-byte secp256k1 secret key".to_string()
            })?
        }
        _ => return Ok(None),
    };

    Ok(Some(ResolvedAuthRuntimeCredential {
        access_kind: access.kind.clone(),
        secret_key_hex: encode_secret_key_hex(&secret_key),
        pubkey: canonical_npub_from_secret_key(&secret_key)?,
    }))
}

pub fn resolve_auth_runtime_binding(
    access: &LoginAccessInput,
) -> Result<Option<ResolvedAuthRuntimeBinding>, String> {
    let parsed = match access.kind {
        LoginAccessKind::Bunker => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "remote signer bunker URI must be a valid `bunker://<remote-signer-pubkey>?relay=...` URI".to_string()
            })?;
            parse_auth_runtime_binding_uri(
                value,
                BUNKER_SCHEME,
                false,
                "remote signer bunker URI must be a valid `bunker://<remote-signer-pubkey>?relay=...` URI",
            )?
        }
        LoginAccessKind::NostrConnect => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "remote signer nostrConnect URI must be a valid `nostrconnect://<remote-signer-pubkey>?relay=...&secret=...` URI".to_string()
            })?;
            parse_auth_runtime_binding_uri(
                value,
                NOSTRCONNECT_SCHEME,
                true,
                "remote signer nostrConnect URI must be a valid `nostrconnect://<remote-signer-pubkey>?relay=...&secret=...` URI",
            )?
        }
        _ => return Ok(None),
    };

    Ok(Some(ResolvedAuthRuntimeBinding {
        access_kind: access.kind.clone(),
        endpoint: parsed.endpoint,
        connection_pubkey: parsed.connection_pubkey,
        relay_count: parsed.relay_count,
        has_secret: parsed.has_secret,
        requested_permissions: parsed.requested_permissions,
        client_name: parsed.client_name,
    }))
}

pub fn sign_auth_runtime_text_note(
    secret_key_hex: &str,
    content: &str,
    created_at: u64,
    tags: Vec<Vec<String>>,
) -> Result<SignedNostrEvent, String> {
    let secret_key = SecretKey::from_str(secret_key_hex)
        .map_err(|_| "auth runtime secret key in native credential store is invalid".to_string())?;
    let secp = Secp256k1::new();
    let keypair = Keypair::from_secret_key(&secp, &secret_key);
    let (public_key, _) = secret_key.x_only_public_key(&secp);
    let pubkey = encode_lower_hex(&public_key.serialize());
    let signed_event = SignedNostrEvent {
        event_id: String::new(),
        pubkey,
        created_at,
        kind: 1,
        tags,
        content: content.to_string(),
        signature: String::new(),
    };
    let digest = build_signed_nostr_event_digest(&signed_event)?;
    let signature = secp.sign_schnorr_no_aux_rand(&digest, &keypair);

    Ok(SignedNostrEvent {
        event_id: encode_lower_hex(&digest),
        signature: signature.to_string(),
        ..signed_event
    })
}

fn build_signed_nostr_event_digest(event: &SignedNostrEvent) -> Result<[u8; 32], String> {
    let serialized = serde_json::to_string(&(
        0_u8,
        event.pubkey.as_str(),
        event.created_at,
        event.kind,
        &event.tags,
        event.content.as_str(),
    ))
    .map_err(|error| error.to_string())?;
    Ok(Sha256::digest(serialized.as_bytes()).into())
}

fn resolve_quick_start_access(access: &LoginAccessInput) -> Result<LoginAccessSummary, String> {
    if !matches!(access.kind, LoginAccessKind::LocalProfile) {
        return Err("quick start requires `localProfile` access".into());
    }

    Ok(LoginAccessSummary {
        kind: access.kind.clone(),
        label: "Quick Start".into(),
        pubkey: None,
    })
}

fn resolve_existing_account_access(
    access: &LoginAccessInput,
) -> Result<LoginAccessSummary, String> {
    match access.kind {
        LoginAccessKind::Nsec => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account requires an `nsec` key when access kind is `nsec`".to_string()
            })?;
            if !value.to_ascii_lowercase().starts_with(NSEC_HRP) {
                return Err(
                    "existing account requires an `nsec` key when access kind is `nsec`".into(),
                );
            }

            let pubkey = canonical_npub_from_secret_key(&decode_nsec_secret_key(value)?)?;
            Ok(build_public_key_access_summary(access.kind.clone(), pubkey))
        }
        LoginAccessKind::Npub => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account requires an `npub` key when access kind is `npub`".to_string()
            })?;
            if !value.to_ascii_lowercase().starts_with(NPUB_HRP) {
                return Err(
                    "existing account requires an `npub` key when access kind is `npub`".into(),
                );
            }

            let pubkey = canonical_npub_from_encoded_public_key(value)?;
            Ok(build_public_key_access_summary(access.kind.clone(), pubkey))
        }
        LoginAccessKind::HexKey => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account hex key must be a valid 32-byte secp256k1 secret key".to_string()
            })?;
            let secret_key = SecretKey::from_str(value).map_err(|_| {
                "existing account hex key must be a valid 32-byte secp256k1 secret key".to_string()
            })?;
            let pubkey = canonical_npub_from_secret_key(&secret_key)?;
            Ok(build_public_key_access_summary(access.kind.clone(), pubkey))
        }
        LoginAccessKind::Bunker => {
            let value = normalized_non_empty(access.value.as_deref()).ok_or_else(|| {
                "existing account bunker handoff must be a valid `bunker://<remote-signer-pubkey>?relay=...` URI".to_string()
            })?;
            resolve_auth_runtime_binding(access).map(|_| ())?;

            Ok(LoginAccessSummary {
                kind: access.kind.clone(),
                label: mask_value(value, 10, 6).unwrap_or_else(|| "bunker://".into()),
                pubkey: None,
            })
        }
        _ => {
            Err("existing account only supports `nsec`, `npub`, `hexKey` or `bunker` access".into())
        }
    }
}

fn resolve_signer_access(access: &LoginAccessInput) -> Result<LoginAccessSummary, String> {
    let value = normalized_non_empty(access.value.as_deref()).unwrap_or_default();

    match access.kind {
        LoginAccessKind::Bunker => {
            resolve_auth_runtime_binding(access).map(|_| ())?;

            Ok(LoginAccessSummary {
                kind: access.kind.clone(),
                label: mask_value(value, 10, 6).unwrap_or_else(|| "bunker://".into()),
                pubkey: None,
            })
        }
        LoginAccessKind::NostrConnect => {
            resolve_auth_runtime_binding(access).map(|_| ())?;

            Ok(LoginAccessSummary {
                kind: access.kind.clone(),
                label: mask_value(value, 14, 6).unwrap_or_else(|| "nostrconnect://".into()),
                pubkey: None,
            })
        }
        _ => Err("signer login only supports `bunker` or `nostrConnect` access".into()),
    }
}

fn build_public_key_access_summary(kind: LoginAccessKind, pubkey: String) -> LoginAccessSummary {
    LoginAccessSummary {
        kind,
        label: mask_value(&pubkey, 10, 6).unwrap_or_else(|| pubkey.clone()),
        pubkey: Some(pubkey),
    }
}

fn decode_nsec_secret_key(value: &str) -> Result<SecretKey, String> {
    let (hrp, data) = bech32::decode(value)
        .map_err(|_| "existing account `nsec` key is invalid or unsupported".to_string())?;
    if hrp.to_lowercase() != NSEC_HRP {
        return Err("existing account `nsec` key is invalid or unsupported".into());
    }

    let key_bytes: [u8; 32] = data
        .as_slice()
        .try_into()
        .map_err(|_| "existing account `nsec` key is invalid or unsupported".to_string())?;
    SecretKey::from_byte_array(key_bytes)
        .map_err(|_| "existing account `nsec` key is invalid or unsupported".to_string())
}

fn canonical_npub_from_encoded_public_key(value: &str) -> Result<String, String> {
    let (hrp, data) = bech32::decode(value)
        .map_err(|_| "existing account `npub` key is invalid or unsupported".to_string())?;
    if hrp.to_lowercase() != NPUB_HRP {
        return Err("existing account `npub` key is invalid or unsupported".into());
    }

    let key_bytes: [u8; 32] = data
        .as_slice()
        .try_into()
        .map_err(|_| "existing account `npub` key is invalid or unsupported".to_string())?;
    let public_key = XOnlyPublicKey::from_byte_array(key_bytes)
        .map_err(|_| "existing account `npub` key is invalid or unsupported".to_string())?;
    encode_npub(&public_key)
}

fn canonical_npub_from_secret_key(secret_key: &SecretKey) -> Result<String, String> {
    let secp = Secp256k1::new();
    let (public_key, _) = secret_key.x_only_public_key(&secp);
    encode_npub(&public_key)
}

fn encode_npub(public_key: &XOnlyPublicKey) -> Result<String, String> {
    let hrp = Hrp::parse(NPUB_HRP).expect("valid npub hrp");
    bech32::encode::<Bech32>(hrp, &public_key.serialize())
        .map_err(|_| "failed to encode canonical npub summary".to_string())
}

fn encode_secret_key_hex(secret_key: &SecretKey) -> String {
    encode_lower_hex(&secret_key.secret_bytes())
}

fn parse_auth_runtime_binding_uri(
    value: &str,
    expected_scheme: &str,
    require_secret: bool,
    error_message: &str,
) -> Result<ParsedAuthRuntimeBindingUri, String> {
    let uri = Url::parse(value).map_err(|_| error_message.to_string())?;
    if !uri.scheme().eq_ignore_ascii_case(expected_scheme) {
        return Err(error_message.into());
    }

    let connection_pubkey =
        canonical_hex_xonly_public_key(uri.host_str().ok_or_else(|| error_message.to_string())?)
            .map_err(|_| error_message.to_string())?;

    let mut relays = Vec::new();
    let mut has_secret = false;
    let mut requested_permissions = Vec::new();
    let mut client_name = None;

    for (name, value) in uri.query_pairs() {
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        match name.as_ref().to_ascii_lowercase().as_str() {
            "relay" => relays.push(validate_remote_signer_relay(value, error_message)?),
            "secret" => has_secret = true,
            "perms" => {
                requested_permissions.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|permission| !permission.is_empty())
                        .map(ToOwned::to_owned),
                );
            }
            "name" => {
                if client_name.is_none() {
                    client_name = Some(value.to_string());
                }
            }
            _ => {}
        }
    }

    if relays.is_empty() {
        return Err(error_message.into());
    }
    if require_secret && !has_secret {
        return Err(error_message.into());
    }

    Ok(ParsedAuthRuntimeBindingUri {
        endpoint: relays[0].clone(),
        connection_pubkey,
        relay_count: relays.len() as u32,
        has_secret,
        requested_permissions,
        client_name,
    })
}

fn validate_remote_signer_relay(value: &str, error_message: &str) -> Result<String, String> {
    let relay = Url::parse(value).map_err(|_| error_message.to_string())?;
    if !matches!(relay.scheme(), "ws" | "wss") || relay.host_str().is_none() {
        return Err(error_message.into());
    }

    Ok(value.to_string())
}

fn canonical_hex_xonly_public_key(value: &str) -> Result<String, String> {
    let key_bytes = decode_hex_32_bytes(value)?;
    let public_key = XOnlyPublicKey::from_byte_array(key_bytes)
        .map_err(|_| "invalid x-only public key".to_string())?;
    Ok(encode_lower_hex(&public_key.serialize()))
}

fn decode_hex_32_bytes(value: &str) -> Result<[u8; 32], String> {
    let trimmed = value.trim();
    if trimmed.len() != 64 {
        return Err("invalid 32-byte hex string".into());
    }

    let mut bytes = [0_u8; 32];
    for (index, chunk) in trimmed.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = decode_hex_nibble(chunk[0])? << 4 | decode_hex_nibble(chunk[1])?;
    }

    Ok(bytes)
}

fn decode_hex_nibble(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("invalid hex string".into()),
    }
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn mask_value(value: &str, prefix: usize, suffix: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() <= prefix + suffix {
        return Some(trimmed.to_string());
    }

    let head = trimmed.chars().take(prefix).collect::<String>();
    let tail = trimmed
        .chars()
        .rev()
        .take(suffix)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    Some(format!("{head}...{tail}"))
}

fn normalized_non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat::{LoginCircleSelectionInput, UserProfile};
    use secp256k1::schnorr::Signature;

    fn valid_binding_pubkey_hex() -> String {
        let secret_key =
            SecretKey::from_str("1111111111111111111111111111111111111111111111111111111111111111")
                .expect("valid test secret key should parse");
        let secp = Secp256k1::new();
        let (pubkey, _) = secret_key.x_only_public_key(&secp);
        encode_lower_hex(&pubkey.serialize())
    }

    fn make_input(method: LoginMethod, access: LoginAccessInput) -> LoginCompletionInput {
        LoginCompletionInput {
            method,
            access,
            user_profile: UserProfile {
                name: "Nora Blake".into(),
                handle: "@nora".into(),
                initials: "NB".into(),
                status: "Research".into(),
            },
            circle_selection: LoginCircleSelectionInput {
                mode: crate::domain::chat::LoginCircleSelectionMode::Existing,
                circle_id: Some("main-circle".into()),
                invite_code: None,
                name: None,
                relay: None,
                relays: None,
            },
            logged_in_at: Some("2026-04-19T08:00:00Z".into()),
        }
    }

    #[test]
    fn resolves_canonical_npub_from_hex_secret_key() {
        let summary = resolve_login_access_summary(&make_input(
            LoginMethod::ExistingAccount,
            LoginAccessInput {
                kind: LoginAccessKind::HexKey,
                value: Some(
                    "1111111111111111111111111111111111111111111111111111111111111111".into(),
                ),
            },
        ))
        .expect("hex key should resolve");

        let pubkey = summary
            .pubkey
            .expect("hex key should derive canonical npub");
        assert!(pubkey.starts_with("npub1"));
        assert_eq!(
            summary.label,
            mask_value(&pubkey, 10, 6).expect("pubkey mask should be present")
        );
    }

    #[test]
    fn resolves_runtime_credential_from_hex_secret_key() {
        let credential = resolve_auth_runtime_credential(&LoginAccessInput {
            kind: LoginAccessKind::HexKey,
            value: Some("1111111111111111111111111111111111111111111111111111111111111111".into()),
        })
        .expect("hex key should resolve credential")
        .expect("credential should be present");

        assert!(matches!(credential.access_kind, LoginAccessKind::HexKey));
        assert_eq!(
            credential.secret_key_hex,
            "1111111111111111111111111111111111111111111111111111111111111111"
        );
        assert!(credential.pubkey.starts_with("npub1"));
    }

    #[test]
    fn rejects_invalid_nsec_key() {
        let error = resolve_login_access_summary(&make_input(
            LoginMethod::ExistingAccount,
            LoginAccessInput {
                kind: LoginAccessKind::Nsec,
                value: Some("nsec1invalid".into()),
            },
        ))
        .expect_err("invalid nsec should reject");

        assert!(error.contains("invalid"));
    }

    #[test]
    fn signs_nostr_text_note_from_valid_secret_key() {
        let signed_note = sign_auth_runtime_text_note(
            "1111111111111111111111111111111111111111111111111111111111111111",
            "Ship the auth runtime.",
            1_713_513_600,
            vec![vec!["p".into(), valid_binding_pubkey_hex()]],
        )
        .expect("valid secret key should sign text note");

        let digest =
            build_signed_nostr_event_digest(&signed_note).expect("digest should serialize");
        let pubkey = XOnlyPublicKey::from_byte_array(
            decode_hex_32_bytes(&signed_note.pubkey).expect("pubkey hex should decode"),
        )
        .expect("pubkey bytes should parse");
        let signature = Signature::from_str(&signed_note.signature)
            .expect("signature should be encoded as hex");

        assert_eq!(signed_note.event_id, encode_lower_hex(&digest));
        assert_eq!(signed_note.pubkey.len(), 64);
        assert_eq!(signed_note.signature.len(), 128);
        assert_eq!(signed_note.tags.len(), 1);
        assert_eq!(signed_note.tags[0][0], "p");
        assert!(Secp256k1::new()
            .verify_schnorr(&signature, &digest, &pubkey)
            .is_ok());
    }

    #[test]
    fn resolves_bunker_auth_runtime_binding_summary() {
        let pubkey = valid_binding_pubkey_hex();
        let binding = resolve_auth_runtime_binding(&LoginAccessInput {
            kind: LoginAccessKind::Bunker,
            value: Some(format!(
                "bunker://{pubkey}?relay=wss://relay.example.com&relay=wss://backup.example.com"
            )),
        })
        .expect("bunker uri should resolve")
        .expect("binding should be present");

        assert!(matches!(binding.access_kind, LoginAccessKind::Bunker));
        assert_eq!(binding.connection_pubkey, pubkey);
        assert_eq!(binding.endpoint, "wss://relay.example.com");
        assert_eq!(binding.relay_count, 2);
        assert!(!binding.has_secret);
        assert!(binding.requested_permissions.is_empty());
        assert!(binding.client_name.is_none());
    }

    #[test]
    fn resolves_nostrconnect_auth_runtime_binding_summary() {
        let pubkey = valid_binding_pubkey_hex();
        let binding = resolve_auth_runtime_binding(&LoginAccessInput {
            kind: LoginAccessKind::NostrConnect,
            value: Some(format!(
                "nostrconnect://{pubkey}?relay=wss://relay.example.com&secret=shared-secret&perms=sign_event,get_public_key&name=Desk%20Client"
            )),
        })
        .expect("nostrconnect uri should resolve")
        .expect("binding should be present");

        assert!(matches!(binding.access_kind, LoginAccessKind::NostrConnect));
        assert_eq!(binding.connection_pubkey, pubkey);
        assert_eq!(binding.endpoint, "wss://relay.example.com");
        assert_eq!(binding.relay_count, 1);
        assert!(binding.has_secret);
        assert_eq!(
            binding.requested_permissions,
            vec!["sign_event", "get_public_key"]
        );
        assert_eq!(binding.client_name.as_deref(), Some("Desk Client"));
    }

    #[test]
    fn rejects_nostrconnect_binding_without_secret() {
        let pubkey = valid_binding_pubkey_hex();
        let error = resolve_auth_runtime_binding(&LoginAccessInput {
            kind: LoginAccessKind::NostrConnect,
            value: Some(format!(
                "nostrconnect://{pubkey}?relay=wss://relay.example.com"
            )),
        })
        .expect_err("nostrconnect binding without secret should reject");

        assert!(error.contains("nostrConnect URI"));
    }
}
