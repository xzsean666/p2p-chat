use crate::app::shell_auth;
use crate::infra::{media_store, shell_state_store};
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::Value as JsonValue;
use sha2::Digest;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::Runtime;

const MEDIA_UPLOAD_DRIVER_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_DRIVER";
const MEDIA_UPLOAD_ENDPOINT_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_ENDPOINT";
const MEDIA_UPLOAD_BEARER_TOKEN_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_BEARER_TOKEN";
const MEDIA_UPLOAD_FILE_FIELD_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_FILE_FIELD";
const MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_ACCESS_KEY";
const MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_SECRET_KEY";
const MEDIA_UPLOAD_MINIO_BUCKET_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_BUCKET";
const MEDIA_UPLOAD_MINIO_REGION_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_REGION";
const MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_SESSION_TOKEN";
const MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_PATH_PREFIX";
const MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV: &str = "P2P_CHAT_MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL";

enum MediaUploadBackend {
    LocalFileServer,
    Filedrop(FiledropUploadConfig),
    Nip96(Nip96UploadConfig),
    Blossom(BlossomUploadConfig),
    Minio(MinioUploadConfig),
}

struct FiledropUploadConfig {
    endpoint: String,
    bearer_token: Option<String>,
    file_field: String,
}

enum Nip96UploadTarget {
    Metadata { url: String },
    DirectApi { url: String },
}

struct Nip96UploadConfig {
    target: Nip96UploadTarget,
}

struct BlossomUploadConfig {
    upload_url: String,
}

struct MinioUploadConfig {
    upload_base_url: reqwest::Url,
    public_base_url: Option<reqwest::Url>,
    access_key: String,
    secret_key: String,
    bucket: String,
    region: String,
    session_token: Option<String>,
    path_prefix: Option<String>,
}

struct ResolvedNip96UploadTarget {
    api_url: String,
    is_nip98_required: bool,
}

struct PreparedUploadPayload {
    bytes: Vec<u8>,
    file_name: String,
    mime_type: &'static str,
}

#[derive(Debug, Default)]
struct PersistedMediaUploadPreference {
    driver: Option<String>,
    endpoint: Option<String>,
}

pub fn resolve_outbound_chat_media_remote_url<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    local_path: &str,
    file_name: &str,
) -> Result<String, String> {
    match media_upload_backend(app_handle)? {
        MediaUploadBackend::LocalFileServer => media_store::local_chat_media_file_url(local_path),
        MediaUploadBackend::Filedrop(config) => {
            upload_with_filedrop(local_path, file_name, &config)
        }
        MediaUploadBackend::Nip96(config) => {
            upload_with_nip96(app_handle, local_path, file_name, &config)
        }
        MediaUploadBackend::Blossom(config) => {
            upload_with_blossom(app_handle, local_path, file_name, &config)
        }
        MediaUploadBackend::Minio(config) => upload_with_minio(local_path, file_name, &config),
    }
}

fn media_upload_backend<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<MediaUploadBackend, String> {
    let persisted = load_persisted_media_upload_preference(app_handle)?;
    if let Some(backend) = media_upload_backend_from_preference(&persisted)? {
        return Ok(backend);
    }

    media_upload_backend_from_env()
}

fn load_persisted_media_upload_preference<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
) -> Result<PersistedMediaUploadPreference, String> {
    let Some(shell) = shell_state_store::load(app_handle)? else {
        return Ok(PersistedMediaUploadPreference::default());
    };
    let advanced = shell
        .get("advancedPreferences")
        .and_then(JsonValue::as_object);

    Ok(PersistedMediaUploadPreference {
        driver: advanced
            .and_then(|value| value.get("mediaUploadDriver"))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_ascii_lowercase()),
        endpoint: advanced
            .and_then(|value| value.get("mediaUploadEndpoint"))
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

fn media_upload_backend_from_preference(
    preference: &PersistedMediaUploadPreference,
) -> Result<Option<MediaUploadBackend>, String> {
    match preference.driver.as_deref() {
        None | Some("auto") => Ok(None),
        Some("local") | Some("preview") => Ok(Some(MediaUploadBackend::LocalFileServer)),
        Some("filedrop") | Some("multipart") | Some("httpmultipart") => {
            let endpoint = preference.endpoint.clone().ok_or_else(|| {
                "media upload endpoint is required when persisted mediaUploadDriver is filedrop"
                    .to_string()
            })?;
            Ok(Some(MediaUploadBackend::Filedrop(build_filedrop_config(
                endpoint,
            )?)))
        }
        Some("nip96") => {
            let endpoint = preference.endpoint.clone().ok_or_else(|| {
                "media upload endpoint is required when persisted mediaUploadDriver is nip96"
                    .to_string()
            })?;
            Ok(Some(MediaUploadBackend::Nip96(build_nip96_config(
                endpoint,
            )?)))
        }
        Some("blossom") => {
            let endpoint = preference.endpoint.clone().ok_or_else(|| {
                "media upload endpoint is required when persisted mediaUploadDriver is blossom"
                    .to_string()
            })?;
            Ok(Some(MediaUploadBackend::Blossom(build_blossom_config(
                endpoint,
            )?)))
        }
        Some("minio") => {
            let endpoint = preference.endpoint.clone().ok_or_else(|| {
                "media upload endpoint is required when persisted mediaUploadDriver is minio"
                    .to_string()
            })?;
            Ok(Some(MediaUploadBackend::Minio(build_minio_config(
                endpoint,
            )?)))
        }
        Some(other) => Err(format!(
            "unsupported persisted media upload driver: {other}"
        )),
    }
}

fn media_upload_backend_from_env() -> Result<MediaUploadBackend, String> {
    let driver = env_var_trim(MEDIA_UPLOAD_DRIVER_ENV).map(|value| value.to_ascii_lowercase());
    let endpoint = env_var_trim(MEDIA_UPLOAD_ENDPOINT_ENV);

    match driver.as_deref() {
        None => match endpoint {
            Some(endpoint) => Ok(MediaUploadBackend::Filedrop(build_filedrop_config(
                endpoint,
            )?)),
            None => Ok(MediaUploadBackend::LocalFileServer),
        },
        Some("local") | Some("preview") => Ok(MediaUploadBackend::LocalFileServer),
        Some("auto") => match endpoint {
            Some(endpoint) => Ok(MediaUploadBackend::Filedrop(build_filedrop_config(
                endpoint,
            )?)),
            None => Ok(MediaUploadBackend::LocalFileServer),
        },
        Some("filedrop") | Some("multipart") | Some("httpmultipart") => {
            let endpoint = endpoint.ok_or_else(|| {
                format!(
                    "{MEDIA_UPLOAD_ENDPOINT_ENV} is required when {MEDIA_UPLOAD_DRIVER_ENV} is set to {}",
                    driver.as_deref().unwrap_or_default()
                )
            })?;
            Ok(MediaUploadBackend::Filedrop(build_filedrop_config(
                endpoint,
            )?))
        }
        Some("nip96") => {
            let endpoint = endpoint.ok_or_else(|| {
                format!(
                    "{MEDIA_UPLOAD_ENDPOINT_ENV} is required when {MEDIA_UPLOAD_DRIVER_ENV} is set to nip96"
                )
            })?;
            Ok(MediaUploadBackend::Nip96(build_nip96_config(endpoint)?))
        }
        Some("blossom") => {
            let endpoint = endpoint.ok_or_else(|| {
                format!(
                    "{MEDIA_UPLOAD_ENDPOINT_ENV} is required when {MEDIA_UPLOAD_DRIVER_ENV} is set to blossom"
                )
            })?;
            Ok(MediaUploadBackend::Blossom(build_blossom_config(endpoint)?))
        }
        Some("minio") => {
            let endpoint = endpoint.ok_or_else(|| {
                format!(
                    "{MEDIA_UPLOAD_ENDPOINT_ENV} is required when {MEDIA_UPLOAD_DRIVER_ENV} is set to minio"
                )
            })?;
            Ok(MediaUploadBackend::Minio(build_minio_config(endpoint)?))
        }
        Some(other) => Err(format!("unsupported media upload driver: {other}")),
    }
}

fn build_filedrop_config(endpoint: String) -> Result<FiledropUploadConfig, String> {
    let mut parsed = reqwest::Url::parse(&endpoint).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("media upload endpoint must use http or https".into()),
    }
    if parsed.path().is_empty() || parsed.path() == "/" {
        parsed = parsed.join("upload").map_err(|error| error.to_string())?;
    }

    Ok(FiledropUploadConfig {
        endpoint: parsed.to_string(),
        bearer_token: env_var_trim(MEDIA_UPLOAD_BEARER_TOKEN_ENV),
        file_field: env_var_trim(MEDIA_UPLOAD_FILE_FIELD_ENV).unwrap_or_else(|| "file".into()),
    })
}

fn build_nip96_config(endpoint: String) -> Result<Nip96UploadConfig, String> {
    let parsed = parse_http_url(&endpoint, "media upload endpoint")?;
    let path = parsed.path().trim();

    if path.is_empty() || path == "/" {
        return Ok(Nip96UploadConfig {
            target: Nip96UploadTarget::Metadata {
                url: nip96_metadata_url_from_origin(&parsed)?.to_string(),
            },
        });
    }

    if path == "/.well-known/nostr/nip96.json" {
        return Ok(Nip96UploadConfig {
            target: Nip96UploadTarget::Metadata {
                url: parsed.to_string(),
            },
        });
    }

    Ok(Nip96UploadConfig {
        target: Nip96UploadTarget::DirectApi {
            url: parsed.to_string(),
        },
    })
}

fn build_blossom_config(endpoint: String) -> Result<BlossomUploadConfig, String> {
    let mut parsed = parse_http_url(&endpoint, "media upload endpoint")?;
    if parsed.path().trim().is_empty() || parsed.path() == "/" {
        parsed = parsed.join("upload").map_err(|error| error.to_string())?;
    }

    Ok(BlossomUploadConfig {
        upload_url: parsed.to_string(),
    })
}

fn build_minio_config(endpoint: String) -> Result<MinioUploadConfig, String> {
    let mut upload_base_url = parse_http_url(&endpoint, "media upload endpoint")?;
    upload_base_url.set_query(None);
    upload_base_url.set_fragment(None);

    let access_key = env_var_trim(MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV).ok_or_else(|| {
        format!("{MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV} is required when media upload driver is minio")
    })?;
    let secret_key = env_var_trim(MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV).ok_or_else(|| {
        format!("{MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV} is required when media upload driver is minio")
    })?;
    let bucket = env_var_trim(MEDIA_UPLOAD_MINIO_BUCKET_ENV).ok_or_else(|| {
        format!("{MEDIA_UPLOAD_MINIO_BUCKET_ENV} is required when media upload driver is minio")
    })?;
    let public_base_url = env_var_trim(MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV)
        .map(|value| parse_http_url(&value, "MinIO public base url"))
        .transpose()?;
    let region = env_var_trim(MEDIA_UPLOAD_MINIO_REGION_ENV).unwrap_or_else(|| "us-east-1".into());
    let path_prefix = env_var_trim(MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV)
        .map(normalize_storage_path_prefix)
        .transpose()?;

    Ok(MinioUploadConfig {
        upload_base_url,
        public_base_url,
        access_key,
        secret_key,
        bucket,
        region,
        session_token: env_var_trim(MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV),
        path_prefix,
    })
}

fn env_var_trim(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn upload_with_filedrop(
    local_path: &str,
    file_name: &str,
    config: &FiledropUploadConfig,
) -> Result<String, String> {
    let payload = prepare_upload_payload(local_path, file_name)?;
    let part = Part::bytes(payload.bytes)
        .file_name(payload.file_name)
        .mime_str(payload.mime_type)
        .map_err(|error| error.to_string())?;
    let form = Form::new().part(config.file_field.clone(), part);
    let client = build_http_client()?;
    let mut request = client.post(&config.endpoint).multipart(form);
    if let Some(token) = &config.bearer_token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = request
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?;
    let body = response.text().map_err(|error| error.to_string())?;
    extract_upload_url(&body)
        .ok_or_else(|| "media upload response did not include a remote url".to_string())
}

fn upload_with_nip96(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    local_path: &str,
    file_name: &str,
    config: &Nip96UploadConfig,
) -> Result<String, String> {
    let payload = prepare_upload_payload(local_path, file_name)?;
    let client = build_http_client()?;
    let target = resolve_nip96_upload_target(&client, config)?;
    let authorization = if target.is_nip98_required {
        Some(
            shell_auth::build_auth_runtime_nip98_http_auth_header(
                app_handle,
                &target.api_url,
                &payload.bytes,
            )
            .map_err(|error| {
                format!("failed to build NIP-98 auth header for NIP-96 upload: {error}")
            })?,
        )
    } else {
        None
    };
    let PreparedUploadPayload {
        bytes,
        file_name,
        mime_type,
    } = payload;
    let part = Part::bytes(bytes)
        .file_name(file_name)
        .mime_str(mime_type)
        .map_err(|error| error.to_string())?;
    let form = Form::new().part("file", part);
    let mut request = client.post(&target.api_url).multipart(form);
    if let Some(authorization) = authorization {
        request = request.header(AUTHORIZATION, authorization);
    }
    let response = request
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to upload media to NIP-96 server: {error}"))?;
    let body = response.text().map_err(|error| error.to_string())?;
    extract_upload_url(&body)
        .ok_or_else(|| "media upload response did not include a remote url".to_string())
}

fn upload_with_blossom(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    local_path: &str,
    file_name: &str,
    config: &BlossomUploadConfig,
) -> Result<String, String> {
    let PreparedUploadPayload {
        bytes,
        file_name: _file_name,
        mime_type,
    } = prepare_upload_payload(local_path, file_name)?;
    let client = build_http_client()?;
    let authorization = shell_auth::build_auth_runtime_blossom_upload_auth_header(
        app_handle,
        &config.upload_url,
        &bytes,
    )
    .map_err(|error| format!("failed to build Blossom auth header: {error}"))?;
    let response = client
        .put(&config.upload_url)
        .header(AUTHORIZATION, authorization)
        .header(CONTENT_TYPE, mime_type)
        .body(bytes)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to upload media to Blossom server: {error}"))?;
    let body = response.text().map_err(|error| error.to_string())?;
    extract_upload_url(&body)
        .ok_or_else(|| "media upload response did not include a remote url".to_string())
}

fn upload_with_minio(
    local_path: &str,
    file_name: &str,
    config: &MinioUploadConfig,
) -> Result<String, String> {
    let payload = prepare_upload_payload(local_path, file_name)?;
    let payload_hash = encode_lower_hex(&sha2::Sha256::digest(&payload.bytes));
    let object_key = build_minio_object_key(&payload, &payload_hash, config.path_prefix.as_deref());
    let object_url = minio_object_url(&config.upload_base_url, &config.bucket, &object_key)?;
    let host = canonical_host_header(&object_url)?;
    let amz_date = current_amz_date();
    let date_scope = &amz_date[..8];
    let credential_scope = format!("{date_scope}/{}/s3/aws4_request", config.region);
    let canonical_uri = object_url.path().to_string();
    let signed_headers = if config.session_token.is_some() {
        "host;x-amz-content-sha256;x-amz-date;x-amz-security-token"
    } else {
        "host;x-amz-content-sha256;x-amz-date"
    };
    let canonical_headers = if let Some(session_token) = &config.session_token {
        format!(
            "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\nx-amz-security-token:{session_token}\n"
        )
    } else {
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n")
    };
    let canonical_request =
        format!("PUT\n{canonical_uri}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}");
    let canonical_request_hash =
        encode_lower_hex(&sha2::Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign =
        format!("AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{canonical_request_hash}");
    let signature = encode_lower_hex(&aws_sigv4_signing_key(
        &config.secret_key,
        date_scope,
        &config.region,
        "s3",
        string_to_sign.as_bytes(),
    ));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        config.access_key
    );

    let client = build_http_client()?;
    let mut request = client
        .put(object_url.clone())
        .header(CONTENT_TYPE, payload.mime_type)
        .header("x-amz-content-sha256", payload_hash)
        .header("x-amz-date", amz_date)
        .header(AUTHORIZATION, authorization)
        .body(payload.bytes);
    if let Some(session_token) = &config.session_token {
        request = request.header("x-amz-security-token", session_token);
    }
    request
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to upload media to MinIO server: {error}"))?;

    minio_public_url(config, &object_key)
}

fn prepare_upload_payload(
    local_path: &str,
    file_name: &str,
) -> Result<PreparedUploadPayload, String> {
    let normalized_path = local_path.trim();
    if normalized_path.is_empty() {
        return Err("local media path is empty".into());
    }

    let path = Path::new(normalized_path);
    if !path.is_file() {
        return Err("local media path must point to an existing file".into());
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("media payload is empty".into());
    }

    Ok(PreparedUploadPayload {
        bytes,
        file_name: normalized_upload_file_name(path, file_name),
        mime_type: guess_mime_type(path),
    })
}

fn build_http_client() -> Result<BlockingHttpClient, String> {
    BlockingHttpClient::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())
}

fn resolve_nip96_upload_target(
    client: &BlockingHttpClient,
    config: &Nip96UploadConfig,
) -> Result<ResolvedNip96UploadTarget, String> {
    match &config.target {
        Nip96UploadTarget::Metadata { url } => discover_nip96_upload_target(client, url),
        Nip96UploadTarget::DirectApi { url } => Ok(ResolvedNip96UploadTarget {
            api_url: url.clone(),
            is_nip98_required: false,
        }),
    }
}

fn discover_nip96_upload_target(
    client: &BlockingHttpClient,
    metadata_url: &str,
) -> Result<ResolvedNip96UploadTarget, String> {
    let response = client
        .get(metadata_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("failed to fetch NIP-96 metadata: {error}"))?;
    let body = response.text().map_err(|error| error.to_string())?;
    let value = serde_json::from_str::<JsonValue>(&body)
        .map_err(|error| format!("failed to parse NIP-96 metadata: {error}"))?;
    let api_url = value
        .get("api_url")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "NIP-96 metadata response did not include api_url".to_string())?;

    Ok(ResolvedNip96UploadTarget {
        api_url: resolve_http_url(api_url, Some(metadata_url), "NIP-96 api_url")?,
        is_nip98_required: value
            .get("plans")
            .and_then(JsonValue::as_object)
            .and_then(|plans| plans.get("free"))
            .and_then(JsonValue::as_object)
            .and_then(|plan| plan.get("is_nip98_required"))
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
    })
}

fn parse_http_url(value: &str, label: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(value.trim()).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        _ => Err(format!("{label} must use http or https")),
    }
}

fn resolve_http_url(value: &str, base_url: Option<&str>, label: &str) -> Result<String, String> {
    if let Ok(parsed) = parse_http_url(value, label) {
        return Ok(parsed.to_string());
    }

    let Some(base_url) = base_url else {
        return Err(format!("{label} must use http or https"));
    };
    let base = parse_http_url(base_url, label)?;
    let parsed = base.join(value).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed.to_string()),
        _ => Err(format!("{label} must use http or https")),
    }
}

fn nip96_metadata_url_from_origin(origin: &reqwest::Url) -> Result<reqwest::Url, String> {
    let mut normalized = origin.clone();
    normalized.set_path("/");
    normalized.set_query(None);
    normalized.set_fragment(None);
    normalized
        .join(".well-known/nostr/nip96.json")
        .map_err(|error| error.to_string())
}

fn normalized_upload_file_name(path: &Path, file_name: &str) -> String {
    let normalized = file_name.trim();
    if !normalized.is_empty() {
        return normalized.into();
    }

    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("upload.bin")
        .to_string()
}

fn guess_mime_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    match extension.as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("heic") => "image/heic",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("zip") => "application/zip",
        _ => "application/octet-stream",
    }
}

fn extract_upload_url(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_http_url(trimmed) {
        return Some(trimmed.to_string());
    }

    let value = serde_json::from_str::<JsonValue>(trimmed).ok()?;
    extract_upload_url_from_json(&value)
}

fn extract_upload_url_from_json(value: &JsonValue) -> Option<String> {
    if let Some(url) = value.get("url").and_then(json_http_url) {
        return Some(url);
    }
    if let Some(url) = value.get("fileUrl").and_then(json_http_url) {
        return Some(url);
    }
    if let Some(url) = value.get("data").and_then(extract_upload_url_from_json) {
        return Some(url);
    }
    if let Some(url) = value
        .get("nip94_event")
        .and_then(extract_upload_url_from_json)
    {
        return Some(url);
    }

    match value {
        JsonValue::String(url) => is_http_url(url).then(|| url.clone()),
        JsonValue::Array(items) => items.iter().find_map(extract_upload_url_from_json),
        JsonValue::Object(map) => map.values().find_map(extract_upload_url_from_json),
        _ => None,
    }
}

fn json_http_url(value: &JsonValue) -> Option<String> {
    value
        .as_str()
        .filter(|value| is_http_url(value))
        .map(str::to_string)
}

fn is_http_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .ok()
        .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn build_minio_object_key(
    payload: &PreparedUploadPayload,
    payload_hash: &str,
    path_prefix: Option<&str>,
) -> String {
    let folder = minio_object_folder(payload.mime_type);
    let extension = Path::new(&payload.file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let file_name = extension
        .map(|extension| format!("{payload_hash}.{extension}"))
        .unwrap_or_else(|| payload_hash.to_string());

    match path_prefix {
        Some(prefix) => format!("{prefix}/{folder}/{file_name}"),
        None => format!("{folder}/{file_name}"),
    }
}

fn minio_object_folder(mime_type: &str) -> &'static str {
    if mime_type.starts_with("image/") {
        "images"
    } else if mime_type.starts_with("video/") {
        "videos"
    } else {
        "files"
    }
}

fn minio_object_url(
    base_url: &reqwest::Url,
    bucket: &str,
    object_key: &str,
) -> Result<reqwest::Url, String> {
    let mut url = base_url.clone();
    let mut segments = url.path_segments_mut().map_err(|_| {
        "MinIO media upload endpoint must not be a cannot-be-a-base URL".to_string()
    })?;
    segments.push(bucket);
    for segment in object_key.split('/').filter(|segment| !segment.is_empty()) {
        segments.push(segment);
    }
    drop(segments);
    Ok(url)
}

fn minio_public_url(config: &MinioUploadConfig, object_key: &str) -> Result<String, String> {
    let url = match &config.public_base_url {
        Some(base_url) => {
            let mut url = base_url.clone();
            let mut segments = url.path_segments_mut().map_err(|_| {
                "MinIO public base url must not be a cannot-be-a-base URL".to_string()
            })?;
            for segment in object_key.split('/').filter(|segment| !segment.is_empty()) {
                segments.push(segment);
            }
            drop(segments);
            url
        }
        None => minio_object_url(&config.upload_base_url, &config.bucket, object_key)?,
    };
    Ok(url.to_string())
}

fn normalize_storage_path_prefix(value: String) -> Result<String, String> {
    let normalized = value
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.trim().is_empty())
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() {
        return Err("MinIO path prefix must not be empty when provided".into());
    }
    Ok(normalized)
}

fn canonical_host_header(url: &reqwest::Url) -> Result<String, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "MinIO upload endpoint must include a host".to_string())?;
    let host = match url.port() {
        Some(port)
            if !(url.scheme() == "http" && port == 80
                || url.scheme() == "https" && port == 443) =>
        {
            format!("{host}:{port}")
        }
        _ => host.to_string(),
    };
    Ok(host)
}

fn current_amz_date() -> String {
    let value = nostr_connect::prelude::Timestamp::now().to_human_datetime();
    let compact = value.replace('-', "").replace(':', "");
    compact
}

fn aws_sigv4_signing_key(
    secret_key: &str,
    date_scope: &str,
    region: &str,
    service: &str,
    string_to_sign: &[u8],
) -> [u8; 32] {
    let date_key = hmac_sha256(
        format!("AWS4{secret_key}").as_bytes(),
        date_scope.as_bytes(),
    );
    let region_key = hmac_sha256(&date_key, region.as_bytes());
    let service_key = hmac_sha256(&region_key, service.as_bytes());
    let signing_key = hmac_sha256(&service_key, b"aws4_request");
    hmac_sha256(&signing_key, string_to_sign)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut normalized_key = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        normalized_key[..32].copy_from_slice(&sha2::Sha256::digest(key));
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }

    let mut inner_key = [0_u8; BLOCK_SIZE];
    let mut outer_key = [0_u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        inner_key[index] = normalized_key[index] ^ 0x36;
        outer_key[index] = normalized_key[index] ^ 0x5c;
    }

    let mut inner = sha2::Sha256::new();
    inner.update(inner_key);
    inner.update(data);
    let inner_digest = inner.finalize();

    let mut outer = sha2::Sha256::new();
    outer.update(outer_key);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::auth_access;
    use crate::domain::chat::{
        default_advanced_preferences, default_app_preferences, default_notification_preferences,
        default_user_profile, AuthRuntimeState, AuthRuntimeSummary, AuthSessionSummary,
        ChatMediaKind, LoginAccessInput, LoginAccessKind, LoginAccessSummary,
        LoginCircleSelectionMode, LoginMethod, ShellStateSnapshot, StoreChatMediaAssetInput,
    };
    use crate::infra::auth_runtime_binding_store::{self, StoredAuthRuntimeBinding};
    use crate::infra::auth_runtime_credential_store::{self, StoredAuthRuntimeCredential};
    use crate::infra::media_store::store_chat_media_asset;
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use nostr_connect::prelude::{
        nip44, Event as NostrEvent, EventBuilder as NostrEventBuilder, JsonUtil, Keys as NostrKeys,
        Kind as NostrKind, NostrConnectMessage, NostrConnectRequest, NostrConnectResponse,
        PublicKey as NostrPublicKey, ResponseResult, ToBech32,
    };
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::sync::MutexGuard;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tungstenite::protocol::Message as WebSocketMessage;

    const TEST_LOCAL_SECRET_KEY: &str =
        "1111111111111111111111111111111111111111111111111111111111111111";
    const TEST_BUNKER_SIGNER_SECRET_KEY: &str =
        "2222222222222222222222222222222222222222222222222222222222222222";
    const TEST_BUNKER_USER_SECRET_KEY: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";
    const TEST_BUNKER_SHARED_SECRET: &str = "shared-secret";
    const TEST_AUTH_LOGGED_IN_AT: &str = "2026-04-21T10:00:00Z";
    const SHA256_HELLO_HEX: &str =
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    struct TestHttpResponse {
        status_line: &'static str,
        content_type: &'static str,
        body: String,
    }

    impl TestHttpResponse {
        fn json(body: &str) -> Self {
            Self {
                status_line: "200 OK",
                content_type: "application/json",
                body: body.to_string(),
            }
        }
    }

    struct TestAppGuard {
        _env_guard: MutexGuard<'static, ()>,
        app: tauri::App<tauri::test::MockRuntime>,
        config_root: PathBuf,
        previous_xdg_config_home: Option<String>,
        previous_upload_driver: Option<String>,
        previous_upload_endpoint: Option<String>,
        previous_upload_bearer_token: Option<String>,
        previous_upload_file_field: Option<String>,
        previous_minio_access_key: Option<String>,
        previous_minio_secret_key: Option<String>,
        previous_minio_bucket: Option<String>,
        previous_minio_region: Option<String>,
        previous_minio_session_token: Option<String>,
        previous_minio_path_prefix: Option<String>,
        previous_minio_public_base_url: Option<String>,
    }

    impl Drop for TestAppGuard {
        fn drop(&mut self) {
            restore_env_var("XDG_CONFIG_HOME", &self.previous_xdg_config_home);
            restore_env_var(MEDIA_UPLOAD_DRIVER_ENV, &self.previous_upload_driver);
            restore_env_var(MEDIA_UPLOAD_ENDPOINT_ENV, &self.previous_upload_endpoint);
            restore_env_var(
                MEDIA_UPLOAD_BEARER_TOKEN_ENV,
                &self.previous_upload_bearer_token,
            );
            restore_env_var(
                MEDIA_UPLOAD_FILE_FIELD_ENV,
                &self.previous_upload_file_field,
            );
            restore_env_var(
                MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV,
                &self.previous_minio_access_key,
            );
            restore_env_var(
                MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV,
                &self.previous_minio_secret_key,
            );
            restore_env_var(MEDIA_UPLOAD_MINIO_BUCKET_ENV, &self.previous_minio_bucket);
            restore_env_var(MEDIA_UPLOAD_MINIO_REGION_ENV, &self.previous_minio_region);
            restore_env_var(
                MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV,
                &self.previous_minio_session_token,
            );
            restore_env_var(
                MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV,
                &self.previous_minio_path_prefix,
            );
            restore_env_var(
                MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV,
                &self.previous_minio_public_base_url,
            );

            let _ = fs::remove_dir_all(&self.config_root);
        }
    }

    fn restore_env_var(key: &str, value: &Option<String>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
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
        let config_root = std::env::temp_dir().join(format!("p2p-chat-media-upload-test-{unique}"));
        fs::create_dir_all(&config_root).expect("failed to create test config root");

        let previous_xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
        let previous_upload_driver = std::env::var(MEDIA_UPLOAD_DRIVER_ENV).ok();
        let previous_upload_endpoint = std::env::var(MEDIA_UPLOAD_ENDPOINT_ENV).ok();
        let previous_upload_bearer_token = std::env::var(MEDIA_UPLOAD_BEARER_TOKEN_ENV).ok();
        let previous_upload_file_field = std::env::var(MEDIA_UPLOAD_FILE_FIELD_ENV).ok();
        let previous_minio_access_key = std::env::var(MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV).ok();
        let previous_minio_secret_key = std::env::var(MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV).ok();
        let previous_minio_bucket = std::env::var(MEDIA_UPLOAD_MINIO_BUCKET_ENV).ok();
        let previous_minio_region = std::env::var(MEDIA_UPLOAD_MINIO_REGION_ENV).ok();
        let previous_minio_session_token = std::env::var(MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV).ok();
        let previous_minio_path_prefix = std::env::var(MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV).ok();
        let previous_minio_public_base_url =
            std::env::var(MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV).ok();
        std::env::set_var("XDG_CONFIG_HOME", &config_root);
        std::env::remove_var(MEDIA_UPLOAD_DRIVER_ENV);
        std::env::remove_var(MEDIA_UPLOAD_ENDPOINT_ENV);
        std::env::remove_var(MEDIA_UPLOAD_BEARER_TOKEN_ENV);
        std::env::remove_var(MEDIA_UPLOAD_FILE_FIELD_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_BUCKET_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_REGION_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV);
        std::env::remove_var(MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV);

        let app = tauri::test::mock_app();
        TestAppGuard {
            _env_guard: env_guard,
            app,
            config_root,
            previous_xdg_config_home,
            previous_upload_driver,
            previous_upload_endpoint,
            previous_upload_bearer_token,
            previous_upload_file_field,
            previous_minio_access_key,
            previous_minio_secret_key,
            previous_minio_bucket,
            previous_minio_region,
            previous_minio_session_token,
            previous_minio_path_prefix,
            previous_minio_public_base_url,
        }
    }

    fn persist_shell_media_upload_preference(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        driver: &str,
        endpoint: &str,
    ) {
        let mut shell = shell_state_store::load(app_handle)
            .expect("shell state should load")
            .unwrap_or_else(|| serde_json::json!({}));
        if !shell.is_object() {
            shell = serde_json::json!({});
        }
        let shell_object = shell
            .as_object_mut()
            .expect("shell state root should be an object");
        let advanced = shell_object
            .entry("advancedPreferences")
            .or_insert_with(|| serde_json::json!({}));
        if !advanced.is_object() {
            *advanced = serde_json::json!({});
        }
        let advanced_object = advanced
            .as_object_mut()
            .expect("advanced preferences should be an object");
        advanced_object.insert("mediaUploadDriver".into(), serde_json::json!(driver));
        advanced_object.insert("mediaUploadEndpoint".into(), serde_json::json!(endpoint));

        shell_state_store::save(app_handle, shell).expect("shell state should persist");
    }

    fn persist_shell_snapshot(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        shell: &ShellStateSnapshot,
    ) {
        shell_state_store::save(
            app_handle,
            serde_json::to_value(shell).expect("shell snapshot should serialize"),
        )
        .expect("shell state should persist");
    }

    fn default_shell_state_snapshot() -> ShellStateSnapshot {
        ShellStateSnapshot {
            is_authenticated: false,
            auth_session: None,
            auth_runtime: None,
            auth_runtime_binding: None,
            user_profile: default_user_profile(),
            restorable_circles: vec![],
            app_preferences: default_app_preferences(),
            notification_preferences: default_notification_preferences(),
            advanced_preferences: default_advanced_preferences(),
            active_circle_id: String::new(),
            selected_session_id: String::new(),
        }
    }

    fn persist_authenticated_local_secret_shell(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
    ) -> String {
        let credential = auth_access::resolve_auth_runtime_credential(&LoginAccessInput {
            kind: LoginAccessKind::HexKey,
            value: Some(TEST_LOCAL_SECRET_KEY.into()),
        })
        .expect("local secret credential should resolve")
        .expect("local secret credential should exist");
        let mut shell = default_shell_state_snapshot();
        shell.is_authenticated = true;
        shell.auth_session = Some(AuthSessionSummary {
            login_method: LoginMethod::ExistingAccount,
            access: LoginAccessSummary {
                kind: LoginAccessKind::HexKey,
                label: "Local Secret".into(),
                pubkey: Some(credential.pubkey.clone()),
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: TEST_AUTH_LOGGED_IN_AT.into(),
        });
        shell.auth_runtime = Some(AuthRuntimeSummary {
            state: AuthRuntimeState::Connected,
            login_method: LoginMethod::ExistingAccount,
            access_kind: LoginAccessKind::HexKey,
            label: "Local Secret".into(),
            pubkey: Some(credential.pubkey.clone()),
            error: None,
            can_send_messages: true,
            send_blocked_reason: None,
            persisted_in_native_store: false,
            credential_persisted_in_native_store: true,
            updated_at: TEST_AUTH_LOGGED_IN_AT.into(),
        });
        persist_shell_snapshot(app_handle, &shell);
        auth_runtime_credential_store::save(
            app_handle,
            &StoredAuthRuntimeCredential {
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                secret_key_hex: credential.secret_key_hex.clone(),
                pubkey: credential.pubkey.clone(),
                stored_at: TEST_AUTH_LOGGED_IN_AT.into(),
            },
        )
        .expect("local secret credential store should persist");

        credential.pubkey
    }

    fn bunker_signer_public_key_hex() -> String {
        NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
            .expect("test bunker signer secret should parse")
            .public_key()
            .to_hex()
    }

    fn bunker_user_public_key_hex() -> String {
        NostrKeys::parse(TEST_BUNKER_USER_SECRET_KEY)
            .expect("test bunker user secret should parse")
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

    fn persist_connected_bunker_shell(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        relay_url: &str,
    ) -> String {
        let signer_pubkey = bunker_signer_public_key_hex();
        let user_npub = bunker_user_public_key_npub();
        let mut shell = default_shell_state_snapshot();
        shell.is_authenticated = true;
        shell.auth_session = Some(AuthSessionSummary {
            login_method: LoginMethod::Signer,
            access: LoginAccessSummary {
                kind: LoginAccessKind::Bunker,
                label: "bunker://signer.example".into(),
                pubkey: None,
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: TEST_AUTH_LOGGED_IN_AT.into(),
        });
        shell.auth_runtime = Some(AuthRuntimeSummary {
            state: AuthRuntimeState::Connected,
            login_method: LoginMethod::Signer,
            access_kind: LoginAccessKind::Bunker,
            label: "bunker://signer.example".into(),
            pubkey: Some(user_npub),
            error: None,
            can_send_messages: true,
            send_blocked_reason: None,
            persisted_in_native_store: false,
            credential_persisted_in_native_store: false,
            updated_at: TEST_AUTH_LOGGED_IN_AT.into(),
        });
        persist_shell_snapshot(app_handle, &shell);
        auth_runtime_binding_store::save(
            app_handle,
            &StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: TEST_AUTH_LOGGED_IN_AT.into(),
            },
        )
        .expect("remote bunker binding should persist");

        bunker_user_public_key_hex()
    }

    fn spawn_upload_server(
        response_body: &str,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        spawn_scripted_http_server(vec![TestHttpResponse::json(response_body)])
    }

    fn spawn_scripted_http_server(
        responses: Vec<TestHttpResponse>,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("server should bind");
        let address = listener
            .local_addr()
            .expect("server address should resolve");
        let (request_tx, request_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener
                    .accept()
                    .expect("server should accept scripted request");
                let request = read_http_request(&mut stream);
                request_tx
                    .send(request)
                    .expect("request should be sent to test");
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status_line,
                    response.content_type,
                    response.body.len(),
                    response.body
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("response should be written");
            }
        });

        (format!("http://{address}"), request_rx, handle)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];

        loop {
            let read = stream.read(&mut buffer).expect("request should read");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = bytes
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map(|index| index + 4)
            {
                let headers = String::from_utf8_lossy(&bytes[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if !name.eq_ignore_ascii_case("content-length") {
                            return None;
                        }
                        value.trim().parse::<usize>().ok()
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn request_header_value(request: &str, header_name: &str) -> Option<String> {
        request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(header_name)
                .then(|| value.trim().to_string())
        })
    }

    fn decode_authorization_event(request: &str) -> NostrEvent {
        let authorization = request_header_value(request, "Authorization")
            .expect("upload request should include authorization header");
        let encoded = authorization
            .strip_prefix("Nostr ")
            .expect("authorization header should use Nostr prefix");
        let body = BASE64_STANDARD
            .decode(encoded)
            .expect("authorization header should decode");
        let event = NostrEvent::from_json(body).expect("authorization event should parse");
        event
            .verify()
            .expect("authorization event signature should verify");
        event
    }

    fn set_minio_test_env(
        access_key: &str,
        secret_key: &str,
        bucket: &str,
        region: &str,
        session_token: Option<&str>,
        path_prefix: Option<&str>,
        public_base_url: Option<&str>,
    ) {
        std::env::set_var(MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV, access_key);
        std::env::set_var(MEDIA_UPLOAD_MINIO_SECRET_KEY_ENV, secret_key);
        std::env::set_var(MEDIA_UPLOAD_MINIO_BUCKET_ENV, bucket);
        std::env::set_var(MEDIA_UPLOAD_MINIO_REGION_ENV, region);
        if let Some(session_token) = session_token {
            std::env::set_var(MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV, session_token);
        } else {
            std::env::remove_var(MEDIA_UPLOAD_MINIO_SESSION_TOKEN_ENV);
        }
        if let Some(path_prefix) = path_prefix {
            std::env::set_var(MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV, path_prefix);
        } else {
            std::env::remove_var(MEDIA_UPLOAD_MINIO_PATH_PREFIX_ENV);
        }
        if let Some(public_base_url) = public_base_url {
            std::env::set_var(MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV, public_base_url);
        } else {
            std::env::remove_var(MEDIA_UPLOAD_MINIO_PUBLIC_BASE_URL_ENV);
        }
    }

    fn event_tag_value(event: &NostrEvent, key: &str) -> Option<String> {
        serde_json::to_value(event)
            .ok()
            .and_then(|value| value.get("tags").cloned())
            .and_then(|value| value.as_array().cloned())
            .and_then(|tags| {
                tags.into_iter().find_map(|tag| {
                    let tag = tag.as_array()?;
                    (tag.first()?.as_str()? == key)
                        .then(|| tag.get(1)?.as_str().map(str::to_string))
                        .flatten()
                })
            })
    }

    fn spawn_bunker_signer_relay_server() -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let handle = thread::spawn(move || {
            let signer_keys = NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
                .expect("test bunker signer secret should parse");
            let user_keys = NostrKeys::parse(TEST_BUNKER_USER_SECRET_KEY)
                .expect("test bunker user secret should parse");
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
                                        serde_json::json!(["EOSE", req_id]).to_string().into(),
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
                                            ResponseResult::GetPublicKey(user_keys.public_key()),
                                        )
                                    }
                                    NostrConnectRequest::SignEvent(unsigned_event) => {
                                        let signed_event = unsigned_event
                                            .sign_with_keys(&user_keys)
                                            .expect("relay should sign remote http auth event");
                                        NostrConnectResponse::with_result(
                                            ResponseResult::SignEvent(Box::new(signed_event)),
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
                                        serde_json::json!(["OK", event_id, true, ""])
                                            .to_string()
                                            .into(),
                                    ))
                                    .expect("relay should ack client EVENT");
                                socket
                                    .send(WebSocketMessage::Text(
                                        serde_json::json!([
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

    #[test]
    fn resolve_outbound_chat_media_remote_url_defaults_to_local_file_server() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "harbor-sunrise.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "harbor-sunrise.png",
        )
        .expect("default local media url should resolve");

        assert!(remote_url.starts_with("http://127.0.0.1:45115/chat-media/asset/"));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_configured_filedrop_endpoint() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::File,
                name: "roadmap.pdf".into(),
                data_url: "data:application/pdf;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) =
            spawn_upload_server(r#"{"url":"https://cdn.example.test/chat-media/roadmap.pdf"}"#);
        std::env::set_var(MEDIA_UPLOAD_DRIVER_ENV, "filedrop");
        std::env::set_var(MEDIA_UPLOAD_ENDPOINT_ENV, &server_url);

        let remote_url =
            resolve_outbound_chat_media_remote_url(app_handle, &stored.local_path, "roadmap.pdf")
                .expect("configured upload should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/roadmap.pdf"
        );
        assert!(request.starts_with("POST /upload HTTP/1.1\r\n"));
        assert!(request.contains("multipart/form-data"));
        assert!(request.contains("filename=\"roadmap.pdf\""));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uses_persisted_shell_upload_backend() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "sunrise.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) =
            spawn_upload_server(r#"{"url":"https://cdn.example.test/chat-media/sunrise.png"}"#);
        persist_shell_media_upload_preference(app_handle, "filedrop", &server_url);

        let remote_url =
            resolve_outbound_chat_media_remote_url(app_handle, &stored.local_path, "sunrise.png")
                .expect("persisted upload backend should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/sunrise.png"
        );
        assert!(request.starts_with("POST /upload HTTP/1.1\r\n"));
        assert!(request.contains("filename=\"sunrise.png\""));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_configured_nip96_origin() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "nip96-photo.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) = spawn_scripted_http_server(vec![
            TestHttpResponse::json(
                r#"{
                    "api_url": "/api/nip96/upload",
                    "plans": {
                        "free": {
                            "is_nip98_required": false
                        }
                    }
                }"#,
            ),
            TestHttpResponse::json(
                r#"{
                    "status": "success",
                    "nip94_event": {
                        "tags": [
                            ["url", "https://cdn.example.test/chat-media/nip96-photo.png"]
                        ]
                    }
                }"#,
            ),
        ]);
        std::env::set_var(MEDIA_UPLOAD_DRIVER_ENV, "nip96");
        std::env::set_var(MEDIA_UPLOAD_ENDPOINT_ENV, &server_url);

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "nip96-photo.png",
        )
        .expect("NIP-96 upload should resolve remote url");
        let metadata_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("metadata request should be captured");
        let upload_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/nip96-photo.png"
        );
        assert!(metadata_request.starts_with("GET /.well-known/nostr/nip96.json HTTP/1.1\r\n"));
        assert!(upload_request.starts_with("POST /api/nip96/upload HTTP/1.1\r\n"));
        assert!(upload_request.contains("filename=\"nip96-photo.png\""));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_direct_nip96_api_url() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::File,
                name: "direct-nip96.txt".into(),
                data_url: "data:text/plain;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) = spawn_upload_server(
            r#"{"url":"https://cdn.example.test/chat-media/direct-nip96.txt"}"#,
        );
        std::env::set_var(MEDIA_UPLOAD_DRIVER_ENV, "nip96");
        std::env::set_var(MEDIA_UPLOAD_ENDPOINT_ENV, format!("{server_url}/upload"));

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "direct-nip96.txt",
        )
        .expect("direct NIP-96 API upload should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/direct-nip96.txt"
        );
        assert!(request.starts_with("POST /upload HTTP/1.1\r\n"));
        assert!(request.contains("filename=\"direct-nip96.txt\""));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_nip96_server_that_requires_nip98_with_local_secret_auth(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "nip98-required.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let local_pubkey = persist_authenticated_local_secret_shell(app_handle);
        let (server_url, request_rx, handle) = spawn_scripted_http_server(vec![
            TestHttpResponse::json(
                r#"{
                    "api_url": "/api/nip96/upload",
                    "plans": {
                        "free": {
                            "is_nip98_required": true
                        }
                    }
                }"#,
            ),
            TestHttpResponse::json(
                r#"{"url":"https://cdn.example.test/chat-media/nip98-required.png"}"#,
            ),
        ]);
        persist_shell_media_upload_preference(app_handle, "nip96", &server_url);

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "nip98-required.png",
        )
        .expect("NIP-98 local secret auth upload should resolve remote url");
        let metadata_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("metadata request should be captured");
        let upload_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        let auth_event = decode_authorization_event(&upload_request);
        let expected_upload_url = format!("{server_url}/api/nip96/upload");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/nip98-required.png"
        );
        assert!(metadata_request.starts_with("GET /.well-known/nostr/nip96.json HTTP/1.1\r\n"));
        assert!(upload_request.starts_with("POST /api/nip96/upload HTTP/1.1\r\n"));
        assert_eq!(auth_event.kind, NostrKind::HttpAuth);
        assert_eq!(
            auth_event
                .pubkey
                .to_bech32()
                .expect("auth pubkey should encode"),
            local_pubkey
        );
        assert_eq!(
            event_tag_value(&auth_event, "u").as_deref(),
            Some(expected_upload_url.as_str())
        );
        assert_eq!(
            event_tag_value(&auth_event, "method").as_deref(),
            Some("POST")
        );
        assert_eq!(
            event_tag_value(&auth_event, "payload").as_deref(),
            Some(SHA256_HELLO_HEX)
        );
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_nip96_server_that_requires_nip98_with_remote_bunker_auth(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "bunker-nip98.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (relay_url, relay_handle) = spawn_bunker_signer_relay_server();
        let bunker_pubkey = persist_connected_bunker_shell(app_handle, &relay_url);
        let (server_url, request_rx, handle) = spawn_scripted_http_server(vec![
            TestHttpResponse::json(
                r#"{
                    "api_url": "/api/nip96/upload",
                    "plans": {
                        "free": {
                            "is_nip98_required": true
                        }
                    }
                }"#,
            ),
            TestHttpResponse::json(
                r#"{"url":"https://cdn.example.test/chat-media/bunker-nip98.png"}"#,
            ),
        ]);
        persist_shell_media_upload_preference(app_handle, "nip96", &server_url);

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "bunker-nip98.png",
        )
        .expect("remote bunker NIP-98 auth upload should resolve remote url");
        let metadata_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("metadata request should be captured");
        let upload_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");
        relay_handle
            .join()
            .expect("bunker relay should exit cleanly");

        let auth_event = decode_authorization_event(&upload_request);
        let expected_upload_url = format!("{server_url}/api/nip96/upload");

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/bunker-nip98.png"
        );
        assert!(metadata_request.starts_with("GET /.well-known/nostr/nip96.json HTTP/1.1\r\n"));
        assert!(upload_request.starts_with("POST /api/nip96/upload HTTP/1.1\r\n"));
        assert_eq!(auth_event.kind, NostrKind::HttpAuth);
        assert_eq!(auth_event.pubkey.to_hex(), bunker_pubkey);
        assert_eq!(
            event_tag_value(&auth_event, "u").as_deref(),
            Some(expected_upload_url.as_str())
        );
        assert_eq!(
            event_tag_value(&auth_event, "method").as_deref(),
            Some("POST")
        );
        assert_eq!(
            event_tag_value(&auth_event, "payload").as_deref(),
            Some(SHA256_HELLO_HEX)
        );
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_fails_when_nip96_server_requires_nip98_without_auth_runtime(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "no-auth-nip98.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) =
            spawn_scripted_http_server(vec![TestHttpResponse::json(
                r#"{
                    "api_url": "/api/nip96/upload",
                    "plans": {
                        "free": {
                            "is_nip98_required": true
                        }
                    }
                }"#,
            )]);
        persist_shell_media_upload_preference(app_handle, "nip96", &server_url);

        let error = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "no-auth-nip98.png",
        )
        .expect_err("NIP-98 auth should fail without authenticated runtime");
        let metadata_request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("metadata request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert!(metadata_request.starts_with("GET /.well-known/nostr/nip96.json HTTP/1.1\r\n"));
        assert!(error.contains("failed to build NIP-98 auth header"));
        assert!(error.contains("authenticated shell"));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_blossom_server_with_local_secret_auth() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "blossom-local.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let local_pubkey = persist_authenticated_local_secret_shell(app_handle);
        let (server_url, request_rx, handle) = spawn_upload_server(
            r#"{"url":"https://cdn.example.test/chat-media/blossom-local.png"}"#,
        );
        persist_shell_media_upload_preference(app_handle, "blossom", &server_url);

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "blossom-local.png",
        )
        .expect("Blossom local secret upload should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        let auth_event = decode_authorization_event(&request);

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/blossom-local.png"
        );
        assert!(request.starts_with("PUT /upload HTTP/1.1\r\n"));
        assert_eq!(
            request_header_value(&request, "Content-Type").as_deref(),
            Some("image/png")
        );
        assert_eq!(auth_event.kind, NostrKind::BlossomAuth);
        assert_eq!(
            auth_event
                .pubkey
                .to_bech32()
                .expect("auth pubkey should encode"),
            local_pubkey
        );
        assert_eq!(event_tag_value(&auth_event, "t").as_deref(), Some("upload"));
        assert_eq!(
            event_tag_value(&auth_event, "x").as_deref(),
            Some(SHA256_HELLO_HEX)
        );
        assert_eq!(event_tag_value(&auth_event, "size").as_deref(), Some("5"));
        assert!(event_tag_value(&auth_event, "expiration")
            .and_then(|value| value.parse::<u64>().ok())
            .is_some());
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_blossom_server_with_remote_bunker_auth() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "blossom-bunker.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (relay_url, relay_handle) = spawn_bunker_signer_relay_server();
        let bunker_pubkey = persist_connected_bunker_shell(app_handle, &relay_url);
        let (server_url, request_rx, handle) = spawn_upload_server(
            r#"{"url":"https://cdn.example.test/chat-media/blossom-bunker.png"}"#,
        );
        persist_shell_media_upload_preference(app_handle, "blossom", &server_url);

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "blossom-bunker.png",
        )
        .expect("Blossom bunker upload should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");
        relay_handle
            .join()
            .expect("bunker relay should exit cleanly");

        let auth_event = decode_authorization_event(&request);

        assert_eq!(
            remote_url,
            "https://cdn.example.test/chat-media/blossom-bunker.png"
        );
        assert!(request.starts_with("PUT /upload HTTP/1.1\r\n"));
        assert_eq!(auth_event.kind, NostrKind::BlossomAuth);
        assert_eq!(auth_event.pubkey.to_hex(), bunker_pubkey);
        assert_eq!(event_tag_value(&auth_event, "t").as_deref(), Some("upload"));
        assert_eq!(
            event_tag_value(&auth_event, "x").as_deref(),
            Some(SHA256_HELLO_HEX)
        );
        assert_eq!(event_tag_value(&auth_event, "size").as_deref(), Some("5"));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_fails_when_blossom_upload_requires_auth_without_authenticated_runtime(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "blossom-no-auth.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        persist_shell_media_upload_preference(app_handle, "blossom", "https://files.example.test");

        let error = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "blossom-no-auth.png",
        )
        .expect_err("Blossom auth should fail without authenticated runtime");

        assert!(error.contains("failed to build Blossom auth header"));
        assert!(error.contains("authenticated shell"));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_uploads_to_minio_server_with_env_credentials() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "minio-photo.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        let (server_url, request_rx, handle) =
            spawn_upload_server(r#"{"ignored":"minio returns no body contract here"}"#);
        persist_shell_media_upload_preference(app_handle, "minio", &server_url);
        set_minio_test_env(
            "minio-access",
            "minio-secret",
            "chat-media",
            "us-east-1",
            Some("session-token"),
            Some("tenant-a"),
            Some("https://cdn.example.test/public"),
        );

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "minio-photo.png",
        )
        .expect("MinIO upload should resolve remote url");
        let request = request_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("upload request should be captured");
        handle.join().expect("upload server should exit cleanly");

        assert_eq!(
            remote_url,
            format!("https://cdn.example.test/public/tenant-a/images/{SHA256_HELLO_HEX}.png")
        );
        assert!(request.starts_with(&format!(
            "PUT /chat-media/tenant-a/images/{SHA256_HELLO_HEX}.png HTTP/1.1\r\n"
        )));
        assert_eq!(
            request_header_value(&request, "Content-Type").as_deref(),
            Some("image/png")
        );
        assert_eq!(
            request_header_value(&request, "x-amz-content-sha256").as_deref(),
            Some(SHA256_HELLO_HEX)
        );
        assert_eq!(
            request_header_value(&request, "x-amz-security-token").as_deref(),
            Some("session-token")
        );
        let authorization = request_header_value(&request, "Authorization")
            .expect("MinIO upload should include authorization header");
        assert!(authorization.starts_with("AWS4-HMAC-SHA256 Credential=minio-access/"));
        assert!(authorization.contains("/us-east-1/s3/aws4_request"));
        assert!(authorization
            .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token"));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_fails_when_minio_credentials_are_missing() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::File,
                name: "minio-notes.txt".into(),
                data_url: "data:text/plain;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        persist_shell_media_upload_preference(app_handle, "minio", "https://minio.example.test");

        let error = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "minio-notes.txt",
        )
        .expect_err("MinIO upload should fail without credentials");

        assert!(error.contains(MEDIA_UPLOAD_MINIO_ACCESS_KEY_ENV));
    }

    #[test]
    fn resolve_outbound_chat_media_remote_url_persisted_local_overrides_env_upload_backend() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "local-only.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");
        persist_shell_media_upload_preference(app_handle, "local", "");
        std::env::set_var(MEDIA_UPLOAD_DRIVER_ENV, "filedrop");
        std::env::set_var(MEDIA_UPLOAD_ENDPOINT_ENV, "http://127.0.0.1:9");

        let remote_url = resolve_outbound_chat_media_remote_url(
            app_handle,
            &stored.local_path,
            "local-only.png",
        )
        .expect("persisted local backend should resolve local file url");

        std::env::remove_var(MEDIA_UPLOAD_DRIVER_ENV);
        std::env::remove_var(MEDIA_UPLOAD_ENDPOINT_ENV);

        assert!(remote_url.starts_with("http://127.0.0.1:45115/chat-media/asset/"));
    }

    #[test]
    fn extract_upload_url_supports_nip94_response_shape() {
        let remote_url = extract_upload_url(
            r#"{
                "status": "success",
                "nip94_event": {
                    "tags": [
                        ["x", "sha256"],
                        ["url", "https://cdn.example.test/chat-media/photo.png"]
                    ]
                }
            }"#,
        );

        assert_eq!(
            remote_url.as_deref(),
            Some("https://cdn.example.test/chat-media/photo.png")
        );
    }
}
