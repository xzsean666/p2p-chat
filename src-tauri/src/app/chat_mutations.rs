use crate::app::{auth_access, shell_auth};
use crate::domain::chat::{
    AddCircleInput, AddCircleResult, AuthRuntimeState, AuthSessionSummary,
    CacheChatMessageMediaInput, CachedChatMessageMediaResult, ChatDomainSeed, ChatSessionAction,
    CircleCreateMode, CircleItem, CircleStatus, CircleType, CleanupChatMediaAssetsResult,
    ContactItem, CreateGroupConversationInput, GroupMember, GroupProfile, GroupRole,
    LoginAccessKind, LoginMethod, MergeRemoteDeliveryReceiptsInput, MergeRemoteMessagesInput,
    MessageAuthor, MessageDeliveryStatus, MessageItem, MessageKind, MessageReplyPreview,
    MessageSyncSource, RestoreCircleInput, RetryMessageDeliveryInput, SendFileMessageInput,
    SendImageMessageInput, SendMessageInput, SendVideoMessageInput, SessionActionInput,
    SessionItem, SessionKind, ShellStateSnapshot, SignedNostrEvent, StartConversationInput,
    StartConversationResult, StartLookupConversationInput, StartSelfConversationInput,
    StoreChatMediaAssetInput, StoredChatMediaAsset, UpdateChatMessageMediaRemoteUrlInput,
    UpdateCircleInput, UpdateContactRemarkInput, UpdateGroupMembersInput, UpdateGroupNameInput,
    UpdateMessageDeliveryStatusInput, UpdateSessionDraftInput,
    UpdatedChatMessageMediaRemoteUrlResult,
};
use crate::domain::chat_repository::{
    build_message_reply_preview, build_remote_delivery_receipt_change_set,
    build_remote_message_merge_change_set, merge_message_records, message_media_local_path,
    message_media_meta_with_local_path, message_media_meta_with_remote_url,
    message_media_remote_url, ChatDomainChangeSet, ChatRepository, ChatUpsert,
};
use crate::domain::transport_repository::TransportRepository;
use crate::infra::auth_runtime_credential_store::{self, StoredAuthRuntimeCredential};
use crate::infra::media_store;
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use crate::infra::sqlite_transport_repository::SqliteTransportRepository;
use nostr_connect::prelude::PublicKey as NostrPublicKey;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

pub fn send_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendMessageInput,
) -> Result<ChatDomainSeed, String> {
    send_local_message(
        app_handle,
        SendLocalMessageInput {
            session_id: input.session_id,
            kind: MessageKind::Text,
            body: input.body,
            meta: None,
            reply_to_message_id: input.reply_to_message_id,
        },
    )
}

pub fn send_file_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendFileMessageInput,
) -> Result<ChatDomainSeed, String> {
    send_local_message(
        app_handle,
        SendLocalMessageInput {
            session_id: input.session_id,
            kind: MessageKind::File,
            body: input.name,
            meta: input.meta,
            reply_to_message_id: input.reply_to_message_id,
        },
    )
}

pub fn send_image_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendImageMessageInput,
) -> Result<ChatDomainSeed, String> {
    send_local_message(
        app_handle,
        SendLocalMessageInput {
            session_id: input.session_id,
            kind: MessageKind::Image,
            body: input.name,
            meta: Some(input.meta),
            reply_to_message_id: input.reply_to_message_id,
        },
    )
}

pub fn send_video_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendVideoMessageInput,
) -> Result<ChatDomainSeed, String> {
    send_local_message(
        app_handle,
        SendLocalMessageInput {
            session_id: input.session_id,
            kind: MessageKind::Video,
            body: input.name,
            meta: Some(input.meta),
            reply_to_message_id: input.reply_to_message_id,
        },
    )
}

pub fn store_chat_media_asset(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: StoreChatMediaAssetInput,
) -> Result<StoredChatMediaAsset, String> {
    media_store::store_chat_media_asset(app_handle, input)
}

pub fn cleanup_chat_media_assets(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
) -> Result<CleanupChatMediaAssetsResult, String> {
    let seed = load_domain_seed(app_handle)?;
    media_store::cleanup_chat_media_assets(app_handle, &seed)
}

pub fn cache_chat_message_media(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: CacheChatMessageMediaInput,
) -> Result<CachedChatMessageMediaResult, String> {
    let seed = load_domain_seed(app_handle)?;
    let message = seed
        .message_store
        .get(&input.session_id)
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.id == input.message_id)
        })
        .cloned()
        .ok_or_else(|| format!("message not found: {}", input.message_id))?;

    let kind = match message.kind {
        MessageKind::File => crate::domain::chat::ChatMediaKind::File,
        MessageKind::Image => crate::domain::chat::ChatMediaKind::Image,
        MessageKind::Video => crate::domain::chat::ChatMediaKind::Video,
        _ => return Err("message does not support media caching".into()),
    };

    if let Some(local_path) =
        message_media_local_path(&message).filter(|local_path| Path::new(local_path).exists())
    {
        return Ok(CachedChatMessageMediaResult { seed, local_path });
    }

    let remote_url = message_media_remote_url(&message)
        .ok_or_else(|| "message does not have a remote media url".to_string())?;
    let stored_asset =
        media_store::download_chat_media_asset(app_handle, &kind, &message.body, &remote_url)?;
    let next_meta = message_media_meta_with_local_path(&message, stored_asset.local_path.clone())
        .ok_or_else(|| "message media metadata is invalid".to_string())?;
    let updated_message = merge_message_records(
        message.clone(),
        MessageItem {
            meta: Some(next_meta),
            ..message
        },
    );
    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            messages_upsert: vec![(input.session_id, updated_message)],
            ..Default::default()
        },
    )?;

    Ok(CachedChatMessageMediaResult {
        seed,
        local_path: stored_asset.local_path,
    })
}

pub fn update_chat_message_media_remote_url(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateChatMessageMediaRemoteUrlInput,
) -> Result<UpdatedChatMessageMediaRemoteUrlResult, String> {
    let session_id = input.session_id.clone();
    let message_id = input.message_id.clone();
    let seed = load_domain_seed(app_handle)?;
    let message = seed
        .message_store
        .get(&session_id)
        .and_then(|messages| messages.iter().find(|message| message.id == message_id))
        .cloned()
        .ok_or_else(|| format!("message not found: {message_id}"))?;

    match message.kind {
        MessageKind::File | MessageKind::Image | MessageKind::Video => {}
        _ => return Err("message does not support remote media urls".into()),
    }

    let remote_url = normalize_chat_media_remote_url(&input.remote_url)?;
    let next_meta = message_media_meta_with_remote_url(&message, remote_url.clone())
        .ok_or_else(|| "message media metadata is invalid".to_string())?;
    let updated_message = merge_message_records(
        message.clone(),
        MessageItem {
            meta: Some(next_meta),
            ..message
        },
    );
    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            messages_upsert: vec![(session_id.clone(), updated_message)],
            ..Default::default()
        },
    )?;
    clear_transport_outbound_dispatch_for_message(app_handle, &session_id, &message_id)?;

    Ok(UpdatedChatMessageMediaRemoteUrlResult { seed, remote_url })
}

struct SendLocalMessageInput {
    session_id: String,
    kind: MessageKind,
    body: String,
    meta: Option<String>,
    reply_to_message_id: Option<String>,
}

fn send_local_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendLocalMessageInput,
) -> Result<ChatDomainSeed, String> {
    let send_context = resolve_message_send_context(app_handle)?;
    let SendLocalMessageInput {
        session_id,
        kind,
        body,
        meta,
        reply_to_message_id,
    } = input;

    let content = body.trim();
    if content.is_empty() {
        return Err("message body is empty".into());
    }
    let meta = meta
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let seed = load_domain_seed(app_handle)?;
    let mut updated_session = seed
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    updated_session.subtitle = local_message_preview_text(&kind, content, meta.as_deref());
    updated_session.time = "now".into();
    updated_session.draft = None;
    let delivery_status = delivery_status_for_circle(&seed, &updated_session.circle_id)?;
    let reply_to = resolve_reply_to_preview(&seed, &session_id, reply_to_message_id.as_deref())?;
    let text_note_tags = match &kind {
        MessageKind::Text => message_text_note_tags(&seed, &session_id, reply_to.as_ref()),
        _ => Vec::new(),
    };
    let text_note_signer = match &kind {
        MessageKind::Text => send_context.text_note_signer.as_ref(),
        _ => None,
    };

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: updated_session,
                move_to_top: true,
            }],
            messages_append: vec![(
                session_id,
                build_local_message(
                    app_handle,
                    unique_local_id("message"),
                    kind,
                    content.to_string(),
                    meta,
                    "now".into(),
                    Some(delivery_status),
                    reply_to,
                    &text_note_tags,
                    text_note_signer,
                )?,
            )],
            ..Default::default()
        },
    )
}

fn normalize_chat_media_remote_url(value: &str) -> Result<String, String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err("remote media url is empty".into());
    }

    let parsed = Url::parse(normalized).map_err(|_| "remote media url is invalid".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("remote media url must use http or https".into());
    }

    Ok(normalized.to_string())
}

pub fn update_session_draft(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateSessionDraftInput,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let mut session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;

    session.draft = if input.draft.is_empty() {
        None
    } else {
        Some(input.draft)
    };

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: session,
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn update_message_delivery_status(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateMessageDeliveryStatusInput,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let message = seed
        .message_store
        .get(&input.session_id)
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.id == input.message_id)
                .cloned()
        })
        .ok_or_else(|| format!("message not found: {}", input.message_id))?;

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            messages_upsert: vec![(
                input.session_id,
                with_delivery_status(app_handle, message, input.delivery_status, &[], None)?,
            )],
            ..Default::default()
        },
    )
}

pub fn retry_message_delivery(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: RetryMessageDeliveryInput,
) -> Result<ChatDomainSeed, String> {
    let send_context = resolve_message_send_context(app_handle)?;
    let session_id = input.session_id.clone();
    let message_id = input.message_id.clone();

    let seed = load_domain_seed(app_handle)?;
    let session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;
    let message = seed
        .message_store
        .get(&input.session_id)
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.id == input.message_id)
                .cloned()
        })
        .ok_or_else(|| format!("message not found: {}", input.message_id))?;

    if !matches!(message.author, MessageAuthor::Me) {
        return Err("only local messages can be retried".into());
    }

    let delivery_status = delivery_status_for_circle(&seed, &session.circle_id)?;
    let text_note_tags = message_text_note_tags(&seed, &session_id, message.reply_to.as_ref());
    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            messages_upsert: vec![(
                session_id.clone(),
                with_delivery_status(
                    app_handle,
                    message,
                    delivery_status,
                    &text_note_tags,
                    send_context.text_note_signer.as_ref(),
                )?,
            )],
            ..Default::default()
        },
    )?;
    clear_transport_outbound_dispatch_for_message(app_handle, &session_id, &message_id)?;
    Ok(seed)
}

pub fn merge_remote_messages(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: MergeRemoteMessagesInput,
) -> Result<ChatDomainSeed, String> {
    if input.messages.is_empty() {
        return load_domain_seed(app_handle);
    }

    let seed = load_domain_seed(app_handle)?;
    let change_set =
        build_remote_message_merge_change_set(&seed, &input.session_id, input.messages)?;
    apply_domain_change_set(app_handle, change_set)
}

pub fn merge_remote_delivery_receipts(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: MergeRemoteDeliveryReceiptsInput,
) -> Result<ChatDomainSeed, String> {
    if input.receipts.is_empty() {
        return load_domain_seed(app_handle);
    }

    let seed = load_domain_seed(app_handle)?;
    let change_set =
        build_remote_delivery_receipt_change_set(&seed, &input.session_id, input.receipts)?;
    apply_domain_change_set(app_handle, change_set)
}

pub fn start_conversation(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: StartConversationInput,
) -> Result<StartConversationResult, String> {
    let seed = load_domain_seed(app_handle)?;
    ensure_circle_exists(&seed, &input.circle_id)?;

    let contact = seed
        .contacts
        .iter()
        .find(|item| item.id == input.contact_id)
        .cloned()
        .ok_or_else(|| format!("contact not found: {}", input.contact_id))?;

    let plan = build_direct_session_plan(&seed, &input.circle_id, &contact);
    let session_id = plan.session_id.clone();
    let mut change_set = ChatDomainChangeSet::default();
    apply_session_plan(&mut change_set, plan);
    let seed = apply_domain_change_set(app_handle, change_set)?;
    Ok(StartConversationResult { seed, session_id })
}

pub fn start_self_conversation(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: StartSelfConversationInput,
) -> Result<StartConversationResult, String> {
    let seed = load_domain_seed(app_handle)?;
    ensure_circle_exists(&seed, &input.circle_id)?;

    let plan = build_self_session_plan(&seed, &input.circle_id);
    let session_id = plan.session_id.clone();
    let mut change_set = ChatDomainChangeSet::default();
    apply_session_plan(&mut change_set, plan);
    let seed = apply_domain_change_set(app_handle, change_set)?;
    Ok(StartConversationResult { seed, session_id })
}

pub fn create_group_conversation(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: CreateGroupConversationInput,
) -> Result<StartConversationResult, String> {
    let seed = load_domain_seed(app_handle)?;
    ensure_circle_exists(&seed, &input.circle_id)?;

    let member_contact_ids = dedupe_contact_ids(input.member_contact_ids);
    if member_contact_ids.is_empty() {
        return Err("group needs at least one member".into());
    }

    let contacts = member_contact_ids
        .iter()
        .map(|contact_id| {
            seed.contacts
                .iter()
                .find(|contact| contact.id == *contact_id)
                .cloned()
                .ok_or_else(|| format!("contact not found: {contact_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let group_name = normalized_group_name(input.name.trim(), &contacts);
    let session_id = build_unique_session_id(
        &format!("group-{}", build_circle_slug(&group_name)),
        &seed.sessions,
    );
    let description = default_group_description(&group_name);
    let system_message = format!(
        "Group created with {}",
        contacts
            .iter()
            .map(|contact| contact.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: SessionItem {
                    id: session_id.clone(),
                    circle_id: input.circle_id,
                    name: group_name.clone(),
                    initials: build_initials(&group_name),
                    subtitle: description.clone(),
                    time: "now".into(),
                    unread_count: None,
                    muted: None,
                    pinned: None,
                    draft: None,
                    kind: SessionKind::Group,
                    category: "groups".into(),
                    members: Some((contacts.len() + 1) as u32),
                    contact_id: None,
                    archived: None,
                },
                move_to_top: true,
            }],
            groups_upsert: vec![ChatUpsert {
                item: GroupProfile {
                    session_id: session_id.clone(),
                    name: group_name,
                    description,
                    members: member_contact_ids
                        .iter()
                        .enumerate()
                        .map(|(index, contact_id)| GroupMember {
                            contact_id: contact_id.clone(),
                            role: Some(if index == 0 {
                                GroupRole::Admin
                            } else {
                                GroupRole::Member
                            }),
                        })
                        .collect(),
                    muted: None,
                },
                move_to_top: true,
            }],
            messages_replace: vec![(
                session_id.clone(),
                vec![MessageItem {
                    id: unique_local_id("system"),
                    kind: MessageKind::System,
                    author: MessageAuthor::System,
                    body: system_message,
                    time: String::new(),
                    meta: None,
                    delivery_status: None,
                    remote_id: None,
                    sync_source: Some(MessageSyncSource::System),
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            )],
            ..Default::default()
        },
    )?;
    Ok(StartConversationResult { seed, session_id })
}

pub fn start_lookup_conversation(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: StartLookupConversationInput,
) -> Result<StartConversationResult, String> {
    let seed = load_domain_seed(app_handle)?;
    ensure_circle_exists(&seed, &input.circle_id)?;

    let normalized_query = input.query.trim();
    if normalized_query.is_empty() {
        return Err("lookup query is empty".into());
    }

    let mut change_set = ChatDomainChangeSet::default();
    let contact = if let Some(contact) = find_contact_for_lookup(&seed, normalized_query) {
        contact
    } else {
        let new_contact = build_lookup_contact(&seed, normalized_query);
        change_set.contacts_upsert.push(ChatUpsert {
            item: new_contact.clone(),
            move_to_top: true,
        });
        new_contact
    };

    let plan = build_direct_session_plan(&seed, &input.circle_id, &contact);
    let session_id = plan.session_id.clone();
    apply_session_plan(&mut change_set, plan);
    let seed = apply_domain_change_set(app_handle, change_set)?;
    Ok(StartConversationResult { seed, session_id })
}

pub fn apply_session_action(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SessionActionInput,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let target_session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;

    let mut change_set = ChatDomainChangeSet::default();

    match input.action {
        ChatSessionAction::Pin => {
            let mut session = target_session.clone();
            session.pinned = Some(!session.pinned.unwrap_or(false));
            change_set.sessions_upsert.push(ChatUpsert {
                item: session,
                move_to_top: false,
            });
        }
        ChatSessionAction::Mute => {
            let mut session = target_session.clone();
            let next_value = !session.muted.unwrap_or(false);
            session.muted = Some(next_value);
            change_set.sessions_upsert.push(ChatUpsert {
                item: session,
                move_to_top: false,
            });

            if matches!(target_session.kind, SessionKind::Group) {
                if let Some(mut group) = seed
                    .groups
                    .iter()
                    .find(|group| group.session_id == input.session_id)
                    .cloned()
                {
                    group.muted = Some(next_value);
                    change_set.groups_upsert.push(ChatUpsert {
                        item: group,
                        move_to_top: false,
                    });
                }
            }
        }
        ChatSessionAction::Archive => {
            let mut session = target_session.clone();
            session.archived = Some(true);
            session.pinned = Some(false);
            change_set.sessions_upsert.push(ChatUpsert {
                item: session,
                move_to_top: false,
            });
        }
        ChatSessionAction::Unarchive => {
            let mut session = target_session.clone();
            session.archived = Some(false);
            change_set.sessions_upsert.push(ChatUpsert {
                item: session,
                move_to_top: true,
            });
        }
        ChatSessionAction::ClearUnread => {
            if target_session.unread_count.is_some() {
                let mut session = target_session.clone();
                session.unread_count = None;
                change_set.sessions_upsert.push(ChatUpsert {
                    item: session,
                    move_to_top: false,
                });
            }
        }
        ChatSessionAction::Delete => {
            change_set
                .session_ids_to_delete
                .push(target_session.id.clone());
        }
    }

    apply_domain_change_set(app_handle, change_set)
}

pub fn toggle_contact_block(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    contact_id: String,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let mut contact = seed
        .contacts
        .iter()
        .find(|item| item.id == contact_id)
        .cloned()
        .ok_or_else(|| format!("contact not found: {contact_id}"))?;
    let next_value = !contact.blocked.unwrap_or(false);
    contact.blocked = Some(next_value);

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            contacts_upsert: vec![ChatUpsert {
                item: contact,
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn update_contact_remark(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateContactRemarkInput,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let contact = seed
        .contacts
        .iter()
        .find(|item| item.id == input.contact_id)
        .cloned()
        .ok_or_else(|| format!("contact not found: {}", input.contact_id))?;
    let next_remark = input.remark.trim().to_string();

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            contacts_upsert: vec![ChatUpsert {
                item: ContactItem {
                    subtitle: next_remark,
                    ..contact
                },
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn update_group_name(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateGroupNameInput,
) -> Result<ChatDomainSeed, String> {
    let next_name = input.name.trim();
    if next_name.is_empty() {
        return Err("group name is empty".into());
    }

    let seed = load_domain_seed(app_handle)?;
    let session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;
    if !matches!(session.kind, SessionKind::Group) {
        return Err(format!("session is not a group: {}", input.session_id));
    }

    let group = seed
        .groups
        .iter()
        .find(|group| group.session_id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("group not found for session: {}", input.session_id))?;
    let next_description = if group.description == default_group_description(&group.name) {
        default_group_description(next_name)
    } else {
        group.description.clone()
    };
    let next_subtitle = if session.subtitle == group.description {
        next_description.clone()
    } else {
        session.subtitle.clone()
    };

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: SessionItem {
                    name: next_name.into(),
                    initials: build_initials(next_name),
                    subtitle: next_subtitle,
                    ..session
                },
                move_to_top: false,
            }],
            groups_upsert: vec![ChatUpsert {
                item: GroupProfile {
                    name: next_name.into(),
                    description: next_description,
                    ..group
                },
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn update_group_members(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateGroupMembersInput,
) -> Result<ChatDomainSeed, String> {
    let member_contact_ids = dedupe_contact_ids(input.member_contact_ids);
    if member_contact_ids.is_empty() {
        return Err("group needs at least one member".into());
    }

    let seed = load_domain_seed(app_handle)?;
    let session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;
    if !matches!(session.kind, SessionKind::Group) {
        return Err(format!("session is not a group: {}", input.session_id));
    }

    for contact_id in &member_contact_ids {
        if !seed
            .contacts
            .iter()
            .any(|contact| contact.id == *contact_id)
        {
            return Err(format!("contact not found: {contact_id}"));
        }
    }

    let group = seed
        .groups
        .iter()
        .find(|group| group.session_id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("group not found for session: {}", input.session_id))?;
    let current_roles = group
        .members
        .iter()
        .filter_map(|member| {
            member
                .role
                .clone()
                .map(|role| (member.contact_id.clone(), role))
        })
        .collect::<HashMap<_, _>>();
    let mut next_members = member_contact_ids
        .iter()
        .map(|contact_id| GroupMember {
            contact_id: contact_id.clone(),
            role: Some(
                current_roles
                    .get(contact_id)
                    .cloned()
                    .unwrap_or(GroupRole::Member),
            ),
        })
        .collect::<Vec<_>>();

    if !next_members
        .iter()
        .any(|member| matches!(member.role, Some(GroupRole::Admin)))
    {
        if let Some(first_member) = next_members.first_mut() {
            first_member.role = Some(GroupRole::Admin);
        }
    }

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: SessionItem {
                    members: Some((member_contact_ids.len() + 1) as u32),
                    ..session
                },
                move_to_top: false,
            }],
            groups_upsert: vec![ChatUpsert {
                item: GroupProfile {
                    members: next_members,
                    ..group
                },
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn add_circle(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: AddCircleInput,
) -> Result<AddCircleResult, String> {
    let seed = load_domain_seed(app_handle)?;
    let normalized_name = normalized_circle_name(&input);
    let normalized_relay = normalized_circle_relay(&input, &normalized_name);
    let existing_circle_id = seed
        .circles
        .iter()
        .find(|circle| {
            circle
                .relay
                .trim()
                .eq_ignore_ascii_case(normalized_relay.trim())
        })
        .map(|circle| circle.id.clone());

    if let Some(circle_id) = existing_circle_id {
        return Ok(AddCircleResult { seed, circle_id });
    }

    let circle_id = build_unique_circle_id(&normalized_name, &seed.circles);
    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            circles_upsert: vec![ChatUpsert {
                item: CircleItem {
                    id: circle_id.clone(),
                    name: normalized_name,
                    relay: normalized_relay,
                    circle_type: match input.mode {
                        CircleCreateMode::Private => CircleType::Paid,
                        CircleCreateMode::Custom => CircleType::Custom,
                        CircleCreateMode::Invite => CircleType::Default,
                    },
                    status: CircleStatus::Connecting,
                    latency: "--".into(),
                    description: default_circle_description(&match input.mode {
                        CircleCreateMode::Private => CircleType::Paid,
                        CircleCreateMode::Custom => CircleType::Custom,
                        CircleCreateMode::Invite => CircleType::Default,
                    })
                    .into(),
                },
                move_to_top: true,
            }],
            ..Default::default()
        },
    )?;
    Ok(AddCircleResult { seed, circle_id })
}

pub fn restore_circle(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: RestoreCircleInput,
) -> Result<AddCircleResult, String> {
    let normalized_name = if input.name.trim().is_empty() {
        "Restored Circle".to_string()
    } else {
        input.name.trim().to_string()
    };
    let normalized_relay = input.relay.trim().to_string();
    if normalized_relay.is_empty() {
        return Err("restore relay is empty".into());
    }

    let seed = load_domain_seed(app_handle)?;
    let existing_circle_id = seed
        .circles
        .iter()
        .find(|circle| {
            circle
                .relay
                .trim()
                .eq_ignore_ascii_case(normalized_relay.trim())
        })
        .map(|circle| circle.id.clone());

    if let Some(circle_id) = existing_circle_id {
        return Ok(AddCircleResult { seed, circle_id });
    }

    let circle_id = build_unique_circle_id(&normalized_name, &seed.circles);
    let description = if input.description.trim().is_empty() {
        default_circle_description(&input.circle_type).to_string()
    } else {
        input.description.trim().to_string()
    };
    let seed = apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            circles_upsert: vec![ChatUpsert {
                item: CircleItem {
                    id: circle_id.clone(),
                    name: normalized_name,
                    relay: normalized_relay,
                    circle_type: input.circle_type,
                    status: CircleStatus::Connecting,
                    latency: "--".into(),
                    description,
                },
                move_to_top: true,
            }],
            ..Default::default()
        },
    )?;

    Ok(AddCircleResult { seed, circle_id })
}

pub fn update_circle(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: UpdateCircleInput,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    let circle = seed
        .circles
        .iter()
        .find(|item| item.id == input.circle_id)
        .cloned()
        .ok_or_else(|| format!("circle not found: {}", input.circle_id))?;

    let next_name = if input.name.trim().is_empty() {
        circle.name.clone()
    } else {
        input.name.trim().to_string()
    };
    let next_description = if input.description.trim().is_empty() {
        circle.description.clone()
    } else {
        input.description.trim().to_string()
    };

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            circles_upsert: vec![ChatUpsert {
                item: CircleItem {
                    name: next_name,
                    description: next_description,
                    ..circle
                },
                move_to_top: false,
            }],
            ..Default::default()
        },
    )
}

pub fn remove_circle(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    circle_id: String,
) -> Result<ChatDomainSeed, String> {
    let seed = load_domain_seed(app_handle)?;
    if seed.circles.len() <= 1 {
        return Err("cannot remove the last circle".into());
    }

    if !seed.circles.iter().any(|circle| circle.id == circle_id) {
        return Err(format!("circle not found: {circle_id}"));
    }

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            circle_ids_to_delete: vec![circle_id],
            ..Default::default()
        },
    )
}

fn ensure_circle_exists(seed: &ChatDomainSeed, circle_id: &str) -> Result<(), String> {
    if seed.circles.iter().any(|circle| circle.id == circle_id) {
        Ok(())
    } else {
        Err(format!("circle not found: {circle_id}"))
    }
}

fn delivery_status_for_circle(
    seed: &ChatDomainSeed,
    circle_id: &str,
) -> Result<MessageDeliveryStatus, String> {
    let circle = seed
        .circles
        .iter()
        .find(|circle| circle.id == circle_id)
        .ok_or_else(|| format!("circle not found: {circle_id}"))?;

    Ok(match circle.status {
        CircleStatus::Open => MessageDeliveryStatus::Sent,
        CircleStatus::Connecting => MessageDeliveryStatus::Sending,
        CircleStatus::Closed => MessageDeliveryStatus::Failed,
    })
}

fn session_text_note_tags(seed: &ChatDomainSeed, session_id: &str) -> Vec<Vec<String>> {
    let Some(session) = seed
        .sessions
        .iter()
        .find(|session| session.id == session_id)
    else {
        return Vec::new();
    };
    let mut seen_pubkeys = HashSet::<String>::new();
    match session.kind {
        SessionKind::Direct => session
            .contact_id
            .as_deref()
            .and_then(|contact_id| {
                seed.contacts
                    .iter()
                    .find(|contact| contact.id == contact_id)
            })
            .and_then(|contact| normalize_nostr_pubkey(&contact.pubkey))
            .filter(|pubkey| seen_pubkeys.insert(pubkey.clone()))
            .map(|pubkey| vec![vec!["p".into(), pubkey]])
            .unwrap_or_default(),
        SessionKind::Group => {
            let Some(group) = seed
                .groups
                .iter()
                .find(|group| group.session_id == session_id)
            else {
                return Vec::new();
            };
            let contact_index = seed
                .contacts
                .iter()
                .map(|contact| (contact.id.as_str(), contact))
                .collect::<HashMap<_, _>>();

            group
                .members
                .iter()
                .filter_map(|member| contact_index.get(member.contact_id.as_str()).copied())
                .filter_map(|contact| normalize_nostr_pubkey(&contact.pubkey))
                .filter(|pubkey| seen_pubkeys.insert(pubkey.clone()))
                .map(|pubkey| vec!["p".into(), pubkey])
                .collect()
        }
        SessionKind::SelfChat => Vec::new(),
    }
}

fn message_text_note_tags(
    seed: &ChatDomainSeed,
    session_id: &str,
    reply_to: Option<&MessageReplyPreview>,
) -> Vec<Vec<String>> {
    let mut tags = session_text_note_tags(seed, session_id);
    if let Some(reply_reference_id) = reply_to.and_then(|reply| {
        reply
            .remote_id
            .as_deref()
            .or(Some(reply.message_id.as_str()))
    }) {
        tags.push(vec!["e".into(), reply_reference_id.into()]);
    }
    tags
}

fn resolve_reply_to_preview(
    seed: &ChatDomainSeed,
    session_id: &str,
    reply_to_message_id: Option<&str>,
) -> Result<Option<MessageReplyPreview>, String> {
    let Some(reply_to_message_id) = reply_to_message_id else {
        return Ok(None);
    };

    let message = seed
        .message_store
        .get(session_id)
        .and_then(|messages| {
            messages
                .iter()
                .find(|message| message.id == reply_to_message_id)
        })
        .ok_or_else(|| format!("reply target not found: {reply_to_message_id}"))?;
    Ok(Some(build_message_reply_preview(message)))
}

fn build_local_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    id: String,
    kind: MessageKind,
    body: String,
    meta: Option<String>,
    time: String,
    delivery_status: Option<MessageDeliveryStatus>,
    reply_to: Option<MessageReplyPreview>,
    text_note_tags: &[Vec<String>],
    text_note_signer: Option<&TextNoteSigner>,
) -> Result<MessageItem, String> {
    let delivery_status = delivery_status.map(|status| {
        stage_local_delivery_status(&kind, &MessageAuthor::Me, status, text_note_signer)
    });
    let mut message = MessageItem {
        id,
        kind,
        author: MessageAuthor::Me,
        body,
        time,
        meta,
        delivery_status,
        remote_id: None,
        sync_source: Some(MessageSyncSource::Local),
        acked_at: None,
        signed_nostr_event: None,
        reply_to,
    };
    hydrate_local_delivery_tracking(app_handle, &mut message, text_note_tags, text_note_signer)?;
    Ok(message)
}

fn with_delivery_status(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    mut message: MessageItem,
    delivery_status: MessageDeliveryStatus,
    text_note_tags: &[Vec<String>],
    text_note_signer: Option<&TextNoteSigner>,
) -> Result<MessageItem, String> {
    message.delivery_status = Some(stage_local_delivery_status(
        &message.kind,
        &message.author,
        delivery_status,
        text_note_signer,
    ));
    hydrate_local_delivery_tracking(app_handle, &mut message, text_note_tags, text_note_signer)?;
    Ok(message)
}

fn local_message_preview_text(kind: &MessageKind, body: &str, meta: Option<&str>) -> String {
    match kind {
        MessageKind::Text | MessageKind::System => body.into(),
        MessageKind::Image => format!("Shared image: {body}"),
        MessageKind::Video => format!("Shared video: {body}"),
        MessageKind::File => format!("Shared file: {body}"),
        MessageKind::Audio => format!("Audio: {}", meta.unwrap_or("Voice note")),
    }
}

fn stage_local_delivery_status(
    kind: &MessageKind,
    author: &MessageAuthor,
    delivery_status: MessageDeliveryStatus,
    text_note_signer: Option<&TextNoteSigner>,
) -> MessageDeliveryStatus {
    if text_note_signer.is_some()
        && matches!(author, MessageAuthor::Me)
        && matches!(kind, MessageKind::Text)
        && matches!(delivery_status, MessageDeliveryStatus::Sent)
    {
        MessageDeliveryStatus::Sending
    } else {
        delivery_status
    }
}

fn hydrate_local_delivery_tracking(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    message: &mut MessageItem,
    text_note_tags: &[Vec<String>],
    text_note_signer: Option<&TextNoteSigner>,
) -> Result<(), String> {
    if !matches!(message.author, MessageAuthor::Me) {
        return Ok(());
    }

    if message.sync_source.is_none() {
        message.sync_source = Some(MessageSyncSource::Local);
    }

    hydrate_local_signed_remote_id(app_handle, message, text_note_tags, text_note_signer)?;

    if matches!(message.delivery_status, Some(MessageDeliveryStatus::Sent)) {
        if message.remote_id.is_none() {
            message.remote_id = Some(acked_remote_message_id(&message.id));
        }
        if message.acked_at.is_none() {
            message.acked_at = Some(message.time.clone());
        }
    }

    Ok(())
}

fn acked_remote_message_id(message_id: &str) -> String {
    format!("relay-ack:{message_id}")
}

fn hydrate_local_signed_remote_id(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    message: &mut MessageItem,
    text_note_tags: &[Vec<String>],
    text_note_signer: Option<&TextNoteSigner>,
) -> Result<(), String> {
    if message.signed_nostr_event.is_some()
        || !matches!(message.kind, MessageKind::Text)
        || !matches!(message.author, MessageAuthor::Me)
    {
        return Ok(());
    }

    let Some(text_note_signer) = text_note_signer else {
        return Ok(());
    };
    let created_at = local_message_created_at(message.id.as_str());
    let signed_note =
        text_note_signer.sign_text_note(app_handle, &message.body, created_at, text_note_tags)?;
    if message
        .remote_id
        .as_ref()
        .is_some_and(|remote_id| remote_id != &signed_note.event_id)
    {
        return Ok(());
    }

    if message.remote_id.is_none() {
        message.remote_id = Some(signed_note.event_id.clone());
    }
    message.signed_nostr_event = Some(signed_note);
    Ok(())
}

fn normalize_nostr_pubkey(value: &str) -> Option<String> {
    let trimmed = value.trim();
    let normalized = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    NostrPublicKey::parse(normalized)
        .ok()
        .map(|pubkey| pubkey.to_hex())
}

fn local_message_created_at(message_id: &str) -> u64 {
    message_id
        .rsplit_once('-')
        .and_then(|(_, millis)| millis.parse::<u64>().ok())
        .map(|millis| millis / 1_000)
        .filter(|created_at| *created_at > 0)
        .unwrap_or_else(current_unix_timestamp)
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

struct SessionMutationPlan {
    session_id: String,
    session: SessionItem,
    initial_messages: Option<Vec<MessageItem>>,
}

fn build_direct_session_plan(
    seed: &ChatDomainSeed,
    circle_id: &str,
    contact: &ContactItem,
) -> SessionMutationPlan {
    if let Some(session) = seed.sessions.iter().find(|session| {
        session.circle_id == circle_id && session.contact_id.as_deref() == Some(contact.id.as_str())
    }) {
        let mut session = session.clone();
        session.archived = Some(false);
        return SessionMutationPlan {
            session_id: session.id.clone(),
            session,
            initial_messages: None,
        };
    }

    let session_id = build_unique_session_id(&format!("session-{}", contact.id), &seed.sessions);
    SessionMutationPlan {
        session_id: session_id.clone(),
        session: SessionItem {
            id: session_id.clone(),
            circle_id: circle_id.into(),
            name: contact.name.clone(),
            initials: contact.initials.clone(),
            subtitle: "Start a conversation".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::Direct,
            category: "friends".into(),
            members: None,
            contact_id: Some(contact.id.clone()),
            archived: None,
        },
        initial_messages: Some(vec![MessageItem {
            id: unique_local_id("system"),
            kind: MessageKind::System,
            author: MessageAuthor::System,
            body: format!("New conversation with {}", contact.name),
            time: String::new(),
            meta: None,
            delivery_status: None,
            remote_id: None,
            sync_source: Some(MessageSyncSource::System),
            acked_at: None,
            signed_nostr_event: None,
            reply_to: None,
        }]),
    }
}

fn build_self_session_plan(seed: &ChatDomainSeed, circle_id: &str) -> SessionMutationPlan {
    if let Some(session) = seed.sessions.iter().find(|session| {
        session.circle_id == circle_id && matches!(session.kind, SessionKind::SelfChat)
    }) {
        let mut session = session.clone();
        session.archived = Some(false);
        return SessionMutationPlan {
            session_id: session.id.clone(),
            session,
            initial_messages: None,
        };
    }

    let session_id = build_unique_session_id(&format!("self-{circle_id}"), &seed.sessions);
    SessionMutationPlan {
        session_id: session_id.clone(),
        session: SessionItem {
            id: session_id.clone(),
            circle_id: circle_id.into(),
            name: "Note to Self".into(),
            initials: "ME".into(),
            subtitle: "Private note space".into(),
            time: "now".into(),
            unread_count: None,
            muted: None,
            pinned: None,
            draft: None,
            kind: SessionKind::SelfChat,
            category: "system".into(),
            members: None,
            contact_id: None,
            archived: None,
        },
        initial_messages: Some(vec![MessageItem {
            id: unique_local_id("system"),
            kind: MessageKind::System,
            author: MessageAuthor::System,
            body: "Private note space opened".into(),
            time: String::new(),
            meta: None,
            delivery_status: None,
            remote_id: None,
            sync_source: Some(MessageSyncSource::System),
            acked_at: None,
            signed_nostr_event: None,
            reply_to: None,
        }]),
    }
}

fn apply_session_plan(change_set: &mut ChatDomainChangeSet, plan: SessionMutationPlan) {
    change_set.sessions_upsert.push(ChatUpsert {
        item: plan.session,
        move_to_top: true,
    });

    if let Some(messages) = plan.initial_messages {
        change_set
            .messages_replace
            .push((plan.session_id, messages));
    }
}

fn dedupe_contact_ids(contact_ids: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();

    contact_ids
        .into_iter()
        .filter_map(|contact_id| {
            let trimmed = contact_id.trim();
            if trimmed.is_empty() {
                return None;
            }

            if seen.insert(trimmed.to_string()) {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn normalized_group_name(raw_name: &str, contacts: &[ContactItem]) -> String {
    let trimmed = raw_name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    if contacts.is_empty() {
        return "New Group".into();
    }

    let preview = contacts
        .iter()
        .take(3)
        .map(|contact| contact.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    if contacts.len() > 3 {
        format!("{preview} +{}", contacts.len() - 3)
    } else {
        preview
    }
}

fn default_group_description(group_name: &str) -> String {
    format!("Group created from the new message flow in {group_name}.")
}

fn find_contact_for_lookup(seed: &ChatDomainSeed, query: &str) -> Option<ContactItem> {
    let trimmed = query.trim();
    let normalized = trimmed.to_lowercase();
    let query_pubkey = normalize_nostr_pubkey(trimmed);

    seed.contacts.iter().find_map(|contact| {
        let name_matches = contact.name.trim().to_lowercase() == normalized;
        let id_matches = contact.id.trim().eq_ignore_ascii_case(trimmed);
        let handle_matches = contact.handle.trim().eq_ignore_ascii_case(trimmed);
        let pubkey_matches = contact.pubkey.trim().eq_ignore_ascii_case(trimmed)
            || query_pubkey
                .as_ref()
                .zip(normalize_nostr_pubkey(&contact.pubkey))
                .is_some_and(|(query_pubkey, contact_pubkey)| query_pubkey == &contact_pubkey);

        if name_matches || id_matches || handle_matches || pubkey_matches {
            Some(contact.clone())
        } else {
            None
        }
    })
}

fn build_lookup_contact(seed: &ChatDomainSeed, query: &str) -> ContactItem {
    let trimmed = query.trim();
    let lowered = trimmed.to_lowercase();
    let normalized_pubkey = normalize_nostr_pubkey(trimmed);
    let slug = build_circle_slug(trimmed);
    let handle = if trimmed.starts_with('@') {
        trimmed.to_lowercase()
    } else {
        format!("@{slug}")
    };
    let name = if trimmed.starts_with('@') {
        humanize_identifier(trimmed.trim_start_matches('@'))
    } else if let Some(pubkey) = normalized_pubkey.as_ref() {
        format!(
            "Remote {}",
            pubkey.chars().take(6).collect::<String>().to_uppercase()
        )
    } else if trimmed.contains("://") {
        format!(
            "Invite {}",
            humanize_identifier(trimmed.rsplit("://").next().unwrap_or("link"))
        )
    } else {
        humanize_identifier(trimmed)
    };
    let pubkey = if let Some(pubkey) = normalized_pubkey {
        if lowered.starts_with("npub") {
            lowered
        } else {
            pubkey
        }
    } else {
        format!("lookup:{slug}")
    };

    ContactItem {
        id: build_unique_contact_id(&format!("lookup-{slug}"), &seed.contacts),
        name: if name.trim().is_empty() {
            "Remote Contact".into()
        } else {
            name
        },
        initials: build_initials(trimmed),
        handle,
        pubkey,
        subtitle: "Imported from lookup".into(),
        bio: format!("Created locally from lookup query `{trimmed}`."),
        online: Some(false),
        blocked: Some(false),
    }
}

fn build_unique_contact_id(base_id: &str, contacts: &[ContactItem]) -> String {
    let mut candidate = base_id.to_string();
    let mut suffix = 2;

    while contacts.iter().any(|contact| contact.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn build_initials(value: &str) -> String {
    let words = value
        .trim()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    if words.is_empty() {
        return "XC".into();
    }

    if words.len() == 1 {
        return words[0].chars().take(2).collect::<String>().to_uppercase();
    }

    words
        .iter()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
}

fn humanize_identifier(value: &str) -> String {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    chars.as_str().to_ascii_lowercase()
                ),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn load_domain_seed(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
) -> Result<ChatDomainSeed, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.load_domain_seed()
}

fn apply_domain_change_set(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    change_set: ChatDomainChangeSet,
) -> Result<ChatDomainSeed, String> {
    let repository = SqliteChatRepository::new(app_handle);
    repository.apply_change_set(change_set)?;
    let seed = repository.load_domain_seed()?;
    let _ = media_store::cleanup_chat_media_assets(app_handle, &seed);
    Ok(seed)
}

fn clear_transport_outbound_dispatch_for_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    session_id: &str,
    message_id: &str,
) -> Result<(), String> {
    let repository = SqliteTransportRepository::new(app_handle);
    let mut cache = repository.load_transport_cache()?;
    let dispatch_count = cache.outbound_dispatches.len();
    cache.outbound_dispatches.retain(|dispatch| {
        !(dispatch.session_id == session_id && dispatch.message_id == message_id)
    });
    let media_dispatch_count = cache.outbound_media_dispatches.len();
    cache.outbound_media_dispatches.retain(|dispatch| {
        !(dispatch.session_id == session_id && dispatch.message_id == message_id)
    });
    if cache.outbound_dispatches.len() == dispatch_count
        && cache.outbound_media_dispatches.len() == media_dispatch_count
    {
        return Ok(());
    }

    repository.save_transport_cache(cache)
}

fn normalized_circle_name(input: &AddCircleInput) -> String {
    let trimmed = input.name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    match input.mode {
        CircleCreateMode::Private => "Private Circle".into(),
        CircleCreateMode::Custom => "Custom Relay".into(),
        CircleCreateMode::Invite => "Invite Circle".into(),
    }
}

fn normalized_circle_relay(input: &AddCircleInput, normalized_name: &str) -> String {
    match input.mode {
        CircleCreateMode::Private => format!(
            "wss://{}.private.circle.local",
            build_circle_slug(normalized_name)
        ),
        CircleCreateMode::Custom => {
            normalize_custom_circle_relay(input.relay.clone().unwrap_or_default().as_str())
                .unwrap_or_else(|| input.relay.clone().unwrap_or_default().trim().to_string())
        }
        CircleCreateMode::Invite => {
            let invite_code = input.invite_code.clone().unwrap_or_default();
            if invite_code.trim().contains("://") {
                invite_code.trim().to_string()
            } else {
                format!("invite://{}", invite_code.trim())
            }
        }
    }
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

fn normalize_custom_circle_relay(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = resolve_public_relay_shortcut(trimmed)
        .map(str::to_string)
        .unwrap_or_else(|| {
            if trimmed.contains("://") {
                trimmed.to_string()
            } else {
                format!("wss://{trimmed}")
            }
        });
    let parsed = Url::parse(&candidate).ok()?;
    matches!(parsed.scheme(), "ws" | "wss")
        .then_some(candidate)
        .filter(|_| parsed.host_str().is_some())
}

fn default_circle_description(circle_type: &CircleType) -> &'static str {
    match circle_type {
        CircleType::Paid => "Private relay shell created from the onboarding flow.",
        CircleType::Custom => "Custom relay connected from a manually entered endpoint.",
        CircleType::Default | CircleType::Bitchat => {
            "Circle imported from an invite handoff and waiting for relay confirmation."
        }
    }
}

fn build_circle_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for character in value.trim().to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            last_was_separator = false;
            continue;
        }

        if !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "circle".into()
    } else {
        slug
    }
}

fn build_unique_circle_id(base_label: &str, circles: &[CircleItem]) -> String {
    let base_id = build_circle_slug(base_label);
    let mut candidate = base_id.clone();
    let mut suffix = 2;

    while circles.iter().any(|circle| circle.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn build_unique_session_id(base_id: &str, sessions: &[SessionItem]) -> String {
    let mut candidate = base_id.to_string();
    let mut suffix = 2;

    while sessions.iter().any(|session| session.id == candidate) {
        candidate = format!("{base_id}-{suffix}");
        suffix += 1;
    }

    candidate
}

fn unique_local_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{prefix}-{millis}")
}

struct MessageSendContext {
    text_note_signer: Option<TextNoteSigner>,
}

#[derive(Clone)]
enum TextNoteSigner {
    LocalSecret(StoredAuthRuntimeCredential),
    RemoteAuthRuntime(AuthSessionSummary),
}

impl TextNoteSigner {
    fn sign_text_note<R: tauri::Runtime>(
        &self,
        app_handle: &tauri::AppHandle<R>,
        content: &str,
        created_at: u64,
        tags: &[Vec<String>],
    ) -> Result<SignedNostrEvent, String> {
        match self {
            Self::LocalSecret(credential) => auth_access::sign_auth_runtime_text_note(
                &credential.secret_key_hex,
                content,
                created_at,
                tags.to_vec(),
            ),
            Self::RemoteAuthRuntime(auth_session) => {
                shell_auth::sign_remote_auth_runtime_text_note(
                    app_handle,
                    auth_session,
                    content,
                    created_at,
                    tags,
                )
            }
        }
    }
}

fn resolve_message_send_context(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
) -> Result<MessageSendContext, String> {
    let shell = shell_auth::load_saved_shell_snapshot(app_handle)?;

    if let Some(reason) = shell_auth::auth_runtime_send_block_reason(&shell) {
        return Err(format!("chat_send_blocked:{reason}"));
    }

    Ok(MessageSendContext {
        text_note_signer: resolve_text_note_signer(app_handle, &shell)?,
    })
}

fn resolve_text_note_signer<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    shell: &ShellStateSnapshot,
) -> Result<Option<TextNoteSigner>, String> {
    if let Some(credential) = resolve_local_signing_credential(app_handle, shell)? {
        return Ok(Some(TextNoteSigner::LocalSecret(credential)));
    }

    let Some(auth_session) = shell.auth_session.as_ref() else {
        return Ok(None);
    };
    let Some(auth_runtime) = shell.auth_runtime.as_ref() else {
        return Ok(None);
    };

    if matches!(
        auth_session.access.kind,
        LoginAccessKind::Bunker | LoginAccessKind::NostrConnect
    ) && matches!(auth_runtime.state, AuthRuntimeState::Connected)
    {
        return Ok(Some(TextNoteSigner::RemoteAuthRuntime(
            auth_session.clone(),
        )));
    }

    Ok(None)
}

fn resolve_local_signing_credential<R: tauri::Runtime>(
    app_handle: &tauri::AppHandle<R>,
    shell: &ShellStateSnapshot,
) -> Result<Option<StoredAuthRuntimeCredential>, String> {
    let Some(auth_session) = shell.auth_session.as_ref() else {
        return Ok(None);
    };

    if !matches!(
        auth_session.access.kind,
        LoginAccessKind::LocalProfile | LoginAccessKind::Nsec | LoginAccessKind::HexKey
    ) {
        return Ok(None);
    }

    Ok(
        auth_runtime_credential_store::load(app_handle)?.filter(|credential| {
            same_login_method(&credential.login_method, &auth_session.login_method)
                && same_login_access_kind(&credential.access_kind, &auth_session.access.kind)
                && credential.stored_at == auth_session.logged_in_at
                && auth_session
                    .access
                    .pubkey
                    .as_ref()
                    .map_or(true, |pubkey| pubkey == &credential.pubkey)
        }),
    )
}

fn same_login_method(left: &LoginMethod, right: &LoginMethod) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn same_login_access_kind(left: &LoginAccessKind, right: &LoginAccessKind) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{auth_access, shell_auth};
    use crate::domain::chat::{
        AuthRuntimeState, AuthRuntimeSummary, AuthSessionSummary, LoginAccessKind,
        LoginAccessSummary, LoginCircleSelectionMode, LoginMethod, ShellStateSnapshot,
    };
    use crate::domain::chat_repository::ChatRepository;
    use crate::domain::transport::{TransportOutboundDispatch, TransportOutboundMediaDispatch};
    use crate::domain::transport_repository::{TransportCache, TransportRepository};
    use crate::infra::sqlite_chat_repository::SqliteChatRepository;
    use crate::infra::sqlite_transport_repository::SqliteTransportRepository;
    use crate::infra::{
        auth_runtime_binding_store, auth_runtime_credential_store, shell_state_store,
    };
    use nostr_connect::prelude::{
        nip44, EventBuilder as NostrEventBuilder, JsonUtil, Keys as NostrKeys, NostrConnectMessage,
        NostrConnectRequest, NostrConnectResponse, PublicKey as NostrPublicKey, ResponseResult,
    };
    use secp256k1::{Secp256k1, SecretKey};
    use serde_json::json;
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
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
        let config_root = std::env::temp_dir().join(format!("p2p-chat-mutations-test-{unique}"));
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

    struct DetachedTestApp {
        app: tauri::App<tauri::test::MockRuntime>,
        config_root: PathBuf,
    }

    impl Drop for DetachedTestApp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.config_root);
        }
    }

    fn unique_test_config_root(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        let config_root = std::env::temp_dir().join(format!("{prefix}-{unique}"));
        std::fs::create_dir_all(&config_root).expect("failed to create detached test config root");
        config_root
    }

    fn activate_test_config_root(config_root: &Path) {
        std::env::set_var("XDG_CONFIG_HOME", config_root);
    }

    fn detached_test_app(prefix: &str) -> DetachedTestApp {
        let config_root = unique_test_config_root(prefix);
        activate_test_config_root(&config_root);

        DetachedTestApp {
            app: tauri::test::mock_app(),
            config_root,
        }
    }

    fn set_circle_status(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        circle_id: &str,
        status: CircleStatus,
    ) {
        let repository = SqliteChatRepository::new(app_handle);
        let mut seed = repository
            .load_domain_seed()
            .expect("failed to load domain seed");
        let circle = seed
            .circles
            .iter_mut()
            .find(|circle| circle.id == circle_id)
            .expect("missing circle");
        circle.status = status;
        repository
            .save_domain_seed(seed)
            .expect("failed to save domain seed");
    }

    fn transport_cache(app_handle: &tauri::AppHandle<tauri::test::MockRuntime>) -> TransportCache {
        SqliteTransportRepository::new(app_handle)
            .load_transport_cache()
            .expect("failed to load transport cache")
    }

    fn set_contact_pubkey(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        contact_id: &str,
        pubkey: &str,
    ) {
        let repository = SqliteChatRepository::new(app_handle);
        let mut seed = repository
            .load_domain_seed()
            .expect("failed to load domain seed");
        let contact = seed
            .contacts
            .iter_mut()
            .find(|contact| contact.id == contact_id)
            .expect("missing contact");
        contact.pubkey = pubkey.into();
        repository
            .save_domain_seed(seed)
            .expect("failed to save domain seed");
    }

    fn valid_test_pubkey_hex(secret_key_hex: &str) -> String {
        let secret_key =
            SecretKey::from_str(secret_key_hex).expect("valid test secret key should parse");
        let secp = Secp256k1::new();
        let (pubkey, _) = secret_key.x_only_public_key(&secp);
        pubkey
            .serialize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn spawn_http_asset_server(
        content_type: &str,
        body: &[u8],
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test asset listener should bind");
        let address = listener
            .local_addr()
            .expect("test asset listener address should resolve");
        let content_type = content_type.to_string();
        let body = body.to_vec();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener
                .accept()
                .expect("asset listener should accept one connection");
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let response_head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                body.len(),
                content_type
            );
            stream
                .write_all(response_head.as_bytes())
                .expect("asset response head should write");
            stream
                .write_all(&body)
                .expect("asset response body should write");
        });

        (format!("http://{address}/fixture"), handle)
    }

    fn seed_authenticated_shell_runtime(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        session_id: &str,
        runtime: AuthRuntimeSummary,
    ) {
        seed_authenticated_shell_runtime_with_secret(
            app_handle,
            session_id,
            "1111111111111111111111111111111111111111111111111111111111111111",
            runtime,
        );
    }

    fn seed_authenticated_shell_runtime_with_secret(
        app_handle: &tauri::AppHandle<tauri::test::MockRuntime>,
        session_id: &str,
        secret_key_hex: &str,
        runtime: AuthRuntimeSummary,
    ) {
        fn resolved_local_pubkey(secret_key_hex: &str) -> String {
            auth_access::resolve_auth_runtime_credential(&crate::domain::chat::LoginAccessInput {
                kind: LoginAccessKind::HexKey,
                value: Some(secret_key_hex.into()),
            })
            .expect("valid test secret should resolve")
            .expect("credential should be present")
            .pubkey
        }

        fn valid_binding_pubkey_hex(secret_key_hex: &str) -> String {
            let secret_key =
                SecretKey::from_str(secret_key_hex).expect("valid test secret key should parse");
            let secp = Secp256k1::new();
            let (pubkey, _) = secret_key.x_only_public_key(&secp);
            pubkey
                .serialize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect()
        }

        let binding_login_method = runtime.login_method.clone();
        let binding_access_kind = runtime.access_kind.clone();
        let binding_stored_at = runtime.updated_at.clone();
        let credential_login_method = runtime.login_method.clone();
        let credential_access_kind = runtime.access_kind.clone();
        let credential_stored_at = runtime.updated_at.clone();
        let resolved_pubkey = resolved_local_pubkey(secret_key_hex);
        let binding_pubkey = valid_binding_pubkey_hex(secret_key_hex);
        let binding_value = match binding_access_kind {
            LoginAccessKind::Bunker => Some(format!(
                "bunker://{binding_pubkey}?relay=wss://relay.example.com"
            )),
            LoginAccessKind::NostrConnect => Some(format!(
                "nostrconnect://{binding_pubkey}?relay=wss://relay.example.com&secret=shared-secret"
            )),
            _ => None,
        };
        let seed = load_domain_seed(app_handle).expect("failed to load domain seed");
        let active_circle_id = seed
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| session.circle_id.clone())
            .or_else(|| seed.circles.first().map(|circle| circle.id.clone()))
            .unwrap_or_default();
        let mut shell = ShellStateSnapshot::from(seed);
        shell.is_authenticated = true;
        shell.auth_session = Some(AuthSessionSummary {
            login_method: runtime.login_method.clone(),
            access: LoginAccessSummary {
                kind: runtime.access_kind.clone(),
                label: runtime.label.clone(),
                pubkey: runtime
                    .pubkey
                    .clone()
                    .or_else(|| match runtime.access_kind {
                        LoginAccessKind::LocalProfile
                        | LoginAccessKind::Nsec
                        | LoginAccessKind::HexKey => Some(resolved_pubkey.clone()),
                        _ => None,
                    }),
            },
            circle_selection_mode: LoginCircleSelectionMode::Existing,
            logged_in_at: runtime.updated_at.clone(),
        });
        shell.auth_runtime = Some(runtime);
        shell.active_circle_id = active_circle_id;
        shell.selected_session_id = session_id.into();

        shell_state_store::save(
            app_handle,
            serde_json::to_value(shell).expect("failed to encode shell state"),
        )
        .expect("failed to seed shell state store");

        if matches!(
            credential_access_kind,
            LoginAccessKind::LocalProfile | LoginAccessKind::Nsec | LoginAccessKind::HexKey
        ) {
            auth_runtime_credential_store::save(
                app_handle,
                &auth_runtime_credential_store::StoredAuthRuntimeCredential {
                    login_method: credential_login_method,
                    access_kind: credential_access_kind,
                    secret_key_hex: secret_key_hex.into(),
                    pubkey: resolved_pubkey,
                    stored_at: credential_stored_at,
                },
            )
            .expect("failed to seed auth runtime credential store");
        }

        if let Some(value) = binding_value {
            auth_runtime_binding_store::save(
                app_handle,
                &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                    login_method: binding_login_method,
                    access_kind: binding_access_kind,
                    value: value.into(),
                    stored_at: binding_stored_at,
                },
            )
            .expect("failed to seed auth runtime binding store");
        }
    }

    fn connected_local_auth_runtime(updated_at: &str) -> AuthRuntimeSummary {
        AuthRuntimeSummary {
            state: AuthRuntimeState::Connected,
            login_method: LoginMethod::ExistingAccount,
            access_kind: LoginAccessKind::HexKey,
            label: "npub1local".into(),
            pubkey: None,
            error: None,
            can_send_messages: true,
            send_blocked_reason: None,
            persisted_in_native_store: false,
            credential_persisted_in_native_store: true,
            updated_at: updated_at.into(),
        }
    }

    const TEST_BUNKER_SIGNER_SECRET_KEY: &str =
        "2222222222222222222222222222222222222222222222222222222222222222";
    const TEST_BUNKER_USER_SECRET_KEY: &str =
        "3333333333333333333333333333333333333333333333333333333333333333";
    const TEST_BUNKER_SHARED_SECRET: &str = "shared-secret";

    fn bunker_signer_public_key_hex() -> String {
        NostrKeys::parse(TEST_BUNKER_SIGNER_SECRET_KEY)
            .expect("test bunker signer secret should parse")
            .public_key()
            .to_hex()
    }

    fn unreachable_test_relay_url() -> String {
        "ws://127.0.0.1:9".into()
    }

    fn spawn_bunker_signer_relay_server() -> (String, std::thread::JoinHandle<()>) {
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("test relay listener should bind");
        let address = listener
            .local_addr()
            .expect("test relay listener address should resolve");
        let handle = std::thread::spawn(move || {
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
                                    matches!(&request, NostrConnectRequest::SignEvent(_));
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
                                            .expect("relay should sign remote text note");
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

    #[test]
    fn start_self_conversation_creates_session_for_circle_without_self_chat() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "paid-circle".into(),
            },
        )
        .expect("failed to start self conversation");

        let session = result
            .seed
            .sessions
            .iter()
            .find(|session| session.id == result.session_id)
            .expect("missing self session");
        assert!(matches!(session.kind, SessionKind::SelfChat));
        assert_eq!(session.circle_id, "paid-circle");
        assert_eq!(session.name, "Note to Self");
    }

    #[test]
    fn send_message_moves_updated_session_to_front() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Need this thread back on top.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        assert_eq!(
            seed.sessions.first().map(|session| session.id.as_str()),
            Some("mika")
        );
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.draft, None);
        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended message");
        assert_eq!(last_message.body, "Need this thread back on top.");
        assert!(matches!(last_message.author, MessageAuthor::Me));
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(
            last_message
                .sync_source
                .as_ref()
                .map(|source| match source {
                    MessageSyncSource::Local => "local",
                    MessageSyncSource::Relay => "relay",
                    MessageSyncSource::System => "system",
                }),
            Some("local")
        );
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(last_message.acked_at.as_deref(), Some("now"));
    }

    #[test]
    fn send_message_in_connecting_circle_marks_message_sending() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "nora".into(),
                body: "Circle still connecting.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        let last_message = seed
            .message_store
            .get("nora")
            .and_then(|messages| messages.last())
            .expect("missing appended message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        assert_eq!(last_message.remote_id, None);
        assert_eq!(last_message.acked_at, None);
    }

    #[test]
    fn send_message_in_connecting_circle_with_local_auth_sets_signed_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "nora",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "nora".into(),
                body: "Circle still connecting, but the note is already signed.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        let last_message = seed
            .message_store
            .get("nora")
            .and_then(|messages| messages.last())
            .expect("missing appended message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        assert!(last_message
            .remote_id
            .as_deref()
            .is_some_and(is_lower_hex_64));
        assert_signed_nostr_event(
            last_message,
            "Circle still connecting, but the note is already signed.",
        );
        assert_eq!(last_message.acked_at, None);
    }

    #[test]
    fn send_message_in_connecting_circle_with_quick_start_local_profile_sets_signed_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "nora",
            AuthRuntimeSummary {
                state: AuthRuntimeState::LocalProfile,
                login_method: LoginMethod::QuickStart,
                access_kind: LoginAccessKind::LocalProfile,
                label: "Quick Start".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "nora".into(),
                body: "Quick Start accounts should also sign messages.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send quick start message");

        let last_message = seed
            .message_store
            .get("nora")
            .and_then(|messages| messages.last())
            .expect("missing appended quick start message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        assert!(last_message
            .remote_id
            .as_deref()
            .is_some_and(is_lower_hex_64));
        assert_signed_nostr_event(
            last_message,
            "Quick Start accounts should also sign messages.",
        );
        assert_eq!(last_message.acked_at, None);
    }

    #[test]
    fn send_direct_message_with_local_auth_sets_contact_p_tag() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let contact_pubkey = valid_test_pubkey_hex(
            "6666666666666666666666666666666666666666666666666666666666666666",
        );
        set_contact_pubkey(app_handle, "mika-contact", &contact_pubkey);
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Direct session should carry one recipient tag.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send direct message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended message");
        let signed_note = last_message
            .signed_nostr_event
            .as_ref()
            .expect("direct message should be signed");

        assert_eq!(signed_note.tags, vec![vec!["p".into(), contact_pubkey]]);
    }

    #[test]
    fn send_reply_message_persists_reply_preview_and_sets_e_tag() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "main-circle".into(),
            },
        )
        .expect("failed to start self conversation");
        seed_authenticated_shell_runtime(
            app_handle,
            &started.session_id,
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let first_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Original message to reply to.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send original message");
        let replied_message = first_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing original message");
        let reply_reference_id = replied_message
            .remote_id
            .clone()
            .expect("signed original message should have remote id");

        let reply_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Reply that should carry an e tag.".into(),
                reply_to_message_id: Some(replied_message.id.clone()),
            },
        )
        .expect("failed to send reply message");
        let reply_message = reply_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing reply message");
        let reply_preview = reply_message
            .reply_to
            .as_ref()
            .expect("reply message should persist preview");
        let signed_event = reply_message
            .signed_nostr_event
            .as_ref()
            .expect("reply message should include signed event");

        assert_eq!(reply_preview.message_id, replied_message.id);
        assert_eq!(
            reply_preview.remote_id.as_deref(),
            Some(reply_reference_id.as_str())
        );
        assert!(matches!(reply_preview.author, MessageAuthor::Me));
        assert_eq!(reply_preview.author_label, "You");
        assert!(matches!(reply_preview.kind, MessageKind::Text));
        assert_eq!(reply_preview.snippet, "Original message to reply to.");
        assert_eq!(
            signed_event.tags,
            vec![vec!["e".into(), reply_reference_id]]
        );
    }

    #[test]
    fn send_file_message_persists_file_metadata_and_session_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = send_file_message(
            app_handle,
            SendFileMessageInput {
                session_id: "mika".into(),
                name: "roadmap.pdf".into(),
                meta: Some("PDF · 2.4 MB".into()),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send file message");

        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Shared file: roadmap.pdf");
        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended file message");

        assert!(matches!(last_message.kind, MessageKind::File));
        assert_eq!(last_message.body, "roadmap.pdf");
        assert_eq!(last_message.meta.as_deref(), Some("PDF · 2.4 MB"));
        assert!(matches!(last_message.author, MessageAuthor::Me));
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(last_message.acked_at.as_deref(), Some("now"));
        assert!(last_message.signed_nostr_event.is_none());
        assert!(last_message.reply_to.is_none());
    }

    #[test]
    fn send_file_message_with_local_auth_does_not_create_signed_nostr_event() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_file_message(
            app_handle,
            SendFileMessageInput {
                session_id: "mika".into(),
                name: "field-notes.txt".into(),
                meta: Some("TXT · 12 KB".into()),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send file message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended file message");

        assert!(matches!(last_message.kind, MessageKind::File));
        assert!(last_message.signed_nostr_event.is_none());
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
    }

    #[test]
    fn send_file_message_preserves_reply_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "main-circle".into(),
            },
        )
        .expect("failed to start self conversation");

        let first_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Message that the file upload will reply to.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send original message");
        let replied_message = first_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing original message");

        let reply_seed = send_file_message(
            app_handle,
            SendFileMessageInput {
                session_id: started.session_id.clone(),
                name: "diagram.png".into(),
                meta: Some("PNG · 88 KB".into()),
                reply_to_message_id: Some(replied_message.id.clone()),
            },
        )
        .expect("failed to send reply file message");
        let reply_message = reply_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing reply file message");
        let reply_preview = reply_message
            .reply_to
            .as_ref()
            .expect("file reply should persist preview");

        assert!(matches!(reply_message.kind, MessageKind::File));
        assert_eq!(reply_preview.message_id, replied_message.id);
        assert_eq!(
            reply_preview.remote_id.as_deref(),
            replied_message.remote_id.as_deref()
        );
        assert!(matches!(reply_preview.author, MessageAuthor::Me));
        assert_eq!(reply_preview.author_label, "You");
        assert!(matches!(reply_preview.kind, MessageKind::Text));
        assert_eq!(
            reply_preview.snippet,
            "Message that the file upload will reply to."
        );
        assert!(reply_message.signed_nostr_event.is_none());
    }

    #[test]
    fn send_image_message_persists_meta_and_session_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = send_image_message(
            app_handle,
            SendImageMessageInput {
                session_id: "mika".into(),
                name: "harbor-sunrise.png".into(),
                meta: "{\"version\":1,\"label\":\"PNG · 1280 x 720 · 84 KB\",\"previewDataUrl\":\"data:image/png;base64,preview\"}".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send image message");

        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Shared image: harbor-sunrise.png");
        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended image message");

        assert!(matches!(last_message.kind, MessageKind::Image));
        assert_eq!(last_message.body, "harbor-sunrise.png");
        assert_eq!(
            last_message.meta.as_deref(),
            Some(
                "{\"version\":1,\"label\":\"PNG · 1280 x 720 · 84 KB\",\"previewDataUrl\":\"data:image/png;base64,preview\"}"
            )
        );
        assert!(matches!(last_message.author, MessageAuthor::Me));
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(last_message.acked_at.as_deref(), Some("now"));
        assert!(last_message.signed_nostr_event.is_none());
        assert!(last_message.reply_to.is_none());
    }

    #[test]
    fn cache_chat_message_media_downloads_remote_image_and_updates_meta() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (remote_url, server_handle) = spawn_http_asset_server("image/png", b"remote-image");

        merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-image-remote-only".into(),
                    kind: MessageKind::Image,
                    author: MessageAuthor::Peer,
                    body: "remote-image.png".into(),
                    time: "09:51".into(),
                    meta: Some(
                        json!({
                            "version": 3,
                            "label": "PNG · remote",
                            "remoteUrl": remote_url
                        })
                        .to_string(),
                    ),
                    delivery_status: None,
                    remote_id: Some("relay:image-remote-only".into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            },
        )
        .expect("failed to seed remote-only image message");

        let result = cache_chat_message_media(
            app_handle,
            CacheChatMessageMediaInput {
                session_id: "mika".into(),
                message_id: "relay-image-remote-only".into(),
            },
        )
        .expect("remote image cache should succeed");
        server_handle
            .join()
            .expect("asset server should finish cleanly");

        assert!(result.local_path.contains("chat-media/images/"));
        assert_eq!(
            std::fs::read(&result.local_path).expect("cached image should be readable"),
            b"remote-image"
        );

        let cached_message = result
            .seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == "relay-image-remote-only")
            })
            .expect("cached message should remain in session");
        let meta = serde_json::from_str::<serde_json::Value>(
            cached_message
                .meta
                .as_deref()
                .expect("cached message should keep meta"),
        )
        .expect("cached meta should be valid json");

        assert_eq!(
            meta.get("version").and_then(|value| value.as_u64()),
            Some(3)
        );
        assert_eq!(
            meta.get("remoteUrl").and_then(|value| value.as_str()),
            Some(remote_url.as_str())
        );
        assert_eq!(
            meta.get("localPath").and_then(|value| value.as_str()),
            Some(result.local_path.as_str())
        );
    }

    #[test]
    fn update_chat_message_media_remote_url_preserves_local_image_path() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent = send_image_message(
            app_handle,
            SendImageMessageInput {
                session_id: "mika".into(),
                name: "harbor-sunrise.png".into(),
                meta: json!({
                    "version": 2,
                    "label": "PNG · 1280 x 720 · 84 KB",
                    "localPath": "/tmp/chat-media/images/harbor-sunrise.png"
                })
                .to_string(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to seed image message");
        let message = sent
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing seeded image message");

        let result = update_chat_message_media_remote_url(
            app_handle,
            UpdateChatMessageMediaRemoteUrlInput {
                session_id: "mika".into(),
                message_id: message.id.clone(),
                remote_url: " https://cdn.example.test/chat-media/harbor-sunrise.png ".into(),
            },
        )
        .expect("remote url update should succeed");

        assert_eq!(
            result.remote_url,
            "https://cdn.example.test/chat-media/harbor-sunrise.png"
        );
        let updated_message = result
            .seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.iter().find(|candidate| candidate.id == message.id))
            .expect("updated image message should remain in session");
        let meta = serde_json::from_str::<serde_json::Value>(
            updated_message
                .meta
                .as_deref()
                .expect("updated image message should keep meta"),
        )
        .expect("updated image meta should be valid json");

        assert_eq!(
            meta.get("version").and_then(|value| value.as_u64()),
            Some(3)
        );
        assert_eq!(
            meta.get("localPath").and_then(|value| value.as_str()),
            Some("/tmp/chat-media/images/harbor-sunrise.png")
        );
        assert_eq!(
            meta.get("remoteUrl").and_then(|value| value.as_str()),
            Some("https://cdn.example.test/chat-media/harbor-sunrise.png")
        );
    }

    #[test]
    fn update_chat_message_media_remote_url_clears_transport_outbound_media_dispatch_record() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent = send_image_message(
            app_handle,
            SendImageMessageInput {
                session_id: "mika".into(),
                name: "harbor-sunrise.png".into(),
                meta: json!({
                    "version": 2,
                    "label": "PNG · 1280 x 720 · 84 KB",
                    "localPath": "/tmp/chat-media/images/harbor-sunrise.png"
                })
                .to_string(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to seed image message");
        let message = sent
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing seeded image message");

        SqliteTransportRepository::new(app_handle)
            .save_transport_cache(TransportCache {
                outbound_media_dispatches: vec![TransportOutboundMediaDispatch {
                    circle_id: "bitchat-circle".into(),
                    session_id: "mika".into(),
                    message_id: message.id.clone(),
                    remote_id: message
                        .remote_id
                        .clone()
                        .expect("media message should have remote id"),
                    local_path: "/tmp/chat-media/images/harbor-sunrise.png".into(),
                    runtime_generation: 1,
                    request_id: "publish:bitchat-circle:1".into(),
                    dispatched_at: "now".into(),
                }],
                ..TransportCache::default()
            })
            .expect("failed to seed transport outbound media dispatch");

        update_chat_message_media_remote_url(
            app_handle,
            UpdateChatMessageMediaRemoteUrlInput {
                session_id: "mika".into(),
                message_id: message.id.clone(),
                remote_url: "https://cdn.example.test/chat-media/harbor-sunrise.png".into(),
            },
        )
        .expect("remote url update should succeed");

        let cache = transport_cache(app_handle);
        assert!(cache.outbound_media_dispatches.is_empty());
    }

    #[test]
    fn update_chat_message_media_remote_url_promotes_plain_file_meta() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent = send_file_message(
            app_handle,
            SendFileMessageInput {
                session_id: "mika".into(),
                name: "roadmap.pdf".into(),
                meta: Some("PDF · 2.4 MB".into()),
                reply_to_message_id: None,
            },
        )
        .expect("failed to seed file message");
        let message = sent
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing seeded file message");

        let result = update_chat_message_media_remote_url(
            app_handle,
            UpdateChatMessageMediaRemoteUrlInput {
                session_id: "mika".into(),
                message_id: message.id.clone(),
                remote_url: "https://cdn.example.test/chat-media/roadmap.pdf".into(),
            },
        )
        .expect("plain file meta should promote to remote-capable shape");

        let updated_message = result
            .seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.iter().find(|candidate| candidate.id == message.id))
            .expect("updated file message should remain in session");

        let meta = serde_json::from_str::<serde_json::Value>(
            updated_message
                .meta
                .as_deref()
                .expect("updated file message should keep meta"),
        )
        .expect("updated file meta should be valid json");

        assert_eq!(
            meta.get("version").and_then(|value| value.as_u64()),
            Some(2)
        );
        assert_eq!(
            meta.get("label").and_then(|value| value.as_str()),
            Some("PDF · 2.4 MB")
        );
        assert_eq!(
            meta.get("remoteUrl").and_then(|value| value.as_str()),
            Some("https://cdn.example.test/chat-media/roadmap.pdf")
        );
    }

    #[test]
    fn send_image_message_with_local_auth_does_not_create_signed_nostr_event() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_image_message(
            app_handle,
            SendImageMessageInput {
                session_id: "mika".into(),
                name: "whiteboard.jpg".into(),
                meta: "{\"version\":1,\"label\":\"JPG · 1600 x 900 · 120 KB\",\"previewDataUrl\":\"data:image/jpeg;base64,preview\"}".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send image message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended image message");

        assert!(matches!(last_message.kind, MessageKind::Image));
        assert!(last_message.signed_nostr_event.is_none());
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
    }

    #[test]
    fn send_image_message_preserves_reply_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "main-circle".into(),
            },
        )
        .expect("failed to start self conversation");

        let first_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Message that the image upload will reply to.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send original message");
        let replied_message = first_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing original message");

        let reply_seed = send_image_message(
            app_handle,
            SendImageMessageInput {
                session_id: started.session_id.clone(),
                name: "wireframe.png".into(),
                meta: "{\"version\":1,\"label\":\"PNG · 1440 x 900 · 96 KB\",\"previewDataUrl\":\"data:image/png;base64,preview\"}".into(),
                reply_to_message_id: Some(replied_message.id.clone()),
            },
        )
        .expect("failed to send reply image message");
        let reply_message = reply_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing reply image message");
        let reply_preview = reply_message
            .reply_to
            .as_ref()
            .expect("image reply should persist preview");

        assert!(matches!(reply_message.kind, MessageKind::Image));
        assert_eq!(reply_preview.message_id, replied_message.id);
        assert_eq!(
            reply_preview.remote_id.as_deref(),
            replied_message.remote_id.as_deref()
        );
        assert!(matches!(reply_preview.author, MessageAuthor::Me));
        assert_eq!(reply_preview.author_label, "You");
        assert!(matches!(reply_preview.kind, MessageKind::Text));
        assert_eq!(
            reply_preview.snippet,
            "Message that the image upload will reply to."
        );
        assert!(reply_message.signed_nostr_event.is_none());
    }

    #[test]
    fn send_video_message_persists_meta_and_session_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = send_video_message(
            app_handle,
            SendVideoMessageInput {
                session_id: "mika".into(),
                name: "harbor-walkthrough.mp4".into(),
                meta: "{\"version\":1,\"label\":\"MP4 · 1920 x 1080 · 0:14 · 2.4 MB\",\"previewDataUrl\":\"data:video/mp4;base64,preview\"}".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send video message");

        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Shared video: harbor-walkthrough.mp4");
        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended video message");

        assert!(matches!(last_message.kind, MessageKind::Video));
        assert_eq!(last_message.body, "harbor-walkthrough.mp4");
        assert_eq!(
            last_message.meta.as_deref(),
            Some(
                "{\"version\":1,\"label\":\"MP4 · 1920 x 1080 · 0:14 · 2.4 MB\",\"previewDataUrl\":\"data:video/mp4;base64,preview\"}"
            )
        );
        assert!(matches!(last_message.author, MessageAuthor::Me));
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert!(matches!(
            last_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(last_message.acked_at.as_deref(), Some("now"));
        assert!(last_message.signed_nostr_event.is_none());
        assert!(last_message.reply_to.is_none());
    }

    #[test]
    fn send_video_message_with_local_auth_does_not_create_signed_nostr_event() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_video_message(
            app_handle,
            SendVideoMessageInput {
                session_id: "mika".into(),
                name: "whiteboard-demo.webm".into(),
                meta: "{\"version\":1,\"label\":\"WEBM · 1280 x 720 · 0:08 · 640 KB\",\"previewDataUrl\":\"data:video/webm;base64,preview\"}".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send video message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended video message");

        assert!(matches!(last_message.kind, MessageKind::Video));
        assert!(last_message.signed_nostr_event.is_none());
        let expected_remote_id = format!("relay-ack:{}", last_message.id);
        assert_eq!(
            last_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
    }

    #[test]
    fn send_video_message_preserves_reply_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "main-circle".into(),
            },
        )
        .expect("failed to start self conversation");

        let first_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Message that the video upload will reply to.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send original message");
        let replied_message = first_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing original message");

        let reply_seed = send_video_message(
            app_handle,
            SendVideoMessageInput {
                session_id: started.session_id.clone(),
                name: "wireframe-walkthrough.mp4".into(),
                meta: "{\"version\":1,\"label\":\"MP4 · 1600 x 900 · 0:11 · 1.1 MB\",\"previewDataUrl\":\"data:video/mp4;base64,preview\"}".into(),
                reply_to_message_id: Some(replied_message.id.clone()),
            },
        )
        .expect("failed to send reply video message");
        let reply_message = reply_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing reply video message");
        let reply_preview = reply_message
            .reply_to
            .as_ref()
            .expect("video reply should persist preview");

        assert!(matches!(reply_message.kind, MessageKind::Video));
        assert_eq!(reply_preview.message_id, replied_message.id);
        assert_eq!(
            reply_preview.remote_id.as_deref(),
            replied_message.remote_id.as_deref()
        );
        assert!(matches!(reply_preview.author, MessageAuthor::Me));
        assert_eq!(reply_preview.author_label, "You");
        assert!(matches!(reply_preview.kind, MessageKind::Text));
        assert_eq!(
            reply_preview.snippet,
            "Message that the video upload will reply to."
        );
        assert!(reply_message.signed_nostr_event.is_none());
    }

    #[test]
    fn send_message_in_open_circle_with_local_auth_waits_for_runtime_receipt() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Open circle should still wait for relay OK.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(last_message
            .remote_id
            .as_deref()
            .is_some_and(is_lower_hex_64));
        assert_signed_nostr_event(last_message, "Open circle should still wait for relay OK.");
        assert_eq!(last_message.acked_at, None);
    }

    #[test]
    fn send_group_message_with_local_auth_sets_group_member_p_tags() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        set_contact_pubkey(
            app_handle,
            "alice-contact",
            &valid_test_pubkey_hex(
                "4444444444444444444444444444444444444444444444444444444444444444",
            ),
        );
        set_contact_pubkey(
            app_handle,
            "mika-contact",
            &valid_test_pubkey_hex(
                "5555555555555555555555555555555555555555555555555555555555555555",
            ),
        );
        let created = create_group_conversation(
            app_handle,
            CreateGroupConversationInput {
                circle_id: "main-circle".into(),
                name: "Launch Crew".into(),
                member_contact_ids: vec!["alice-contact".into(), "mika-contact".into()],
            },
        )
        .expect("failed to create group conversation");
        seed_authenticated_shell_runtime(
            app_handle,
            &created.session_id,
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: created.session_id.clone(),
                body: "Group tags should follow the configured members.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send group message");

        let last_message = seed
            .message_store
            .get(&created.session_id)
            .and_then(|messages| messages.last())
            .expect("missing appended message");
        let signed_note = last_message
            .signed_nostr_event
            .as_ref()
            .expect("group message should be signed");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(
            signed_note.tags,
            vec![
                vec![
                    "p".into(),
                    valid_test_pubkey_hex(
                        "4444444444444444444444444444444444444444444444444444444444444444",
                    ),
                ],
                vec![
                    "p".into(),
                    valid_test_pubkey_hex(
                        "5555555555555555555555555555555555555555555555555555555555555555",
                    ),
                ],
            ]
        );
    }

    #[test]
    fn send_message_in_open_circle_with_remote_bunker_auth_waits_for_runtime_receipt() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (relay_url, relay_handle) = spawn_bunker_signer_relay_server();
        let signer_pubkey = bunker_signer_public_key_hex();

        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                label: "bunker://signer.example".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: "2026-04-19T09:20:00Z".into(),
            },
        );
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: "2026-04-19T09:20:00Z".into(),
            },
        )
        .expect("failed to seed remote bunker binding");

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Remote bunker signer should also wait for relay OK.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(last_message
            .remote_id
            .as_deref()
            .is_some_and(is_lower_hex_64));
        assert_signed_nostr_event(
            last_message,
            "Remote bunker signer should also wait for relay OK.",
        );
        assert_eq!(last_message.acked_at, None);

        relay_handle
            .join()
            .expect("test relay thread should finish cleanly");
    }

    #[test]
    fn send_message_with_remote_bunker_auth_persists_runtime_sign_error() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let relay_url = unreachable_test_relay_url();
        let signer_pubkey = bunker_signer_public_key_hex();

        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                label: "bunker://signer.example".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: "2026-04-19T09:21:00Z".into(),
            },
        );
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: "2026-04-19T09:21:00Z".into(),
            },
        )
        .expect("failed to seed remote bunker binding");

        let error = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "This bunker sign should fail and persist the runtime error.".into(),
                reply_to_message_id: None,
            },
        )
        .expect_err("send should fail when remote bunker signing is unavailable");

        assert!(error.contains("Remote bunker sign_event failed:"));

        let shell = shell_auth::load_saved_shell_snapshot(app_handle)
            .expect("failed to reload shell after remote sign failure");
        let runtime = shell.auth_runtime.expect("missing updated auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert!(runtime
            .error
            .as_deref()
            .is_some_and(|value| value.contains("Remote bunker sign_event failed:")));
        assert_ne!(runtime.updated_at, "2026-04-19T09:21:00Z");
    }

    #[test]
    fn send_message_with_remote_bunker_auth_clears_previous_runtime_sign_error() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (relay_url, relay_handle) = spawn_bunker_signer_relay_server();
        let signer_pubkey = bunker_signer_public_key_hex();

        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                label: "bunker://signer.example".into(),
                pubkey: None,
                error: Some("Previous bunker sign attempt failed.".into()),
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: "2026-04-19T09:22:00Z".into(),
            },
        );
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                value: format!(
                    "bunker://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}"
                ),
                stored_at: "2026-04-19T09:22:00Z".into(),
            },
        )
        .expect("failed to seed remote bunker binding");

        send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "A successful bunker sign should clear the previous runtime error.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("send should succeed with a reachable remote bunker signer");

        let shell = shell_auth::load_saved_shell_snapshot(app_handle)
            .expect("failed to reload shell after remote sign success");
        let runtime = shell.auth_runtime.expect("missing updated auth runtime");
        assert!(matches!(runtime.state, AuthRuntimeState::Connected));
        assert!(runtime.can_send_messages);
        assert_eq!(runtime.send_blocked_reason, None);
        assert_eq!(runtime.error, None);
        assert_ne!(runtime.updated_at, "2026-04-19T09:22:00Z");

        relay_handle
            .join()
            .expect("test relay thread should finish cleanly");
    }

    #[test]
    fn send_message_rejects_when_auth_runtime_cannot_send() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
                state: AuthRuntimeState::Failed,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::Npub,
                label: "npub1readonly".into(),
                pubkey: None,
                error: Some("Read-only npub import cannot sign messages yet.".into()),
                can_send_messages: false,
                send_blocked_reason: Some("Read-only npub import cannot sign messages yet.".into()),
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let error = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "This should be blocked by auth runtime.".into(),
                reply_to_message_id: None,
            },
        )
        .expect_err("send should be blocked by auth runtime");

        assert_eq!(
            error,
            "chat_send_blocked:Read-only npub import cannot sign messages yet."
        );
    }

    #[test]
    fn send_message_in_open_circle_with_remote_nostrconnect_auth_waits_for_runtime_receipt() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let (relay_url, relay_handle) = spawn_bunker_signer_relay_server();
        let signer_pubkey = bunker_signer_public_key_hex();

        seed_authenticated_shell_runtime(
            app_handle,
            "mika",
            AuthRuntimeSummary {
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
                updated_at: "2026-04-19T09:10:00Z".into(),
            },
        );
        auth_runtime_binding_store::save(
            app_handle,
            &auth_runtime_binding_store::StoredAuthRuntimeBinding {
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::NostrConnect,
                value: format!(
                    "nostrconnect://{signer_pubkey}?relay={relay_url}&secret={TEST_BUNKER_SHARED_SECRET}&perms=sign_event&name=Desk%20Client"
                ),
                stored_at: "2026-04-19T09:10:00Z".into(),
            },
        )
        .expect("failed to seed remote nostrConnect binding");

        let seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Remote nostrConnect signer should also wait for relay OK.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");

        let last_message = seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .expect("missing appended message");

        assert!(matches!(
            last_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert!(last_message
            .remote_id
            .as_deref()
            .is_some_and(is_lower_hex_64));
        assert_signed_nostr_event(
            last_message,
            "Remote nostrConnect signer should also wait for relay OK.",
        );
        assert_eq!(last_message.acked_at, None);

        relay_handle
            .join()
            .expect("test relay thread should finish cleanly");
    }

    #[test]
    fn update_session_draft_persists_and_clears_draft() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = update_session_draft(
            app_handle,
            UpdateSessionDraftInput {
                session_id: "alice".into(),
                draft: "Draft relay follow-up".into(),
            },
        )
        .expect("failed to update session draft");

        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "alice")
            .expect("missing updated draft session");
        assert_eq!(session.draft.as_deref(), Some("Draft relay follow-up"));

        let cleared_seed = update_session_draft(
            app_handle,
            UpdateSessionDraftInput {
                session_id: "alice".into(),
                draft: String::new(),
            },
        )
        .expect("failed to clear session draft");

        let cleared_session = cleared_seed
            .sessions
            .iter()
            .find(|session| session.id == "alice")
            .expect("missing cleared draft session");
        assert_eq!(cleared_session.draft, None);
    }

    #[test]
    fn clear_unread_session_action_persists_session_state() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "remote-mika-clear-unread".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: "Unread should persist as cleared.".into(),
                    time: "now".into(),
                    meta: None,
                    delivery_status: None,
                    remote_id: Some("relay:mika:clear-unread".into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            },
        )
        .expect("failed to seed unread state");
        let unread_session = merged_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing mika session after unread seed");
        assert_eq!(unread_session.unread_count, Some(1));

        let cleared_seed = apply_session_action(
            app_handle,
            SessionActionInput {
                session_id: "mika".into(),
                action: ChatSessionAction::ClearUnread,
            },
        )
        .expect("failed to clear unread state");
        let cleared_session = cleared_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing cleared session");
        assert_eq!(cleared_session.unread_count, None);

        let repository = SqliteChatRepository::new(app_handle);
        let persisted_session = repository
            .load_domain_seed()
            .expect("failed to load persisted seed")
            .sessions
            .into_iter()
            .find(|session| session.id == "mika")
            .expect("missing persisted mika session");
        assert_eq!(persisted_session.unread_count, None);
    }

    #[test]
    fn update_message_delivery_status_updates_existing_message() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = update_message_delivery_status(
            app_handle,
            UpdateMessageDeliveryStatusInput {
                session_id: "assistant".into(),
                message_id: "assistant-1".into(),
                delivery_status: MessageDeliveryStatus::Failed,
            },
        )
        .expect("failed to update message delivery status");

        let message = seed
            .message_store
            .get("assistant")
            .and_then(|messages| messages.iter().find(|message| message.id == "assistant-1"))
            .expect("missing updated message");

        assert!(matches!(
            message.delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));
        assert!(matches!(
            message.sync_source,
            Some(MessageSyncSource::Local)
        ));
    }

    #[test]
    fn retry_message_delivery_recomputes_status_from_circle() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "bitchat-circle".into(),
            },
        )
        .expect("failed to start self conversation");
        let failed_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Retry after relay recovers.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");
        let failed_message = failed_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing failed message");
        assert!(matches!(
            failed_message.delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));

        set_circle_status(app_handle, "bitchat-circle", CircleStatus::Open);

        let retried_seed = retry_message_delivery(
            app_handle,
            RetryMessageDeliveryInput {
                session_id: started.session_id.clone(),
                message_id: failed_message.id.clone(),
            },
        )
        .expect("failed to retry message delivery");
        let retried_message = retried_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == failed_message.id)
            })
            .expect("missing retried message");

        assert!(matches!(
            retried_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert!(matches!(
            retried_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        let expected_remote_id = format!("relay-ack:{}", retried_message.id);
        assert_eq!(
            retried_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(retried_message.acked_at.as_deref(), Some("now"));
    }

    #[test]
    fn retry_message_delivery_with_local_auth_preserves_signed_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "bitchat-circle".into(),
            },
        )
        .expect("failed to start self conversation");
        seed_authenticated_shell_runtime(
            app_handle,
            &started.session_id,
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let failed_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Sign once, retry without changing the event id.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");
        let failed_message = failed_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing failed message");

        assert!(matches!(
            failed_message.delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));
        let signed_remote_id = failed_message
            .remote_id
            .clone()
            .expect("signed local message should have remote id");
        assert!(is_lower_hex_64(&signed_remote_id));
        assert_signed_nostr_event(
            failed_message,
            "Sign once, retry without changing the event id.",
        );

        set_circle_status(app_handle, "bitchat-circle", CircleStatus::Open);

        let retried_seed = retry_message_delivery(
            app_handle,
            RetryMessageDeliveryInput {
                session_id: started.session_id.clone(),
                message_id: failed_message.id.clone(),
            },
        )
        .expect("failed to retry message delivery");
        let retried_message = retried_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == failed_message.id)
            })
            .expect("missing retried message");

        assert!(matches!(
            retried_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(
            retried_message.remote_id.as_deref(),
            Some(signed_remote_id.as_str())
        );
        assert_signed_nostr_event(
            retried_message,
            "Sign once, retry without changing the event id.",
        );
        assert_eq!(retried_message.acked_at, None);
    }

    #[test]
    fn retry_message_delivery_clears_transport_outbound_dispatch_record() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "bitchat-circle".into(),
            },
        )
        .expect("failed to start self conversation");
        seed_authenticated_shell_runtime(
            app_handle,
            &started.session_id,
            AuthRuntimeSummary {
                state: AuthRuntimeState::Connected,
                login_method: LoginMethod::ExistingAccount,
                access_kind: LoginAccessKind::HexKey,
                label: "npub1local".into(),
                pubkey: None,
                error: None,
                can_send_messages: true,
                send_blocked_reason: None,
                persisted_in_native_store: false,
                credential_persisted_in_native_store: true,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let failed_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Retry should clear the last queued transport dispatch.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");
        let failed_message = failed_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing failed message");
        let signed_remote_id = failed_message
            .remote_id
            .clone()
            .expect("signed message should have remote id");

        let repository = SqliteTransportRepository::new(app_handle);
        repository
            .save_transport_cache(TransportCache {
                outbound_dispatches: vec![TransportOutboundDispatch {
                    circle_id: "bitchat-circle".into(),
                    session_id: started.session_id.clone(),
                    message_id: failed_message.id.clone(),
                    remote_id: signed_remote_id.clone(),
                    event_id: signed_remote_id,
                    runtime_generation: 1,
                    request_id: "sync:bitchat-circle:1".into(),
                    dispatched_at: "now".into(),
                }],
                ..TransportCache::default()
            })
            .expect("failed to seed transport outbound dispatch");

        set_circle_status(app_handle, "bitchat-circle", CircleStatus::Open);

        retry_message_delivery(
            app_handle,
            RetryMessageDeliveryInput {
                session_id: started.session_id.clone(),
                message_id: failed_message.id.clone(),
            },
        )
        .expect("failed to retry message delivery");

        let cache = transport_cache(app_handle);
        assert!(cache.outbound_dispatches.is_empty());
    }

    #[test]
    fn retry_message_delivery_rejects_when_auth_runtime_is_pending() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let started = start_self_conversation(
            app_handle,
            StartSelfConversationInput {
                circle_id: "bitchat-circle".into(),
            },
        )
        .expect("failed to start self conversation");
        let failed_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: started.session_id.clone(),
                body: "Retry should stay blocked until signer connects.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send message");
        let failed_message = failed_seed
            .message_store
            .get(&started.session_id)
            .and_then(|messages| messages.last())
            .expect("missing failed message");
        assert!(matches!(
            failed_message.delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));

        seed_authenticated_shell_runtime(
            app_handle,
            &started.session_id,
            AuthRuntimeSummary {
                state: AuthRuntimeState::Pending,
                login_method: LoginMethod::Signer,
                access_kind: LoginAccessKind::Bunker,
                label: "bunker://relay".into(),
                pubkey: None,
                error: Some("Remote signer handshake is pending.".into()),
                can_send_messages: false,
                send_blocked_reason: Some("Remote signer handshake is pending.".into()),
                persisted_in_native_store: false,
                credential_persisted_in_native_store: false,
                updated_at: "2026-04-19T09:00:00Z".into(),
            },
        );

        let error = retry_message_delivery(
            app_handle,
            RetryMessageDeliveryInput {
                session_id: started.session_id.clone(),
                message_id: failed_message.id.clone(),
            },
        )
        .expect_err("retry should be blocked by pending auth runtime");

        assert_eq!(
            error,
            "chat_send_blocked:Remote signer handshake is pending."
        );
    }

    fn is_lower_hex_64(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn assert_signed_nostr_event(message: &MessageItem, expected_content: &str) {
        let signed_event = message
            .signed_nostr_event
            .as_ref()
            .expect("local signed message should include signed nostr event");
        assert_eq!(
            signed_event.event_id,
            message.remote_id.clone().unwrap_or_default()
        );
        assert_eq!(signed_event.kind, 1);
        assert_eq!(signed_event.content, expected_content);
        assert!(is_lower_hex_64(&signed_event.pubkey));
        assert_eq!(signed_event.signature.len(), 128);
    }

    fn test_relay_text_message(
        id: &str,
        author: MessageAuthor,
        body: &str,
        time: &str,
        created_at: u64,
    ) -> MessageItem {
        let delivery_status = if matches!(author, MessageAuthor::Me) {
            Some(MessageDeliveryStatus::Sent)
        } else {
            None
        };
        let acked_at = if matches!(author, MessageAuthor::Me) {
            Some("relay-sync".into())
        } else {
            None
        };

        MessageItem {
            id: id.into(),
            kind: MessageKind::Text,
            author,
            body: body.into(),
            time: time.into(),
            meta: None,
            delivery_status,
            remote_id: Some(id.into()),
            sync_source: Some(MessageSyncSource::Relay),
            acked_at,
            signed_nostr_event: Some(crate::domain::chat::SignedNostrEvent {
                event_id: id.into(),
                pubkey: "1".repeat(64),
                created_at,
                kind: 1,
                tags: Vec::new(),
                content: body.into(),
                signature: "2".repeat(128),
            }),
            reply_to: None,
        }
    }

    fn relay_peer_copy_from_local_message(message: &MessageItem, time: &str) -> MessageItem {
        let remote_id = message
            .remote_id
            .clone()
            .expect("local signed message should include remote id");

        MessageItem {
            id: remote_id.clone(),
            kind: message.kind.clone(),
            author: MessageAuthor::Peer,
            body: message.body.clone(),
            time: time.into(),
            meta: message.meta.clone(),
            delivery_status: None,
            remote_id: Some(remote_id),
            sync_source: Some(MessageSyncSource::Relay),
            acked_at: None,
            signed_nostr_event: message.signed_nostr_event.clone(),
            reply_to: None,
        }
    }

    #[test]
    fn merge_remote_messages_updates_session_preview_and_presence() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "remote-mika-1".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: "Remote runtime merged this reply into SQLite.".into(),
                    time: "now".into(),
                    meta: None,
                    delivery_status: None,
                    remote_id: Some("relay:mika:1".into()),
                    sync_source: None,
                    acked_at: None,
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            },
        )
        .expect("failed to merge remote messages");

        assert_eq!(
            seed.sessions.first().map(|session| session.id.as_str()),
            Some("mika")
        );
        let session = seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(
            session.subtitle,
            "Remote runtime merged this reply into SQLite."
        );
        assert_eq!(session.time, "now");
        assert_eq!(session.unread_count, Some(1));

        let contact = seed
            .contacts
            .iter()
            .find(|contact| contact.id == "mika-contact")
            .expect("missing mika contact");
        assert_eq!(contact.online, Some(true));

        let message = seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == "remote-mika-1")
            })
            .expect("missing merged remote message");
        assert!(matches!(
            message.sync_source,
            Some(MessageSyncSource::Relay)
        ));
        assert_eq!(message.remote_id.as_deref(), Some("relay:mika:1"));
    }

    #[test]
    fn merge_remote_messages_hydrates_reply_preview_from_signed_event_e_tag() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let parent_remote_id = "relay-parent-event";

        merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-parent-local".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: "Parent relay message".into(),
                    time: "09:40".into(),
                    meta: None,
                    delivery_status: None,
                    remote_id: Some(parent_remote_id.into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: None,
                    signed_nostr_event: Some(SignedNostrEvent {
                        event_id: parent_remote_id.into(),
                        pubkey: "1".repeat(64),
                        created_at: 1_735_690_000,
                        kind: 1,
                        tags: Vec::new(),
                        content: "Parent relay message".into(),
                        signature: "2".repeat(128),
                    }),
                    reply_to: None,
                }],
            },
        )
        .expect("failed to seed parent relay message");

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-child-local".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: "Reply from relay".into(),
                    time: "09:41".into(),
                    meta: None,
                    delivery_status: None,
                    remote_id: Some("relay-child-event".into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: None,
                    signed_nostr_event: Some(SignedNostrEvent {
                        event_id: "relay-child-event".into(),
                        pubkey: "1".repeat(64),
                        created_at: 1_735_690_100,
                        kind: 1,
                        tags: vec![vec!["e".into(), parent_remote_id.into()]],
                        content: "Reply from relay".into(),
                        signature: "2".repeat(128),
                    }),
                    reply_to: None,
                }],
            },
        )
        .expect("failed to merge relay reply");

        let reply_message = merged_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == "relay-child-local")
            })
            .expect("missing merged relay reply");
        let reply_preview = reply_message
            .reply_to
            .as_ref()
            .expect("merged relay reply should hydrate preview");

        assert_eq!(reply_preview.message_id, "relay-parent-local");
        assert_eq!(reply_preview.remote_id.as_deref(), Some(parent_remote_id));
        assert!(matches!(reply_preview.author, MessageAuthor::Peer));
        assert_eq!(reply_preview.author_label, "Peer");
        assert!(matches!(reply_preview.kind, MessageKind::Text));
        assert_eq!(reply_preview.snippet, "Parent relay message");
    }

    #[test]
    fn merge_remote_messages_deduplicates_by_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Need this thread back on top.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local message");
        let local_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing local message");
        let original_message_count = sent_seed
            .message_store
            .get("mika")
            .map(|messages| messages.len())
            .expect("missing message bucket");

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-copy-mika-1".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Me,
                    body: local_message.body.clone(),
                    time: local_message.time.clone(),
                    meta: None,
                    delivery_status: Some(MessageDeliveryStatus::Sent),
                    remote_id: local_message.remote_id.clone(),
                    sync_source: None,
                    acked_at: Some("remote-ack".into()),
                    signed_nostr_event: local_message.signed_nostr_event.clone(),
                    reply_to: None,
                }],
            },
        )
        .expect("failed to merge remote echo");

        let messages = merged_seed
            .message_store
            .get("mika")
            .expect("missing merged message bucket");
        assert_eq!(messages.len(), original_message_count);
        assert!(messages
            .iter()
            .all(|message| message.id != "relay-copy-mika-1"));
        let merged_message = messages
            .iter()
            .find(|message| message.id == local_message.id)
            .expect("missing preserved local message");
        assert_eq!(merged_message.remote_id, local_message.remote_id);
        assert_eq!(merged_message.acked_at.as_deref(), Some("remote-ack"));
    }

    #[test]
    fn merge_remote_messages_preserves_local_author_on_relay_echo() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Do not flip me into a peer echo.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local message");
        let local_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing local message");

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-copy-mika-peer".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Peer,
                    body: local_message.body.clone(),
                    time: local_message.time.clone(),
                    meta: None,
                    delivery_status: None,
                    remote_id: local_message.remote_id.clone(),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: Some("relay-sync".into()),
                    signed_nostr_event: local_message.signed_nostr_event.clone(),
                    reply_to: None,
                }],
            },
        )
        .expect("failed to merge relay echo");

        let merged_message = merged_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == local_message.id)
            })
            .expect("missing preserved local message");
        assert!(matches!(merged_message.author, MessageAuthor::Me));
        assert!(matches!(
            merged_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
        assert_eq!(merged_message.acked_at.as_deref(), Some("relay-sync"));
    }

    #[test]
    fn merge_remote_messages_updates_session_preview_for_self_authored_relay_message() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![MessageItem {
                    id: "relay-self-mika-1".into(),
                    kind: MessageKind::Text,
                    author: MessageAuthor::Me,
                    body: "Sent from another device".into(),
                    time: "09:41".into(),
                    meta: None,
                    delivery_status: Some(MessageDeliveryStatus::Sent),
                    remote_id: Some("relay-self-mika-1".into()),
                    sync_source: Some(MessageSyncSource::Relay),
                    acked_at: Some("relay-sync".into()),
                    signed_nostr_event: None,
                    reply_to: None,
                }],
            },
        )
        .expect("failed to merge self-authored relay message");

        let session = merged_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Sent from another device");
        assert_eq!(session.time, "09:41");
        assert_eq!(session.unread_count, None);
        let merged_message = merged_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == "relay-self-mika-1")
            })
            .expect("missing merged self-authored relay message");
        assert!(matches!(merged_message.author, MessageAuthor::Me));
        assert!(matches!(
            merged_message.sync_source,
            Some(MessageSyncSource::Relay)
        ));
    }

    #[test]
    fn merge_remote_messages_keeps_new_peer_preview_when_batch_ends_with_duplicate_echo() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![test_relay_text_message(
                    "relay-peer-old",
                    MessageAuthor::Peer,
                    "Older relay message",
                    "09:10",
                    1_735_689_000,
                )],
            },
        )
        .expect("failed to seed known peer relay message");

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![
                    test_relay_text_message(
                        "relay-peer-fresh",
                        MessageAuthor::Peer,
                        "Fresh relay message",
                        "09:45",
                        1_735_691_000,
                    ),
                    test_relay_text_message(
                        "relay-peer-old",
                        MessageAuthor::Peer,
                        "Older relay message",
                        "09:10",
                        1_735_689_000,
                    ),
                ],
            },
        )
        .expect("failed to merge peer relay batch");

        let session = merged_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Fresh relay message");
        assert_eq!(session.time, "09:45");
        assert_eq!(session.unread_count, Some(2));
    }

    #[test]
    fn merge_remote_messages_prefers_latest_relay_created_at_for_session_preview() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![
                    test_relay_text_message(
                        "relay-peer-newest",
                        MessageAuthor::Peer,
                        "Newest relay event",
                        "09:45",
                        1_735_691_000,
                    ),
                    test_relay_text_message(
                        "relay-peer-earlier",
                        MessageAuthor::Peer,
                        "Earlier relay event",
                        "09:40",
                        1_735_690_000,
                    ),
                ],
            },
        )
        .expect("failed to merge out-of-order relay batch");

        let session = merged_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Newest relay event");
        assert_eq!(session.time, "09:45");
        assert_eq!(session.unread_count, Some(2));
    }

    #[test]
    fn merge_remote_messages_keeps_new_self_authored_relay_preview_when_batch_ends_with_duplicate_echo(
    ) {
        let guard = test_app();
        let app_handle = guard.app.handle();

        merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![test_relay_text_message(
                    "relay-self-old",
                    MessageAuthor::Me,
                    "Earlier device message",
                    "09:30",
                    1_735_690_000,
                )],
            },
        )
        .expect("failed to seed known self-authored relay message");

        let merged_seed = merge_remote_messages(
            app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![
                    test_relay_text_message(
                        "relay-self-new",
                        MessageAuthor::Me,
                        "Latest device message",
                        "09:50",
                        1_735_692_000,
                    ),
                    test_relay_text_message(
                        "relay-self-old",
                        MessageAuthor::Me,
                        "Earlier device message",
                        "09:30",
                        1_735_690_000,
                    ),
                ],
            },
        )
        .expect("failed to merge self-authored relay batch");

        let session = merged_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing updated session");
        assert_eq!(session.subtitle, "Latest device message");
        assert_eq!(session.time, "09:50");
        assert_eq!(session.unread_count, None);
    }

    #[test]
    fn merge_remote_delivery_receipts_updates_local_message_by_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Receipt merge should update this message.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local message");
        let local_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing local sent message");

        let merged_seed = merge_remote_delivery_receipts(
            app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "mika".into(),
                receipts: vec![crate::domain::chat::RemoteDeliveryReceipt {
                    remote_id: local_message
                        .remote_id
                        .clone()
                        .expect("missing remote id for local message"),
                    message_id: None,
                    delivery_status: MessageDeliveryStatus::Failed,
                    acked_at: Some("relay-failure".into()),
                }],
            },
        )
        .expect("failed to merge remote delivery receipt");

        let merged_message = merged_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == local_message.id)
            })
            .expect("missing merged message");
        assert!(matches!(
            merged_message.delivery_status,
            Some(MessageDeliveryStatus::Failed)
        ));
        assert_eq!(merged_message.acked_at.as_deref(), Some("relay-failure"));
        assert!(matches!(
            merged_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
    }

    #[test]
    fn merge_remote_delivery_receipts_is_idempotent_for_duplicate_receipts() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Duplicate receipts should be idempotent.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local message");
        let local_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing local sent message");
        let remote_id = local_message
            .remote_id
            .clone()
            .expect("missing remote id for local message");

        let duplicate_receipt = crate::domain::chat::RemoteDeliveryReceipt {
            remote_id: remote_id.clone(),
            message_id: None,
            delivery_status: MessageDeliveryStatus::Sent,
            acked_at: Some("relay-ok".into()),
        };

        let merged_once_seed = merge_remote_delivery_receipts(
            app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "mika".into(),
                receipts: vec![duplicate_receipt.clone(), duplicate_receipt.clone()],
            },
        )
        .expect("failed to merge duplicated receipt batch");
        let merged_once_bucket = merged_once_seed
            .message_store
            .get("mika")
            .expect("missing merged message bucket after first merge");
        let merged_once_message = merged_once_bucket
            .iter()
            .find(|message| message.id == local_message.id)
            .expect("missing merged message after first merge");

        assert!(matches!(
            merged_once_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(merged_once_message.acked_at.as_deref(), Some("relay-ok"));

        let merged_twice_seed = merge_remote_delivery_receipts(
            app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "mika".into(),
                receipts: vec![duplicate_receipt],
            },
        )
        .expect("failed to merge duplicated receipt again");
        let merged_twice_bucket = merged_twice_seed
            .message_store
            .get("mika")
            .expect("missing merged message bucket after second merge");
        let merged_twice_message = merged_twice_bucket
            .iter()
            .find(|message| message.id == local_message.id)
            .expect("missing merged message after second merge");

        assert_eq!(merged_twice_bucket.len(), merged_once_bucket.len());
        assert!(matches!(
            merged_twice_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(merged_twice_message.acked_at.as_deref(), Some("relay-ok"));
        assert_eq!(
            merged_twice_message.remote_id.as_deref(),
            Some(remote_id.as_str())
        );
        assert!(matches!(
            merged_twice_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
    }

    #[test]
    fn two_local_accounts_can_exchange_direct_messages_and_reconcile_receipts() {
        const SENDER_SECRET_KEY_HEX: &str =
            "1111111111111111111111111111111111111111111111111111111111111111";
        const RECEIVER_SECRET_KEY_HEX: &str =
            "6666666666666666666666666666666666666666666666666666666666666666";

        let sender_guard = test_app();
        let sender_app_handle = sender_guard.app.handle();
        let sender_config_root = sender_guard.config_root.clone();
        let receiver_guard = detached_test_app("p2p-chat-mutations-receiver-test");
        let receiver_app_handle = receiver_guard.app.handle();
        let receiver_config_root = receiver_guard.config_root.clone();
        let sender_pubkey = valid_test_pubkey_hex(SENDER_SECRET_KEY_HEX);
        let receiver_pubkey = valid_test_pubkey_hex(RECEIVER_SECRET_KEY_HEX);

        activate_test_config_root(&sender_config_root);
        set_contact_pubkey(sender_app_handle, "mika-contact", &receiver_pubkey);
        seed_authenticated_shell_runtime_with_secret(
            sender_app_handle,
            "mika",
            SENDER_SECRET_KEY_HEX,
            connected_local_auth_runtime("2026-04-23T10:00:00Z"),
        );

        let sent_seed = send_message(
            sender_app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Hello from sender account.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("sender should create an outbound direct message");
        let sent_message = sent_seed
            .message_store
            .get("mika")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing sender outbound message");
        let sent_remote_id = sent_message
            .remote_id
            .clone()
            .expect("sender outbound message should include remote id");
        let sent_signed_event = sent_message
            .signed_nostr_event
            .as_ref()
            .expect("sender outbound message should include signed event");

        assert!(matches!(
            sent_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(sent_signed_event.pubkey, sender_pubkey);
        assert_eq!(
            sent_signed_event.tags,
            vec![vec!["p".into(), receiver_pubkey.clone()]]
        );

        activate_test_config_root(&receiver_config_root);
        set_contact_pubkey(receiver_app_handle, "alice-contact", &sender_pubkey);
        seed_authenticated_shell_runtime_with_secret(
            receiver_app_handle,
            "alice",
            RECEIVER_SECRET_KEY_HEX,
            connected_local_auth_runtime("2026-04-23T10:01:00Z"),
        );
        let receiver_initial_unread_count = load_domain_seed(receiver_app_handle)
            .expect("failed to load receiver domain seed")
            .sessions
            .iter()
            .find(|session| session.id == "alice")
            .and_then(|session| session.unread_count)
            .unwrap_or_default();

        let received_seed = merge_remote_messages(
            receiver_app_handle,
            MergeRemoteMessagesInput {
                session_id: "alice".into(),
                messages: vec![relay_peer_copy_from_local_message(&sent_message, "10:01")],
            },
        )
        .expect("receiver should merge the sender relay message");
        let receiver_session = received_seed
            .sessions
            .iter()
            .find(|session| session.id == "alice")
            .expect("missing receiver session");
        let received_message = received_seed
            .message_store
            .get("alice")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.remote_id == Some(sent_remote_id.clone()))
            })
            .cloned()
            .expect("missing receiver inbound message");

        assert_eq!(receiver_session.subtitle, "Hello from sender account.");
        assert_eq!(
            receiver_session.unread_count.unwrap_or_default(),
            receiver_initial_unread_count + 1
        );
        assert!(matches!(received_message.author, MessageAuthor::Peer));
        assert!(matches!(
            received_message.sync_source,
            Some(MessageSyncSource::Relay)
        ));
        assert_eq!(
            received_message.remote_id.as_deref(),
            Some(sent_remote_id.as_str())
        );

        activate_test_config_root(&sender_config_root);
        let sender_receipt_seed = merge_remote_delivery_receipts(
            sender_app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "mika".into(),
                receipts: vec![crate::domain::chat::RemoteDeliveryReceipt {
                    remote_id: sent_remote_id.clone(),
                    message_id: None,
                    delivery_status: MessageDeliveryStatus::Sent,
                    acked_at: Some("relay-ok".into()),
                }],
            },
        )
        .expect("sender should merge a delivery receipt for the outbound message");
        let acked_sender_message = sender_receipt_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == sent_message.id)
            })
            .expect("missing sender message after receipt merge");

        assert!(matches!(
            acked_sender_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(acked_sender_message.acked_at.as_deref(), Some("relay-ok"));

        activate_test_config_root(&receiver_config_root);
        let reply_seed = send_message(
            receiver_app_handle,
            SendMessageInput {
                session_id: "alice".into(),
                body: "Reply from receiver account.".into(),
                reply_to_message_id: Some(received_message.id.clone()),
            },
        )
        .expect("receiver should send a reply back to the sender");
        let receiver_reply = reply_seed
            .message_store
            .get("alice")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing receiver reply message");
        let receiver_reply_remote_id = receiver_reply
            .remote_id
            .clone()
            .expect("receiver reply should include remote id");
        let receiver_reply_preview = receiver_reply
            .reply_to
            .as_ref()
            .expect("receiver reply should carry a reply preview");
        let receiver_reply_signed_event = receiver_reply
            .signed_nostr_event
            .as_ref()
            .expect("receiver reply should be signed");

        assert!(matches!(
            receiver_reply.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(
            receiver_reply_preview.remote_id.as_deref(),
            Some(sent_remote_id.as_str())
        );
        assert!(matches!(receiver_reply_preview.author, MessageAuthor::Peer));
        assert_eq!(receiver_reply_preview.snippet, "Hello from sender account.");
        assert_eq!(
            receiver_reply_signed_event.tags,
            vec![
                vec!["p".into(), sender_pubkey.clone()],
                vec!["e".into(), sent_remote_id.clone()],
            ]
        );

        activate_test_config_root(&sender_config_root);
        let sender_initial_unread_count = load_domain_seed(sender_app_handle)
            .expect("failed to load sender domain seed")
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .and_then(|session| session.unread_count)
            .unwrap_or_default();
        let sender_received_reply_seed = merge_remote_messages(
            sender_app_handle,
            MergeRemoteMessagesInput {
                session_id: "mika".into(),
                messages: vec![relay_peer_copy_from_local_message(&receiver_reply, "10:02")],
            },
        )
        .expect("sender should merge the receiver reply from relay");
        let sender_session_after_reply = sender_received_reply_seed
            .sessions
            .iter()
            .find(|session| session.id == "mika")
            .expect("missing sender session after reply merge");
        let sender_received_reply = sender_received_reply_seed
            .message_store
            .get("mika")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.remote_id == Some(receiver_reply_remote_id.clone()))
            })
            .expect("missing sender inbound reply");
        let sender_received_reply_preview = sender_received_reply
            .reply_to
            .as_ref()
            .expect("sender inbound reply should hydrate the reply preview");

        assert_eq!(
            sender_session_after_reply.subtitle,
            "Reply from receiver account."
        );
        assert_eq!(
            sender_session_after_reply.unread_count.unwrap_or_default(),
            sender_initial_unread_count + 1
        );
        assert!(matches!(sender_received_reply.author, MessageAuthor::Peer));
        assert_eq!(
            sender_received_reply.remote_id.as_deref(),
            Some(receiver_reply_remote_id.as_str())
        );
        assert_eq!(
            sender_received_reply_preview.remote_id.as_deref(),
            Some(sent_remote_id.as_str())
        );
        assert!(matches!(
            sender_received_reply_preview.author,
            MessageAuthor::Me
        ));
        assert_eq!(sender_received_reply_preview.author_label, "You");
        assert_eq!(
            sender_received_reply_preview.snippet,
            "Hello from sender account."
        );

        activate_test_config_root(&receiver_config_root);
        let receiver_receipt_seed = merge_remote_delivery_receipts(
            receiver_app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "alice".into(),
                receipts: vec![crate::domain::chat::RemoteDeliveryReceipt {
                    remote_id: receiver_reply_remote_id,
                    message_id: None,
                    delivery_status: MessageDeliveryStatus::Sent,
                    acked_at: Some("relay-ok-reply".into()),
                }],
            },
        )
        .expect("receiver should merge the delivery receipt for the reply");
        let acked_receiver_reply = receiver_receipt_seed
            .message_store
            .get("alice")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == receiver_reply.id)
            })
            .expect("missing receiver reply after receipt merge");

        assert!(matches!(
            acked_receiver_reply.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(
            acked_receiver_reply.acked_at.as_deref(),
            Some("relay-ok-reply")
        );
    }

    #[test]
    fn merge_remote_delivery_receipts_ignores_unknown_remote_ids() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let before_seed = load_domain_seed(app_handle).expect("failed to load initial domain");
        let original_bucket = before_seed
            .message_store
            .get("mika")
            .cloned()
            .expect("missing initial message bucket");

        let merged_seed = merge_remote_delivery_receipts(
            app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "mika".into(),
                receipts: vec![crate::domain::chat::RemoteDeliveryReceipt {
                    remote_id: "relay:missing".into(),
                    message_id: None,
                    delivery_status: MessageDeliveryStatus::Sent,
                    acked_at: Some("ignored".into()),
                }],
            },
        )
        .expect("failed to merge unknown receipt");

        let merged_bucket = merged_seed
            .message_store
            .get("mika")
            .expect("missing merged message bucket");
        assert_eq!(merged_bucket.len(), original_bucket.len());
        for (before, after) in original_bucket.iter().zip(merged_bucket.iter()) {
            assert_eq!(before.id, after.id);
            assert_eq!(
                before.delivery_status.as_ref().map(|status| match status {
                    MessageDeliveryStatus::Sending => "sending",
                    MessageDeliveryStatus::Sent => "sent",
                    MessageDeliveryStatus::Failed => "failed",
                }),
                after.delivery_status.as_ref().map(|status| match status {
                    MessageDeliveryStatus::Sending => "sending",
                    MessageDeliveryStatus::Sent => "sent",
                    MessageDeliveryStatus::Failed => "failed",
                })
            );
            assert_eq!(before.remote_id, after.remote_id);
            assert_eq!(before.acked_at, after.acked_at);
        }
    }

    #[test]
    fn merge_remote_delivery_receipts_falls_back_to_message_id_when_remote_id_is_missing() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "nora".into(),
                body: "Receipt fallback should match this local message.".into(),
                reply_to_message_id: None,
            },
        )
        .expect("failed to send local message");
        let local_message = sent_seed
            .message_store
            .get("nora")
            .and_then(|messages| messages.last())
            .cloned()
            .expect("missing local sent message");

        assert!(matches!(
            local_message.delivery_status,
            Some(MessageDeliveryStatus::Sending)
        ));
        assert_eq!(local_message.remote_id, None);
        assert_eq!(local_message.acked_at, None);
        let expected_remote_id = format!("relay-ack:{}", local_message.id);

        let merged_seed = merge_remote_delivery_receipts(
            app_handle,
            MergeRemoteDeliveryReceiptsInput {
                session_id: "nora".into(),
                receipts: vec![crate::domain::chat::RemoteDeliveryReceipt {
                    remote_id: expected_remote_id.clone(),
                    message_id: Some(local_message.id.clone()),
                    delivery_status: MessageDeliveryStatus::Sent,
                    acked_at: Some("relay-acknowledged".into()),
                }],
            },
        )
        .expect("failed to merge fallback delivery receipt");

        let merged_message = merged_seed
            .message_store
            .get("nora")
            .and_then(|messages| {
                messages
                    .iter()
                    .find(|message| message.id == local_message.id)
            })
            .expect("missing merged message");
        assert!(matches!(
            merged_message.delivery_status,
            Some(MessageDeliveryStatus::Sent)
        ));
        assert_eq!(
            merged_message.remote_id.as_deref(),
            Some(expected_remote_id.as_str())
        );
        assert_eq!(
            merged_message.acked_at.as_deref(),
            Some("relay-acknowledged")
        );
        assert!(matches!(
            merged_message.sync_source,
            Some(MessageSyncSource::Local)
        ));
    }

    #[test]
    fn create_group_conversation_adds_group_profile_and_message_bucket() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = create_group_conversation(
            app_handle,
            CreateGroupConversationInput {
                circle_id: "main-circle".into(),
                name: "Launch Crew".into(),
                member_contact_ids: vec!["alice-contact".into(), "mika-contact".into()],
            },
        )
        .expect("failed to create group conversation");

        let session = result
            .seed
            .sessions
            .iter()
            .find(|session| session.id == result.session_id)
            .expect("missing group session");
        assert!(matches!(session.kind, SessionKind::Group));
        assert_eq!(session.name, "Launch Crew");
        assert_eq!(session.members, Some(3));
        assert!(result
            .seed
            .groups
            .iter()
            .any(|group| group.session_id == result.session_id));
        assert!(result.seed.message_store.contains_key(&result.session_id));
    }

    #[test]
    fn update_group_name_updates_session_and_group_name() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let created = create_group_conversation(
            app_handle,
            CreateGroupConversationInput {
                circle_id: "main-circle".into(),
                name: "Launch Crew".into(),
                member_contact_ids: vec!["alice-contact".into(), "mika-contact".into()],
            },
        )
        .expect("failed to create group conversation");

        let updated = update_group_name(
            app_handle,
            UpdateGroupNameInput {
                session_id: created.session_id.clone(),
                name: "Mission Control".into(),
            },
        )
        .expect("failed to update group name");

        let session = updated
            .sessions
            .iter()
            .find(|session| session.id == created.session_id)
            .expect("missing updated session");
        let group = updated
            .groups
            .iter()
            .find(|group| group.session_id == created.session_id)
            .expect("missing updated group");
        assert_eq!(session.name, "Mission Control");
        assert_eq!(session.initials, "MC");
        assert_eq!(group.name, "Mission Control");
        assert_eq!(
            group.description,
            "Group created from the new message flow in Mission Control."
        );
    }

    #[test]
    fn update_group_members_recomputes_member_count_and_preserves_admin() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let created = create_group_conversation(
            app_handle,
            CreateGroupConversationInput {
                circle_id: "main-circle".into(),
                name: "Launch Crew".into(),
                member_contact_ids: vec!["alice-contact".into(), "mika-contact".into()],
            },
        )
        .expect("failed to create group conversation");

        let updated = update_group_members(
            app_handle,
            UpdateGroupMembersInput {
                session_id: created.session_id.clone(),
                member_contact_ids: vec![
                    "alice-contact".into(),
                    "mika-contact".into(),
                    "nora-contact".into(),
                ],
            },
        )
        .expect("failed to update group members");

        let session = updated
            .sessions
            .iter()
            .find(|session| session.id == created.session_id)
            .expect("missing updated session");
        let group = updated
            .groups
            .iter()
            .find(|group| group.session_id == created.session_id)
            .expect("missing updated group");

        assert_eq!(session.members, Some(4));
        assert_eq!(group.members.len(), 3);
        assert!(group
            .members
            .iter()
            .any(|member| member.contact_id == "alice-contact"
                && matches!(member.role, Some(GroupRole::Admin))));
        assert!(group
            .members
            .iter()
            .any(|member| member.contact_id == "nora-contact"));
    }

    #[test]
    fn update_contact_remark_updates_contact_subtitle() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let updated = update_contact_remark(
            app_handle,
            UpdateContactRemarkInput {
                contact_id: "alice-contact".into(),
                remark: "Design review partner".into(),
            },
        )
        .expect("failed to update contact remark");

        let contact = updated
            .contacts
            .iter()
            .find(|contact| contact.id == "alice-contact")
            .expect("missing updated contact");
        assert_eq!(contact.subtitle, "Design review partner");
    }

    #[test]
    fn add_circle_places_new_circle_first() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = add_circle(
            app_handle,
            AddCircleInput {
                mode: CircleCreateMode::Custom,
                name: "Ops Circle".into(),
                relay: Some("wss://ops.example.local".into()),
                invite_code: None,
            },
        )
        .expect("failed to add circle");

        assert_eq!(
            result.seed.circles.first().map(|circle| circle.id.as_str()),
            Some(result.circle_id.as_str())
        );
    }

    #[test]
    fn normalize_custom_circle_relay_resolves_public_shortcuts() {
        assert_eq!(
            normalize_custom_circle_relay("damus").as_deref(),
            Some("wss://relay.damus.io")
        );
        assert_eq!(
            normalize_custom_circle_relay(" nos ").as_deref(),
            Some("wss://nos.lol")
        );
        assert_eq!(
            normalize_custom_circle_relay("PRIMAL").as_deref(),
            Some("wss://relay.primal.net")
        );
        assert_eq!(
            normalize_custom_circle_relay("0xchat").as_deref(),
            Some("wss://relay.0xchat.com")
        );
    }

    #[test]
    fn restore_circle_preserves_original_relay_type_and_description() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = restore_circle(
            app_handle,
            RestoreCircleInput {
                name: "Archive Relay".into(),
                relay: "wss://archive.example.local".into(),
                circle_type: CircleType::Paid,
                description: "Recovered from the local restore catalog.".into(),
            },
        )
        .expect("failed to restore circle");

        let restored_circle = result
            .seed
            .circles
            .iter()
            .find(|circle| circle.id == result.circle_id)
            .expect("missing restored circle");
        assert_eq!(restored_circle.name, "Archive Relay");
        assert_eq!(restored_circle.relay, "wss://archive.example.local");
        assert!(matches!(restored_circle.circle_type, CircleType::Paid));
        assert_eq!(
            restored_circle.description,
            "Recovered from the local restore catalog."
        );
    }

    #[test]
    fn restore_circle_returns_existing_circle_when_relay_already_exists() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = restore_circle(
            app_handle,
            RestoreCircleInput {
                name: "Should Not Duplicate".into(),
                relay: "relay.p2p-chat.local".into(),
                circle_type: CircleType::Custom,
                description: "Duplicate restore entry".into(),
            },
        )
        .expect("failed to restore existing circle");

        assert_eq!(result.circle_id, "main-circle");
        let matching_circles = result
            .seed
            .circles
            .iter()
            .filter(|circle| circle.relay == "relay.p2p-chat.local")
            .count();
        assert_eq!(matching_circles, 1);
    }

    #[test]
    fn start_lookup_conversation_creates_contact_from_handle_lookup() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let result = start_lookup_conversation(
            app_handle,
            StartLookupConversationInput {
                circle_id: "main-circle".into(),
                query: "@relaypilot".into(),
            },
        )
        .expect("failed to start lookup conversation");

        let session = result
            .seed
            .sessions
            .iter()
            .find(|session| session.id == result.session_id)
            .expect("missing lookup session");
        let contact_id = session
            .contact_id
            .clone()
            .expect("lookup session missing contact");
        let contact = result
            .seed
            .contacts
            .iter()
            .find(|contact| contact.id == contact_id)
            .expect("lookup contact missing");

        assert_eq!(contact.handle, "@relaypilot");
        assert_eq!(contact.subtitle, "Imported from lookup");
        assert!(matches!(session.kind, SessionKind::Direct));
    }

    #[test]
    fn start_lookup_conversation_reuses_existing_contact_for_0x_prefixed_hex_pubkey() {
        let guard = test_app();
        let app_handle = guard.app.handle();
        let pubkey_hex = valid_test_pubkey_hex(
            "1111111111111111111111111111111111111111111111111111111111111111",
        );
        set_contact_pubkey(app_handle, "alice-contact", &pubkey_hex);

        let result = start_lookup_conversation(
            app_handle,
            StartLookupConversationInput {
                circle_id: "main-circle".into(),
                query: format!("0x{pubkey_hex}"),
            },
        )
        .expect("failed to start lookup conversation from 0x-prefixed pubkey");

        let session = result
            .seed
            .sessions
            .iter()
            .find(|session| session.id == result.session_id)
            .expect("missing lookup session");

        assert_eq!(session.contact_id.as_deref(), Some("alice-contact"));
        assert_eq!(
            result
                .seed
                .contacts
                .iter()
                .filter(|contact| contact.id == "alice-contact")
                .count(),
            1
        );
        assert!(!result
            .seed
            .contacts
            .iter()
            .any(|contact| contact.id.starts_with("lookup-")));
    }
}
