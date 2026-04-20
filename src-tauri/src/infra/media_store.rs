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
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, Runtime};

pub fn store_chat_media_asset<R: Runtime>(
    app_handle: &tauri::AppHandle<R>,
    input: StoreChatMediaAssetInput,
) -> Result<StoredChatMediaAsset, String> {
    let file_name = input.name.trim();
    if file_name.is_empty() {
        return Err("media name is empty".into());
    }

    let parsed = decode_data_url(&input.data_url)?;
    persist_media_asset_bytes(app_handle, &input.kind, file_name, parsed.mime_type.as_deref(), &parsed.bytes)
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

    persist_media_asset_bytes(app_handle, kind, file_name, mime_type.as_deref(), bytes.as_ref())
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

struct ParsedDataUrl {
    mime_type: Option<String>,
    bytes: Vec<u8>,
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
