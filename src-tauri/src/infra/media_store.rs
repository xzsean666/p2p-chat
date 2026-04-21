use crate::domain::chat::{
    ChatDomainSeed, ChatMediaKind, CleanupChatMediaAssetsResult, StoreChatMediaAssetInput,
    StoredChatMediaAsset,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use reqwest::blocking::Client as BlockingHttpClient;
use reqwest::header::CONTENT_TYPE;
use serde_json::Value as JsonValue;
use std::ffi::OsStr;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, Runtime};

const LOCAL_CHAT_MEDIA_SERVER_PORT: u16 = 45115;
const LOCAL_CHAT_MEDIA_ROUTE_PREFIX: &str = "/chat-media/asset/";

pub fn store_chat_media_asset<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: StoreChatMediaAssetInput,
) -> Result<StoredChatMediaAsset, String> {
    let file_name = input.name.trim();
    if file_name.is_empty() {
        return Err("media name is empty".into());
    }

    let parsed = decode_data_url(&input.data_url)?;
    persist_media_asset_bytes(
        app_handle,
        &input.kind,
        file_name,
        parsed.mime_type.as_deref(),
        &parsed.bytes,
    )
}

pub fn download_chat_media_asset<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    kind: &ChatMediaKind,
    name: &str,
    remote_url: &str,
) -> Result<StoredChatMediaAsset, String> {
    let file_name = name.trim();
    if file_name.is_empty() {
        return Err("media name is empty".into());
    }

    let normalized_url = remote_url.trim();
    if normalized_url.is_empty() {
        return Err("media remote url is empty".into());
    }

    let parsed_url = reqwest::Url::parse(normalized_url).map_err(|error| error.to_string())?;
    match parsed_url.scheme() {
        "http" | "https" => {}
        _ => return Err("media remote url must use http or https".into()),
    }

    let client = BlockingHttpClient::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(parsed_url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| error.to_string())?;
    let mime_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let bytes = response.bytes().map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("media payload is empty".into());
    }

    persist_media_asset_bytes(
        app_handle,
        kind,
        file_name,
        mime_type.as_deref(),
        bytes.as_ref(),
    )
}

pub fn cleanup_chat_media_assets<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    seed: &ChatDomainSeed,
) -> Result<CleanupChatMediaAssetsResult, String> {
    let media_root = chat_media_root(app_handle)?;
    if !media_root.exists() {
        return Ok(CleanupChatMediaAssetsResult { removed_count: 0 });
    }

    let referenced_paths = collect_referenced_media_paths(seed, &media_root);
    let removed_count = remove_orphaned_media_files(&media_root, &referenced_paths)?;
    remove_empty_media_directories(&media_root)?;

    Ok(CleanupChatMediaAssetsResult { removed_count })
}

pub fn local_chat_media_file_url(local_path: &str) -> Result<String, String> {
    let normalized_path = local_path.trim();
    if normalized_path.is_empty() {
        return Err("local media path is empty".into());
    }

    let asset_path = PathBuf::from(normalized_path);
    if !path_contains_chat_media_component(&asset_path) {
        return Err("local media path must be stored under chat-media".into());
    }

    let server = local_chat_media_server()?;
    let encoded_path = hex_encode(normalized_path.as_bytes());
    let file_name = sanitize_route_file_name(
        asset_path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("asset.bin"),
    );

    Ok(format!(
        "{}{}{}/{}",
        server.base_url, LOCAL_CHAT_MEDIA_ROUTE_PREFIX, encoded_path, file_name
    ))
}

struct ParsedDataUrl {
    mime_type: Option<String>,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct LocalChatMediaServer {
    base_url: String,
}

impl LocalChatMediaServer {
    fn start() -> Result<Self, String> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", LOCAL_CHAT_MEDIA_SERVER_PORT))
            .map_err(|error| format!("failed to bind local chat media server: {error}"))?;
        let address = listener.local_addr().map_err(|error| {
            format!("failed to resolve local chat media server address: {error}")
        })?;
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let _ = handle_local_chat_media_request(&mut stream);
            }
        });

        Ok(Self {
            base_url: format!("http://{}", address),
        })
    }
}

fn persist_media_asset_bytes<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    kind: &ChatMediaKind,
    name: &str,
    mime_type: Option<&str>,
    bytes: &[u8],
) -> Result<StoredChatMediaAsset, String> {
    let media_dir = chat_media_root(app_handle)?.join(media_kind_directory(kind));
    fs::create_dir_all(&media_dir).map_err(|error| error.to_string())?;

    let asset_path = media_dir.join(build_media_file_name(name, mime_type));
    fs::write(&asset_path, bytes).map_err(|error| error.to_string())?;

    Ok(StoredChatMediaAsset {
        local_path: asset_path.to_string_lossy().into_owned(),
    })
}

fn local_chat_media_server() -> Result<&'static LocalChatMediaServer, String> {
    static LOCAL_CHAT_MEDIA_SERVER: OnceLock<LocalChatMediaServer> = OnceLock::new();

    if let Some(server) = LOCAL_CHAT_MEDIA_SERVER.get() {
        return Ok(server);
    }

    let server = LocalChatMediaServer::start()?;
    let _ = LOCAL_CHAT_MEDIA_SERVER.set(server);
    LOCAL_CHAT_MEDIA_SERVER
        .get()
        .ok_or_else(|| "local chat media server was not initialized".to_string())
}

fn chat_media_root<R: Runtime>(app_handle: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    Ok(app_handle
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?
        .join("chat-media"))
}

fn decode_data_url(value: &str) -> Result<ParsedDataUrl, String> {
    let normalized = value.trim();
    let payload = normalized
        .strip_prefix("data:")
        .ok_or_else(|| "media data url is invalid".to_string())?;
    let (header, encoded) = payload
        .split_once(',')
        .ok_or_else(|| "media data url is invalid".to_string())?;
    let mime_header = header
        .strip_suffix(";base64")
        .ok_or_else(|| "media data url must be base64-encoded".to_string())?;
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())?;

    if bytes.is_empty() {
        return Err("media payload is empty".into());
    }

    Ok(ParsedDataUrl {
        mime_type: if mime_header.trim().is_empty() {
            None
        } else {
            Some(mime_header.trim().to_string())
        },
        bytes,
    })
}

fn media_kind_directory(kind: &ChatMediaKind) -> &'static str {
    match kind {
        ChatMediaKind::File => "files",
        ChatMediaKind::Image => "images",
        ChatMediaKind::Video => "videos",
    }
}

fn build_media_file_name(original_name: &str, mime_type: Option<&str>) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let stem = sanitize_file_stem(
        Path::new(original_name)
            .file_stem()
            .and_then(OsStr::to_str)
            .unwrap_or("asset"),
    );
    let extension = resolve_extension(original_name, mime_type);
    format!("{timestamp}-{stem}.{extension}")
}

fn collect_referenced_media_paths(
    seed: &ChatDomainSeed,
    media_root: &Path,
) -> std::collections::HashSet<PathBuf> {
    seed.message_store
        .values()
        .flat_map(|messages| messages.iter())
        .filter_map(|message| message.meta.as_deref())
        .filter_map(meta_local_path)
        .map(PathBuf::from)
        .filter(|path| path.starts_with(media_root))
        .collect()
}

fn path_contains_chat_media_component(path: &Path) -> bool {
    path.components().any(|component| match component {
        Component::Normal(segment) => segment == OsStr::new("chat-media"),
        _ => false,
    })
}

fn sanitize_route_file_name(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                Some(character)
            } else {
                None
            }
        })
        .take(96)
        .collect::<String>();

    if sanitized.is_empty() {
        "asset.bin".into()
    } else {
        sanitized
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() || value.len() % 2 != 0 {
        return Err("local media asset token is invalid".into());
    }

    value
        .as_bytes()
        .chunks_exact(2)
        .map(|chunk| {
            let chunk = std::str::from_utf8(chunk)
                .map_err(|_| "local media asset token is invalid".to_string())?;
            u8::from_str_radix(chunk, 16)
                .map_err(|_| "local media asset token is invalid".to_string())
        })
        .collect()
}

fn handle_local_chat_media_request(stream: &mut TcpStream) -> Result<(), String> {
    let mut buffer = [0u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .map_err(|error| format!("failed to read local media request: {error}"))?;
    if bytes_read == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" && method != "HEAD" {
        write_http_response(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            if method == "HEAD" {
                &[]
            } else {
                b"method not allowed"
            },
        )?;
        return Ok(());
    }

    let Some(encoded_path) = path
        .strip_prefix(LOCAL_CHAT_MEDIA_ROUTE_PREFIX)
        .and_then(|value| value.split('/').next())
    else {
        write_http_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            if method == "HEAD" {
                &[]
            } else {
                b"asset not found"
            },
        )?;
        return Ok(());
    };

    let asset_path = hex_decode(encoded_path)
        .and_then(|bytes| {
            String::from_utf8(bytes).map_err(|_| "local media asset token is invalid".to_string())
        })
        .map(PathBuf::from)?;
    if !path_contains_chat_media_component(&asset_path) {
        write_http_response(
            stream,
            "403 Forbidden",
            "text/plain; charset=utf-8",
            if method == "HEAD" {
                &[]
            } else {
                b"asset path is not allowed"
            },
        )?;
        return Ok(());
    }

    let bytes = match fs::read(&asset_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_http_response(
                stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                if method == "HEAD" {
                    &[]
                } else {
                    b"asset not found"
                },
            )?;
            return Ok(());
        }
        Err(error) => return Err(format!("failed to read local media asset: {error}")),
    };
    let content_type = media_content_type_from_path(&asset_path);
    if method == "HEAD" {
        write_http_response(stream, "200 OK", content_type, &[])?;
    } else {
        write_http_response(stream, "200 OK", content_type, &bytes)?;
    }

    Ok(())
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|error| format!("failed to write local media response head: {error}"))?;
    if !body.is_empty() {
        stream
            .write_all(body)
            .map_err(|error| format!("failed to write local media response body: {error}"))?;
    }
    Ok(())
}

fn media_content_type_from_path(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("mov") => "video/quicktime",
        Some("webm") => "video/webm",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn meta_local_path(meta: &str) -> Option<String> {
    let parsed = serde_json::from_str::<JsonValue>(meta).ok()?;
    parsed
        .get("localPath")?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn remove_orphaned_media_files(
    root: &Path,
    referenced_paths: &std::collections::HashSet<PathBuf>,
) -> Result<u32, String> {
    let mut removed_count = 0u32;
    let mut pending_directories = vec![root.to_path_buf()];

    while let Some(directory) = pending_directories.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };

        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                pending_directories.push(path);
                continue;
            }

            if referenced_paths.contains(&path) {
                continue;
            }

            fs::remove_file(&path).map_err(|error| error.to_string())?;
            removed_count = removed_count.saturating_add(1);
        }
    }

    Ok(removed_count)
}

fn remove_empty_media_directories(root: &Path) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }

    let mut directories = vec![root.to_path_buf()];
    let mut ordered = Vec::new();
    while let Some(directory) = directories.pop() {
        ordered.push(directory.clone());
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };

        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
            }
        }
    }

    for directory in ordered.into_iter().rev() {
        if directory == root {
            continue;
        }

        let mut entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };

        if entries.next().is_none() {
            let _ = fs::remove_dir(&directory);
        }
    }

    Ok(())
}

fn sanitize_file_stem(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if matches!(character, '-' | '_') {
                Some(character)
            } else {
                None
            }
        })
        .take(48)
        .collect::<String>();

    if sanitized.is_empty() {
        "asset".into()
    } else {
        sanitized
    }
}

fn resolve_extension(original_name: &str, mime_type: Option<&str>) -> String {
    if let Some(extension) = Path::new(original_name)
        .extension()
        .and_then(OsStr::to_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let sanitized = extension
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase();
        if !sanitized.is_empty() {
            return sanitized;
        }
    }

    if let Some(mime_type) = mime_type {
        let subtype = mime_type
            .split('/')
            .nth(1)
            .unwrap_or("bin")
            .split(';')
            .next()
            .unwrap_or("bin")
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .take(12)
            .collect::<String>()
            .to_ascii_lowercase();
        if !subtype.is_empty() {
            return match subtype.as_str() {
                "jpeg" => "jpg".into(),
                other => other.into(),
            };
        }
    }

    "bin".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::MutexGuard;

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

            let _ = fs::remove_dir_all(&self.config_root);
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
        let config_root = std::env::temp_dir().join(format!("p2p-chat-media-store-test-{unique}"));
        fs::create_dir_all(&config_root).expect("failed to create test config root");

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
    fn store_chat_media_asset_persists_file_in_media_directory() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let stored = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::Image,
                name: "Harbor Sunrise.png".into(),
                data_url: "data:image/png;base64,aGVsbG8=".into(),
            },
        )
        .expect("media should be stored");

        assert!(stored.local_path.contains("chat-media/images/"));
        assert!(stored.local_path.ends_with(".png"));
        let bytes = fs::read(&stored.local_path).expect("stored media should be readable");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn store_chat_media_asset_rejects_invalid_data_url() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let error = store_chat_media_asset(
            app_handle,
            StoreChatMediaAssetInput {
                kind: ChatMediaKind::File,
                name: "notes.txt".into(),
                data_url: "not-a-data-url".into(),
            },
        )
        .expect_err("invalid data url should be rejected");

        assert!(error.contains("media data url is invalid"));
    }

    #[test]
    fn local_chat_media_file_url_exposes_stored_media_over_http() {
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

        let remote_url =
            local_chat_media_file_url(&stored.local_path).expect("local media url should resolve");
        assert!(remote_url.starts_with("http://127.0.0.1:45115/chat-media/asset/"));

        let response =
            reqwest::blocking::get(&remote_url).expect("local media file server should respond");
        assert!(response.status().is_success());
        assert_eq!(
            response.bytes().expect("local media bytes should load"),
            b"hello".as_slice()
        );
    }

    #[test]
    fn local_chat_media_file_url_rejects_paths_outside_chat_media() {
        let temp_path = std::env::temp_dir().join(format!(
            "p2p-chat-non-media-{}.txt",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should be after unix epoch")
                .as_nanos()
        ));
        fs::write(&temp_path, b"outside").expect("temp file should be created");

        let error = local_chat_media_file_url(temp_path.to_string_lossy().as_ref())
            .expect_err("outside path should be rejected");
        assert!(error.contains("chat-media"));

        let _ = fs::remove_file(temp_path);
    }

    #[test]
    fn cleanup_chat_media_assets_keeps_referenced_files_and_removes_orphans() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let media_root = chat_media_root(app_handle).expect("media root should resolve");
        let image_dir = media_root.join("images");
        let file_dir = media_root.join("files");
        fs::create_dir_all(&image_dir).expect("image dir should exist");
        fs::create_dir_all(&file_dir).expect("file dir should exist");

        let kept_path = image_dir.join("kept.png");
        let orphan_path = file_dir.join("orphan.txt");
        fs::write(&kept_path, b"keep").expect("kept file should be created");
        fs::write(&orphan_path, b"remove").expect("orphan file should be created");

        let seed = ChatDomainSeed {
            message_store: std::collections::HashMap::from([(
                "mika".into(),
                vec![crate::domain::chat::MessageItem {
                    id: "message-1".into(),
                    kind: crate::domain::chat::MessageKind::Image,
                    author: crate::domain::chat::MessageAuthor::Me,
                    body: "kept.png".into(),
                    time: "now".into(),
                    meta: Some(
                        serde_json::json!({
                            "version": 2,
                            "label": "PNG · 1 KB",
                            "localPath": kept_path.to_string_lossy()
                        })
                        .to_string(),
                    ),
                    delivery_status: None,
                    remote_id: None,
                    sync_source: None,
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            )]),
            ..Default::default()
        };

        let result = cleanup_chat_media_assets(app_handle, &seed).expect("cleanup should complete");

        assert_eq!(result.removed_count, 1);
        assert!(kept_path.exists());
        assert!(!orphan_path.exists());
    }

    #[test]
    fn cleanup_chat_media_assets_keeps_local_file_when_meta_also_has_remote_url() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let media_root = chat_media_root(app_handle).expect("media root should resolve");
        let image_dir = media_root.join("images");
        let file_dir = media_root.join("files");
        fs::create_dir_all(&image_dir).expect("image dir should exist");
        fs::create_dir_all(&file_dir).expect("file dir should exist");

        let kept_path = image_dir.join("kept-remote-backed.png");
        let orphan_path = file_dir.join("orphan-remote-only.txt");
        fs::write(&kept_path, b"keep").expect("kept file should be created");
        fs::write(&orphan_path, b"remove").expect("orphan file should be created");

        let seed = ChatDomainSeed {
            message_store: std::collections::HashMap::from([(
                "mika".into(),
                vec![crate::domain::chat::MessageItem {
                    id: "message-1".into(),
                    kind: crate::domain::chat::MessageKind::Image,
                    author: crate::domain::chat::MessageAuthor::Me,
                    body: "kept-remote-backed.png".into(),
                    time: "now".into(),
                    meta: Some(
                        serde_json::json!({
                            "version": 3,
                            "label": "PNG · 1 KB",
                            "localPath": kept_path.to_string_lossy(),
                            "remoteUrl": "https://cdn.example.test/chat-media/kept-remote-backed.png"
                        })
                        .to_string(),
                    ),
                    delivery_status: None,
                    remote_id: None,
                    sync_source: None,
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            )]),
            ..Default::default()
        };

        let result = cleanup_chat_media_assets(app_handle, &seed).expect("cleanup should complete");

        assert_eq!(result.removed_count, 1);
        assert!(kept_path.exists());
        assert!(!orphan_path.exists());
    }
}
