use crate::domain::chat::{
    AddCircleInput, AddCircleResult, ChatDomainSeed, ChatSessionAction, CircleCreateMode,
    CircleItem, CircleStatus, CircleType, ContactItem, CreateGroupConversationInput, GroupMember,
    GroupProfile, GroupRole, MergeRemoteDeliveryReceiptsInput, MergeRemoteMessagesInput,
    MessageAuthor, MessageDeliveryStatus, MessageItem, MessageKind, MessageSyncSource,
    RetryMessageDeliveryInput, SendMessageInput, SessionActionInput, SessionItem, SessionKind,
    StartConversationInput, StartConversationResult, StartLookupConversationInput,
    StartSelfConversationInput, UpdateCircleInput, UpdateGroupMembersInput, UpdateGroupNameInput,
    UpdateMessageDeliveryStatusInput, UpdateSessionDraftInput,
};
use crate::domain::chat_repository::{
    build_remote_delivery_receipt_change_set, build_remote_message_merge_change_set,
    ChatDomainChangeSet, ChatRepository, ChatUpsert,
};
use crate::infra::sqlite_chat_repository::SqliteChatRepository;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn send_message(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: SendMessageInput,
) -> Result<ChatDomainSeed, String> {
    let content = input.body.trim();
    if content.is_empty() {
        return Err("message body is empty".into());
    }

    let seed = load_domain_seed(app_handle)?;
    let mut updated_session = seed
        .sessions
        .iter()
        .find(|session| session.id == input.session_id)
        .cloned()
        .ok_or_else(|| format!("session not found: {}", input.session_id))?;

    updated_session.subtitle = content.to_string();
    updated_session.time = "now".into();
    updated_session.draft = None;
    let delivery_status = delivery_status_for_circle(&seed, &updated_session.circle_id)?;

    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            sessions_upsert: vec![ChatUpsert {
                item: updated_session,
                move_to_top: true,
            }],
            messages_append: vec![(
                input.session_id,
                build_local_message(
                    unique_local_id("message"),
                    MessageKind::Text,
                    content.to_string(),
                    "now".into(),
                    Some(delivery_status),
                ),
            )],
            ..Default::default()
        },
    )
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
                with_delivery_status(message, input.delivery_status),
            )],
            ..Default::default()
        },
    )
}

pub fn retry_message_delivery(
    app_handle: &tauri::AppHandle<impl tauri::Runtime>,
    input: RetryMessageDeliveryInput,
) -> Result<ChatDomainSeed, String> {
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
    apply_domain_change_set(
        app_handle,
        ChatDomainChangeSet {
            messages_upsert: vec![(
                input.session_id,
                with_delivery_status(message, delivery_status),
            )],
            ..Default::default()
        },
    )
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
                    description: match input.mode {
                        CircleCreateMode::Private => {
                            "Private relay shell created from the onboarding flow.".into()
                        }
                        CircleCreateMode::Custom => {
                            "Custom relay connected from a manually entered endpoint.".into()
                        }
                        CircleCreateMode::Invite => "Circle imported from an invite handoff and waiting for relay confirmation."
                            .into(),
                    },
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

fn build_local_message(
    id: String,
    kind: MessageKind,
    body: String,
    time: String,
    delivery_status: Option<MessageDeliveryStatus>,
) -> MessageItem {
    let mut message = MessageItem {
        id,
        kind,
        author: MessageAuthor::Me,
        body,
        time,
        meta: None,
        delivery_status,
        remote_id: None,
        sync_source: Some(MessageSyncSource::Local),
        acked_at: None,
    };
    hydrate_local_delivery_tracking(&mut message);
    message
}

fn with_delivery_status(
    mut message: MessageItem,
    delivery_status: MessageDeliveryStatus,
) -> MessageItem {
    message.delivery_status = Some(delivery_status);
    hydrate_local_delivery_tracking(&mut message);
    message
}

fn hydrate_local_delivery_tracking(message: &mut MessageItem) {
    if !matches!(message.author, MessageAuthor::Me) {
        return;
    }

    if message.sync_source.is_none() {
        message.sync_source = Some(MessageSyncSource::Local);
    }

    if matches!(message.delivery_status, Some(MessageDeliveryStatus::Sent)) {
        if message.remote_id.is_none() {
            message.remote_id = Some(acked_remote_message_id(&message.id));
        }
        if message.acked_at.is_none() {
            message.acked_at = Some(message.time.clone());
        }
    }
}

fn acked_remote_message_id(message_id: &str) -> String {
    format!("relay-ack:{message_id}")
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
    let normalized = query.trim().to_lowercase();

    seed.contacts.iter().find_map(|contact| {
        let name_matches = contact.name.trim().to_lowercase() == normalized;
        let id_matches = contact.id.trim().eq_ignore_ascii_case(query.trim());
        let handle_matches = contact.handle.trim().eq_ignore_ascii_case(query.trim());
        let pubkey_matches = contact.pubkey.trim().eq_ignore_ascii_case(query.trim());

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
    let slug = build_circle_slug(trimmed);
    let handle = if trimmed.starts_with('@') {
        trimmed.to_lowercase()
    } else {
        format!("@{slug}")
    };
    let name = if trimmed.starts_with('@') {
        humanize_identifier(trimmed.trim_start_matches('@'))
    } else if lowered.starts_with("npub") || trimmed.len() >= 32 {
        format!(
            "Remote {}",
            trimmed.chars().take(6).collect::<String>().to_uppercase()
        )
    } else if trimmed.contains("://") {
        format!(
            "Invite {}",
            humanize_identifier(trimmed.rsplit("://").next().unwrap_or("link"))
        )
    } else {
        humanize_identifier(trimmed)
    };
    let pubkey = if lowered.starts_with("npub") || trimmed.len() >= 32 {
        trimmed.to_string()
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
    repository.load_domain_seed()
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
        CircleCreateMode::Custom => input.relay.clone().unwrap_or_default().trim().to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::chat_repository::ChatRepository;
    use crate::infra::sqlite_chat_repository::SqliteChatRepository;
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
    fn merge_remote_messages_deduplicates_by_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Need this thread back on top.".into(),
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
    fn merge_remote_delivery_receipts_updates_local_message_by_remote_id() {
        let guard = test_app();
        let app_handle = guard.app.handle();

        let sent_seed = send_message(
            app_handle,
            SendMessageInput {
                session_id: "mika".into(),
                body: "Receipt merge should update this message.".into(),
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
}
