use crate::domain::chat::{
    ChatDomainOverview, ChatDomainSeed, ChatSessionMessageUpdates, ChatSessionMessagesPage,
    CircleItem, CircleStatus, CircleType, ContactItem, GroupMember, GroupProfile, GroupRole,
    LoadSessionMessageUpdatesInput, LoadSessionMessagesInput, MessageAuthor, MessageDeliveryStatus,
    MessageItem, MessageKind, MessageSyncSource, SessionItem, SessionKind,
};
use crate::domain::chat_repository::{
    merge_message_records, ChatDomainChangeSet, ChatRepository, ChatUpsert,
};
use crate::infra::seed_chat_repository::SeedChatRepository;
use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;
use tauri::Runtime;

pub struct SqliteChatRepository<R: Runtime> {
    app_handle: tauri::AppHandle<R>,
}

impl<R: Runtime> SqliteChatRepository<R> {
    pub fn new(app_handle: &tauri::AppHandle<R>) -> Self {
        Self {
            app_handle: app_handle.clone(),
        }
    }

    fn with_connection<T>(
        &self,
        handler: impl FnOnce(&rusqlite::Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut conn = open_connection(&self.app_handle)?;
        self.ensure_schema(&conn)?;
        self.ensure_seed_data(&mut conn)?;
        handler(&conn)
    }

    fn with_connection_mut<T>(
        &self,
        handler: impl FnOnce(&mut rusqlite::Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut conn = open_connection(&self.app_handle)?;
        self.ensure_schema(&conn)?;
        self.ensure_seed_data(&mut conn)?;
        handler(&mut conn)
    }

    fn ensure_schema(&self, conn: &rusqlite::Connection) -> Result<(), String> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS circles (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              relay TEXT NOT NULL,
              type TEXT NOT NULL,
              status TEXT NOT NULL,
              latency TEXT NOT NULL,
              description TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS contacts (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              initials TEXT NOT NULL,
              handle TEXT NOT NULL,
              pubkey TEXT NOT NULL,
              subtitle TEXT NOT NULL,
              bio TEXT NOT NULL,
              online INTEGER,
              blocked INTEGER
            );

            CREATE TABLE IF NOT EXISTS sessions (
              id TEXT PRIMARY KEY,
              circle_id TEXT NOT NULL,
              contact_id TEXT,
              name TEXT NOT NULL,
              initials TEXT NOT NULL,
              subtitle TEXT NOT NULL,
              time TEXT NOT NULL,
              unread_count INTEGER,
              muted INTEGER,
              pinned INTEGER,
              draft TEXT,
              kind TEXT NOT NULL,
              category TEXT NOT NULL,
              members INTEGER,
              archived INTEGER
            );

            CREATE TABLE IF NOT EXISTS group_profiles (
              session_id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              description TEXT NOT NULL,
              muted INTEGER
            );

            CREATE TABLE IF NOT EXISTS group_members (
              session_id TEXT NOT NULL,
              contact_id TEXT NOT NULL,
              role TEXT,
              PRIMARY KEY (session_id, contact_id)
            );

            CREATE TABLE IF NOT EXISTS messages (
              id TEXT PRIMARY KEY,
              session_id TEXT NOT NULL,
              kind TEXT NOT NULL,
              author TEXT NOT NULL,
              body TEXT NOT NULL,
              time TEXT NOT NULL,
              meta TEXT,
              delivery_status TEXT,
              remote_id TEXT,
              sync_source TEXT,
              acked_at TEXT
            );

            CREATE TABLE IF NOT EXISTS chat_meta (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              initialized INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())?;

        ensure_message_delivery_status_column(conn)?;
        ensure_message_remote_id_column(conn)?;
        ensure_message_sync_source_column(conn)?;
        ensure_message_acked_at_column(conn)
    }

    fn ensure_seed_data(&self, conn: &mut rusqlite::Connection) -> Result<(), String> {
        let is_initialized = conn
            .query_row(
                "SELECT initialized FROM chat_meta WHERE id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        if is_initialized.unwrap_or_default() != 0 {
            return Ok(());
        }

        let existing_row_count: i64 = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM circles) +
                    (SELECT COUNT(*) FROM contacts) +
                    (SELECT COUNT(*) FROM sessions) +
                    (SELECT COUNT(*) FROM group_profiles) +
                    (SELECT COUNT(*) FROM messages)",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;

        if existing_row_count > 0 {
            mark_initialized(conn)?;
            return Ok(());
        }

        let seed_repository = SeedChatRepository::default();
        self.replace_chat_domain_seed(conn, seed_repository.load_domain_seed()?)
    }

    fn replace_chat_domain_seed(
        &self,
        conn: &mut rusqlite::Connection,
        seed: ChatDomainSeed,
    ) -> Result<(), String> {
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        tx.execute_batch(
            r#"
            DELETE FROM group_members;
            DELETE FROM group_profiles;
            DELETE FROM messages;
            DELETE FROM sessions;
            DELETE FROM contacts;
            DELETE FROM circles;
            "#,
        )
        .map_err(|error| error.to_string())?;

        insert_chat_domain_seed(&tx, seed)?;
        mark_initialized_tx(&tx)?;
        tx.commit().map_err(|error| error.to_string())
    }

    pub fn load_domain_seed_preview(
        &self,
        preferred_session_id: Option<&str>,
        message_limit: u32,
    ) -> Result<ChatDomainSeed, String> {
        let circles = self.load_circles()?;
        let contacts = self.load_contacts()?;
        let sessions = self.load_sessions()?;
        let groups = self.load_groups()?;
        let selected_session_id = preferred_session_id
            .filter(|session_id| sessions.iter().any(|session| session.id == *session_id))
            .map(str::to_string)
            .or_else(|| {
                sessions
                    .iter()
                    .find(|session| !session.archived.unwrap_or(false))
                    .map(|session| session.id.clone())
            })
            .or_else(|| sessions.first().map(|session| session.id.clone()));
        let mut message_store = HashMap::new();

        if let Some(session_id) = selected_session_id {
            let page = self.load_session_messages_page(LoadSessionMessagesInput {
                session_id: session_id.clone(),
                before_message_id: None,
                limit: message_limit,
            })?;
            message_store.insert(session_id, page.messages);
        }

        Ok(ChatDomainSeed {
            circles,
            contacts,
            sessions,
            groups,
            message_store,
        })
    }

    pub fn load_session_messages_page(
        &self,
        input: LoadSessionMessagesInput,
    ) -> Result<ChatSessionMessagesPage, String> {
        self.with_connection(|conn| load_session_messages_page_with_connection(conn, input))
    }

    pub fn load_session_message_updates(
        &self,
        input: LoadSessionMessageUpdatesInput,
    ) -> Result<ChatSessionMessageUpdates, String> {
        self.with_connection(|conn| load_session_message_updates_with_connection(conn, input))
    }

    pub fn load_sessions_overview(&self) -> Result<Vec<SessionItem>, String> {
        self.load_sessions()
    }

    pub fn load_domain_overview(&self) -> Result<ChatDomainOverview, String> {
        Ok(ChatDomainOverview {
            circles: self.load_circles()?,
            contacts: self.load_contacts()?,
            sessions: self.load_sessions()?,
            groups: self.load_groups()?,
        })
    }
}

impl<R: Runtime> ChatRepository for SqliteChatRepository<R> {
    fn load_domain_seed(&self) -> Result<ChatDomainSeed, String> {
        Ok(ChatDomainSeed {
            circles: self.load_circles()?,
            contacts: self.load_contacts()?,
            sessions: self.load_sessions()?,
            groups: self.load_groups()?,
            message_store: self.load_message_store()?,
        })
    }

    fn save_domain_seed(&self, seed: ChatDomainSeed) -> Result<(), String> {
        self.with_connection_mut(|conn| self.replace_chat_domain_seed(conn, seed))
    }

    fn apply_change_set(&self, change_set: ChatDomainChangeSet) -> Result<(), String> {
        self.with_connection_mut(|conn| {
            let tx = conn.transaction().map_err(|error| error.to_string())?;
            apply_chat_domain_change_set(&tx, change_set)?;
            mark_initialized_tx(&tx)?;
            tx.commit().map_err(|error| error.to_string())
        })
    }
}

impl<R: Runtime> SqliteChatRepository<R> {
    fn load_circles(&self) -> Result<Vec<CircleItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, relay, type, status, latency, description FROM circles ORDER BY rowid DESC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(CircleItem {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        relay: row.get(2)?,
                        circle_type: circle_type_from_str(&row.get::<_, String>(3)?)
                            .map_err(sqlite_user_error)?,
                        status: circle_status_from_str(&row.get::<_, String>(4)?)
                            .map_err(sqlite_user_error)?,
                        latency: row.get(5)?,
                        description: row.get(6)?,
                    })
                })
                .map_err(|error| error.to_string())?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        })
    }

    fn load_contacts(&self) -> Result<Vec<ContactItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, initials, handle, pubkey, subtitle, bio, online, blocked FROM contacts ORDER BY rowid DESC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok(ContactItem {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        initials: row.get(2)?,
                        handle: row.get(3)?,
                        pubkey: row.get(4)?,
                        subtitle: row.get(5)?,
                        bio: row.get(6)?,
                        online: optional_i64_to_bool(row.get::<_, Option<i64>>(7)?),
                        blocked: optional_i64_to_bool(row.get::<_, Option<i64>>(8)?),
                    })
                })
                .map_err(|error| error.to_string())?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        })
    }

    fn load_sessions(&self) -> Result<Vec<SessionItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, circle_id, contact_id, name, initials, subtitle, time, unread_count, muted, pinned, draft, kind, category, members, archived FROM sessions ORDER BY rowid DESC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    let unread_count = row
                        .get::<_, Option<i64>>(7)?
                        .map(i64_to_u32)
                        .transpose()
                        .map_err(sqlite_user_error)?;
                    let members = row
                        .get::<_, Option<i64>>(13)?
                        .map(i64_to_u32)
                        .transpose()
                        .map_err(sqlite_user_error)?;

                    Ok(SessionItem {
                        id: row.get(0)?,
                        circle_id: row.get(1)?,
                        contact_id: row.get(2)?,
                        name: row.get(3)?,
                        initials: row.get(4)?,
                        subtitle: row.get(5)?,
                        time: row.get(6)?,
                        unread_count,
                        muted: optional_i64_to_bool(row.get::<_, Option<i64>>(8)?),
                        pinned: optional_i64_to_bool(row.get::<_, Option<i64>>(9)?),
                        draft: row.get(10)?,
                        kind: session_kind_from_str(&row.get::<_, String>(11)?)
                            .map_err(sqlite_user_error)?,
                        category: row.get(12)?,
                        members,
                        archived: optional_i64_to_bool(row.get::<_, Option<i64>>(14)?),
                    })
                })
                .map_err(|error| error.to_string())?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())
        })
    }

    fn load_groups(&self) -> Result<Vec<GroupProfile>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, name, description, muted FROM group_profiles ORDER BY rowid DESC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        optional_i64_to_bool(row.get::<_, Option<i64>>(3)?),
                    ))
                })
                .map_err(|error| error.to_string())?;

            let group_rows = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            let mut groups = Vec::with_capacity(group_rows.len());

            for (session_id, name, description, muted) in group_rows {
                let mut member_stmt = conn
                    .prepare(
                        "SELECT contact_id, role FROM group_members WHERE session_id = ?1 ORDER BY rowid ASC",
                    )
                    .map_err(|error| error.to_string())?;
                let member_rows = member_stmt
                    .query_map([session_id.clone()], |member_row| {
                        let role = member_row
                            .get::<_, Option<String>>(1)?
                            .map(|value| group_role_from_str(&value).map_err(sqlite_user_error))
                            .transpose()?;

                        Ok(GroupMember {
                            contact_id: member_row.get(0)?,
                            role,
                        })
                    })
                    .map_err(|error| error.to_string())?;

                let members = member_rows
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;

                groups.push(GroupProfile {
                    session_id,
                    name,
                    description,
                    members,
                    muted,
                });
            }

            Ok(groups)
        })
    }

    fn load_message_store(&self) -> Result<HashMap<String, Vec<MessageItem>>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, id, kind, author, body, time, meta, delivery_status, remote_id, sync_source, acked_at FROM messages ORDER BY rowid ASC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, message_item_from_row(row, 1)?))
                })
                .map_err(|error| error.to_string())?;

            let mut message_store = HashMap::new();
            for entry in rows {
                let (session_id, message) = entry.map_err(|error| error.to_string())?;
                message_store
                    .entry(session_id)
                    .or_insert_with(Vec::new)
                    .push(message);
            }

            Ok(message_store)
        })
    }
}

fn load_session_messages_page_with_connection(
    conn: &rusqlite::Connection,
    input: LoadSessionMessagesInput,
) -> Result<ChatSessionMessagesPage, String> {
    let limit = input.limit.clamp(1, 100) as usize;
    let requested_limit = i64::try_from(limit + 1).map_err(|error| error.to_string())?;
    let before_rowid = match input.before_message_id.as_deref() {
        Some(before_message_id) => conn
            .query_row(
                "SELECT rowid FROM messages WHERE session_id = ?1 AND id = ?2",
                params![&input.session_id, before_message_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?,
        None => None,
    };

    let rows = if let Some(before_rowid) = before_rowid {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, author, body, time, meta, delivery_status
                        , remote_id, sync_source, acked_at
                 FROM messages
                 WHERE session_id = ?1 AND rowid < ?2
                 ORDER BY rowid DESC
                 LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![&input.session_id, before_rowid, requested_limit],
                |row| message_item_from_row(row, 0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, author, body, time, meta, delivery_status
                        , remote_id, sync_source, acked_at
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY rowid DESC
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![&input.session_id, requested_limit], |row| {
                message_item_from_row(row, 0)
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };

    let has_more = rows.len() > limit;
    let mut messages = rows.into_iter().take(limit).collect::<Vec<_>>();
    messages.reverse();
    let next_before_message_id = has_more.then(|| messages[0].id.clone());

    Ok(ChatSessionMessagesPage {
        session_id: input.session_id,
        has_more,
        next_before_message_id,
        messages,
    })
}

fn load_session_message_updates_with_connection(
    conn: &rusqlite::Connection,
    input: LoadSessionMessageUpdatesInput,
) -> Result<ChatSessionMessageUpdates, String> {
    let limit = input.limit.clamp(1, 100) as usize;
    let requested_limit = i64::try_from(limit + 1).map_err(|error| error.to_string())?;
    let after_rowid = match input.after_message_id.as_deref() {
        Some(after_message_id) => conn
            .query_row(
                "SELECT rowid FROM messages WHERE session_id = ?1 AND id = ?2",
                params![&input.session_id, after_message_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?,
        None => None,
    };

    let rows = if let Some(after_rowid) = after_rowid {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, author, body, time, meta, delivery_status
                        , remote_id, sync_source, acked_at
                 FROM messages
                 WHERE session_id = ?1 AND rowid > ?2
                 ORDER BY rowid ASC
                 LIMIT ?3",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(
                params![&input.session_id, after_rowid, requested_limit],
                |row| message_item_from_row(row, 0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, author, body, time, meta, delivery_status
                        , remote_id, sync_source, acked_at
                 FROM messages
                 WHERE session_id = ?1
                 ORDER BY rowid DESC
                 LIMIT ?2",
            )
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map(params![&input.session_id, requested_limit], |row| {
                message_item_from_row(row, 0)
            })
            .map_err(|error| error.to_string())?;
        let mut rows = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows.reverse();
        rows
    };

    let has_more = rows.len() > limit;
    let messages = rows.into_iter().take(limit).collect::<Vec<_>>();
    let next_after_message_id = has_more
        .then(|| messages.last().map(|message| message.id.clone()))
        .flatten();

    Ok(ChatSessionMessageUpdates {
        session_id: input.session_id,
        has_more,
        next_after_message_id,
        messages,
    })
}

fn insert_chat_domain_seed(
    tx: &rusqlite::Transaction<'_>,
    seed: ChatDomainSeed,
) -> Result<(), String> {
    for circle in seed.circles.into_iter().rev() {
        insert_circle(tx, &circle)?;
    }

    for contact in seed.contacts.into_iter().rev() {
        insert_contact(tx, &contact)?;
    }

    for session in seed.sessions.into_iter().rev() {
        insert_session(tx, &session)?;
    }

    for group in seed.groups.into_iter().rev() {
        insert_group(tx, &group)?;
    }

    for (session_id, messages) in seed.message_store {
        for message in messages {
            insert_message(tx, &session_id, &message)?;
        }
    }

    Ok(())
}

fn apply_chat_domain_change_set(
    tx: &rusqlite::Transaction<'_>,
    change_set: ChatDomainChangeSet,
) -> Result<(), String> {
    for circle_id in change_set.circle_ids_to_delete {
        delete_circle(tx, &circle_id)?;
    }

    for session_id in change_set.session_ids_to_delete {
        delete_session(tx, &session_id)?;
    }

    for upsert in change_set.circles_upsert {
        upsert_circle(tx, upsert)?;
    }

    for upsert in change_set.contacts_upsert {
        upsert_contact(tx, upsert)?;
    }

    for upsert in change_set.sessions_upsert {
        upsert_session(tx, upsert)?;
    }

    for upsert in change_set.groups_upsert {
        upsert_group(tx, upsert)?;
    }

    for (session_id, messages) in change_set.messages_replace {
        tx.execute("DELETE FROM messages WHERE session_id = ?1", [&session_id])
            .map_err(|error| error.to_string())?;
        for message in messages {
            insert_message(tx, &session_id, &message)?;
        }
    }

    for (session_id, message) in change_set.messages_upsert {
        upsert_message(tx, &session_id, &message)?;
    }

    for (session_id, message) in change_set.messages_append {
        insert_message(tx, &session_id, &message)?;
    }

    Ok(())
}

fn upsert_circle(
    tx: &rusqlite::Transaction<'_>,
    upsert: ChatUpsert<CircleItem>,
) -> Result<(), String> {
    if upsert.move_to_top {
        tx.execute("DELETE FROM circles WHERE id = ?1", [&upsert.item.id])
            .map_err(|error| error.to_string())?;
        return insert_circle(tx, &upsert.item);
    }

    tx.execute(
        "INSERT INTO circles (id, name, relay, type, status, latency, description)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           relay = excluded.relay,
           type = excluded.type,
           status = excluded.status,
           latency = excluded.latency,
           description = excluded.description",
        params![
            upsert.item.id,
            upsert.item.name,
            upsert.item.relay,
            circle_type_to_str(&upsert.item.circle_type),
            circle_status_to_str(&upsert.item.status),
            upsert.item.latency,
            upsert.item.description
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn upsert_contact(
    tx: &rusqlite::Transaction<'_>,
    upsert: ChatUpsert<ContactItem>,
) -> Result<(), String> {
    if upsert.move_to_top {
        tx.execute("DELETE FROM contacts WHERE id = ?1", [&upsert.item.id])
            .map_err(|error| error.to_string())?;
        return insert_contact(tx, &upsert.item);
    }

    tx.execute(
        "INSERT INTO contacts (id, name, initials, handle, pubkey, subtitle, bio, online, blocked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
           name = excluded.name,
           initials = excluded.initials,
           handle = excluded.handle,
           pubkey = excluded.pubkey,
           subtitle = excluded.subtitle,
           bio = excluded.bio,
           online = excluded.online,
           blocked = excluded.blocked",
        params![
            upsert.item.id,
            upsert.item.name,
            upsert.item.initials,
            upsert.item.handle,
            upsert.item.pubkey,
            upsert.item.subtitle,
            upsert.item.bio,
            optional_bool_to_i64(upsert.item.online),
            optional_bool_to_i64(upsert.item.blocked)
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn upsert_session(
    tx: &rusqlite::Transaction<'_>,
    upsert: ChatUpsert<SessionItem>,
) -> Result<(), String> {
    if upsert.move_to_top {
        tx.execute("DELETE FROM sessions WHERE id = ?1", [&upsert.item.id])
            .map_err(|error| error.to_string())?;
        return insert_session(tx, &upsert.item);
    }

    tx.execute(
        "INSERT INTO sessions (id, circle_id, contact_id, name, initials, subtitle, time, unread_count, muted, pinned, draft, kind, category, members, archived)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
         ON CONFLICT(id) DO UPDATE SET
           circle_id = excluded.circle_id,
           contact_id = excluded.contact_id,
           name = excluded.name,
           initials = excluded.initials,
           subtitle = excluded.subtitle,
           time = excluded.time,
           unread_count = excluded.unread_count,
           muted = excluded.muted,
           pinned = excluded.pinned,
           draft = excluded.draft,
           kind = excluded.kind,
           category = excluded.category,
           members = excluded.members,
           archived = excluded.archived",
        params![
            upsert.item.id,
            upsert.item.circle_id,
            upsert.item.contact_id,
            upsert.item.name,
            upsert.item.initials,
            upsert.item.subtitle,
            upsert.item.time,
            upsert.item.unread_count.map(i64::from),
            optional_bool_to_i64(upsert.item.muted),
            optional_bool_to_i64(upsert.item.pinned),
            upsert.item.draft,
            session_kind_to_str(&upsert.item.kind),
            upsert.item.category,
            upsert.item.members.map(i64::from),
            optional_bool_to_i64(upsert.item.archived)
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn upsert_group(
    tx: &rusqlite::Transaction<'_>,
    upsert: ChatUpsert<GroupProfile>,
) -> Result<(), String> {
    let session_id = upsert.item.session_id.clone();
    tx.execute(
        "DELETE FROM group_members WHERE session_id = ?1",
        [&session_id],
    )
    .map_err(|error| error.to_string())?;

    if upsert.move_to_top {
        tx.execute(
            "DELETE FROM group_profiles WHERE session_id = ?1",
            [&session_id],
        )
        .map_err(|error| error.to_string())?;
        return insert_group(tx, &upsert.item);
    }

    tx.execute(
        "INSERT INTO group_profiles (session_id, name, description, muted)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(session_id) DO UPDATE SET
           name = excluded.name,
           description = excluded.description,
           muted = excluded.muted",
        params![
            upsert.item.session_id,
            upsert.item.name,
            upsert.item.description,
            optional_bool_to_i64(upsert.item.muted)
        ],
    )
    .map_err(|error| error.to_string())?;

    for member in upsert.item.members {
        insert_group_member(tx, &session_id, &member)?;
    }

    Ok(())
}

fn delete_circle(tx: &rusqlite::Transaction<'_>, circle_id: &str) -> Result<(), String> {
    let session_ids = load_session_ids_for_circle(tx, circle_id)?;
    for session_id in session_ids {
        delete_session(tx, &session_id)?;
    }

    tx.execute("DELETE FROM circles WHERE id = ?1", [circle_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn delete_session(tx: &rusqlite::Transaction<'_>, session_id: &str) -> Result<(), String> {
    tx.execute(
        "DELETE FROM group_members WHERE session_id = ?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM group_profiles WHERE session_id = ?1",
        [session_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM messages WHERE session_id = ?1", [session_id])
        .map_err(|error| error.to_string())?;
    tx.execute("DELETE FROM sessions WHERE id = ?1", [session_id])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn load_session_ids_for_circle(
    conn: &rusqlite::Connection,
    circle_id: &str,
) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT id FROM sessions WHERE circle_id = ?1 ORDER BY rowid ASC")
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([circle_id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn insert_circle(tx: &rusqlite::Transaction<'_>, circle: &CircleItem) -> Result<(), String> {
    tx.execute(
        "INSERT INTO circles (id, name, relay, type, status, latency, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            &circle.id,
            &circle.name,
            &circle.relay,
            circle_type_to_str(&circle.circle_type),
            circle_status_to_str(&circle.status),
            &circle.latency,
            &circle.description
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn insert_contact(tx: &rusqlite::Transaction<'_>, contact: &ContactItem) -> Result<(), String> {
    tx.execute(
        "INSERT INTO contacts (id, name, initials, handle, pubkey, subtitle, bio, online, blocked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            &contact.id,
            &contact.name,
            &contact.initials,
            &contact.handle,
            &contact.pubkey,
            &contact.subtitle,
            &contact.bio,
            optional_bool_to_i64(contact.online),
            optional_bool_to_i64(contact.blocked)
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn insert_session(tx: &rusqlite::Transaction<'_>, session: &SessionItem) -> Result<(), String> {
    tx.execute(
        "INSERT INTO sessions (id, circle_id, contact_id, name, initials, subtitle, time, unread_count, muted, pinned, draft, kind, category, members, archived) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            &session.id,
            &session.circle_id,
            &session.contact_id,
            &session.name,
            &session.initials,
            &session.subtitle,
            &session.time,
            session.unread_count.map(i64::from),
            optional_bool_to_i64(session.muted),
            optional_bool_to_i64(session.pinned),
            &session.draft,
            session_kind_to_str(&session.kind),
            &session.category,
            session.members.map(i64::from),
            optional_bool_to_i64(session.archived)
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn insert_group(tx: &rusqlite::Transaction<'_>, group: &GroupProfile) -> Result<(), String> {
    tx.execute(
        "INSERT INTO group_profiles (session_id, name, description, muted) VALUES (?1, ?2, ?3, ?4)",
        params![
            &group.session_id,
            &group.name,
            &group.description,
            optional_bool_to_i64(group.muted)
        ],
    )
    .map_err(|error| error.to_string())?;

    for member in &group.members {
        insert_group_member(tx, &group.session_id, member)?;
    }

    Ok(())
}

fn insert_group_member(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    member: &GroupMember,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO group_members (session_id, contact_id, role) VALUES (?1, ?2, ?3)",
        params![
            session_id,
            &member.contact_id,
            member.role.as_ref().map(group_role_to_str)
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn insert_message(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    message: &MessageItem,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO messages (id, session_id, kind, author, body, time, meta, delivery_status, remote_id, sync_source, acked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            &message.id,
            session_id,
            message_kind_to_str(&message.kind),
            message_author_to_str(&message.author),
            &message.body,
            &message.time,
            &message.meta,
            message
                .delivery_status
                .as_ref()
                .map(message_delivery_status_to_str),
            &message.remote_id,
            message.sync_source.as_ref().map(message_sync_source_to_str),
            &message.acked_at
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn upsert_message(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    message: &MessageItem,
) -> Result<(), String> {
    let Some(existing_message) = load_existing_message(tx, message)? else {
        return insert_message(tx, session_id, message);
    };
    let merged_message = merge_message_records(existing_message, message.clone());
    update_existing_message(tx, session_id, &merged_message)
}

fn mark_initialized(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute(
        "INSERT INTO chat_meta (id, initialized) VALUES (1, 1) ON CONFLICT(id) DO UPDATE SET initialized = excluded.initialized",
        [],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn mark_initialized_tx(tx: &rusqlite::Transaction<'_>) -> Result<(), String> {
    tx.execute(
        "INSERT INTO chat_meta (id, initialized) VALUES (1, 1) ON CONFLICT(id) DO UPDATE SET initialized = excluded.initialized",
        [],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn message_item_from_row(
    row: &rusqlite::Row<'_>,
    offset: usize,
) -> Result<MessageItem, rusqlite::Error> {
    Ok(MessageItem {
        id: row.get(offset)?,
        kind: message_kind_from_str(&row.get::<_, String>(offset + 1)?)
            .map_err(sqlite_user_error)?,
        author: message_author_from_str(&row.get::<_, String>(offset + 2)?)
            .map_err(sqlite_user_error)?,
        body: row.get(offset + 3)?,
        time: row.get(offset + 4)?,
        meta: row.get(offset + 5)?,
        delivery_status: row
            .get::<_, Option<String>>(offset + 6)?
            .map(|value| message_delivery_status_from_str(&value).map_err(sqlite_user_error))
            .transpose()?,
        remote_id: row.get(offset + 7)?,
        sync_source: row
            .get::<_, Option<String>>(offset + 8)?
            .map(|value| message_sync_source_from_str(&value).map_err(sqlite_user_error))
            .transpose()?,
        acked_at: row.get(offset + 9)?,
    })
}

fn load_existing_message(
    conn: &rusqlite::Connection,
    message: &MessageItem,
) -> Result<Option<MessageItem>, String> {
    let existing_message = if let Some(remote_id) = message.remote_id.as_deref() {
        conn.query_row(
            "SELECT id, kind, author, body, time, meta, delivery_status, remote_id, sync_source, acked_at
             FROM messages
             WHERE id = ?1 OR remote_id = ?2
             ORDER BY CASE WHEN id = ?1 THEN 0 ELSE 1 END
             LIMIT 1",
            params![&message.id, remote_id],
            |row| message_item_from_row(row, 0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    } else {
        conn.query_row(
            "SELECT id, kind, author, body, time, meta, delivery_status, remote_id, sync_source, acked_at
             FROM messages
             WHERE id = ?1
             LIMIT 1",
            params![&message.id],
            |row| message_item_from_row(row, 0),
        )
        .optional()
        .map_err(|error| error.to_string())?
    };

    Ok(existing_message)
}

fn update_existing_message(
    tx: &rusqlite::Transaction<'_>,
    session_id: &str,
    message: &MessageItem,
) -> Result<(), String> {
    tx.execute(
        "UPDATE messages
         SET session_id = ?2,
             kind = ?3,
             author = ?4,
             body = ?5,
             time = ?6,
             meta = ?7,
             delivery_status = ?8,
             remote_id = ?9,
             sync_source = ?10,
             acked_at = ?11
         WHERE id = ?1",
        params![
            &message.id,
            session_id,
            message_kind_to_str(&message.kind),
            message_author_to_str(&message.author),
            &message.body,
            &message.time,
            &message.meta,
            message
                .delivery_status
                .as_ref()
                .map(message_delivery_status_to_str),
            &message.remote_id,
            message.sync_source.as_ref().map(message_sync_source_to_str),
            &message.acked_at
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

fn optional_bool_to_i64(value: Option<bool>) -> Option<i64> {
    value.map(|flag| if flag { 1 } else { 0 })
}

fn optional_i64_to_bool(value: Option<i64>) -> Option<bool> {
    value.map(|flag| flag != 0)
}

fn i64_to_u32(value: i64) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("invalid integer value: {value}"))
}

fn sqlite_user_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn circle_type_to_str(value: &CircleType) -> &'static str {
    match value {
        CircleType::Default => "default",
        CircleType::Paid => "paid",
        CircleType::Bitchat => "bitchat",
        CircleType::Custom => "custom",
    }
}

fn circle_type_from_str(value: &str) -> Result<CircleType, String> {
    match value {
        "default" => Ok(CircleType::Default),
        "paid" => Ok(CircleType::Paid),
        "bitchat" => Ok(CircleType::Bitchat),
        "custom" => Ok(CircleType::Custom),
        _ => Err(format!("unknown circle type: {value}")),
    }
}

fn circle_status_to_str(value: &CircleStatus) -> &'static str {
    match value {
        CircleStatus::Open => "open",
        CircleStatus::Connecting => "connecting",
        CircleStatus::Closed => "closed",
    }
}

fn circle_status_from_str(value: &str) -> Result<CircleStatus, String> {
    match value {
        "open" => Ok(CircleStatus::Open),
        "connecting" => Ok(CircleStatus::Connecting),
        "closed" => Ok(CircleStatus::Closed),
        _ => Err(format!("unknown circle status: {value}")),
    }
}

fn session_kind_to_str(value: &SessionKind) -> &'static str {
    match value {
        SessionKind::Direct => "direct",
        SessionKind::Group => "group",
        SessionKind::SelfChat => "self",
    }
}

fn session_kind_from_str(value: &str) -> Result<SessionKind, String> {
    match value {
        "direct" => Ok(SessionKind::Direct),
        "group" => Ok(SessionKind::Group),
        "self" => Ok(SessionKind::SelfChat),
        _ => Err(format!("unknown session kind: {value}")),
    }
}

fn group_role_to_str(value: &GroupRole) -> &'static str {
    match value {
        GroupRole::Admin => "admin",
        GroupRole::Member => "member",
    }
}

fn group_role_from_str(value: &str) -> Result<GroupRole, String> {
    match value {
        "admin" => Ok(GroupRole::Admin),
        "member" => Ok(GroupRole::Member),
        _ => Err(format!("unknown group role: {value}")),
    }
}

fn message_kind_to_str(value: &MessageKind) -> &'static str {
    match value {
        MessageKind::Text => "text",
        MessageKind::File => "file",
        MessageKind::Audio => "audio",
        MessageKind::System => "system",
    }
}

fn message_kind_from_str(value: &str) -> Result<MessageKind, String> {
    match value {
        "text" => Ok(MessageKind::Text),
        "file" => Ok(MessageKind::File),
        "audio" => Ok(MessageKind::Audio),
        "system" => Ok(MessageKind::System),
        _ => Err(format!("unknown message kind: {value}")),
    }
}

fn message_author_to_str(value: &MessageAuthor) -> &'static str {
    match value {
        MessageAuthor::Me => "me",
        MessageAuthor::Peer => "peer",
        MessageAuthor::System => "system",
    }
}

fn message_author_from_str(value: &str) -> Result<MessageAuthor, String> {
    match value {
        "me" => Ok(MessageAuthor::Me),
        "peer" => Ok(MessageAuthor::Peer),
        "system" => Ok(MessageAuthor::System),
        _ => Err(format!("unknown message author: {value}")),
    }
}

fn message_delivery_status_to_str(value: &MessageDeliveryStatus) -> &'static str {
    match value {
        MessageDeliveryStatus::Sending => "sending",
        MessageDeliveryStatus::Sent => "sent",
        MessageDeliveryStatus::Failed => "failed",
    }
}

fn message_delivery_status_from_str(value: &str) -> Result<MessageDeliveryStatus, String> {
    match value {
        "sending" => Ok(MessageDeliveryStatus::Sending),
        "sent" => Ok(MessageDeliveryStatus::Sent),
        "failed" => Ok(MessageDeliveryStatus::Failed),
        _ => Err(format!("unknown message delivery status: {value}")),
    }
}

fn message_sync_source_to_str(value: &MessageSyncSource) -> &'static str {
    match value {
        MessageSyncSource::Local => "local",
        MessageSyncSource::Relay => "relay",
        MessageSyncSource::System => "system",
    }
}

fn message_sync_source_from_str(value: &str) -> Result<MessageSyncSource, String> {
    match value {
        "local" => Ok(MessageSyncSource::Local),
        "relay" => Ok(MessageSyncSource::Relay),
        "system" => Ok(MessageSyncSource::System),
        _ => Err(format!("unknown message sync source: {value}")),
    }
}

fn ensure_message_delivery_status_column(conn: &rusqlite::Connection) -> Result<(), String> {
    match conn.execute("ALTER TABLE messages ADD COLUMN delivery_status TEXT", []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn ensure_message_remote_id_column(conn: &rusqlite::Connection) -> Result<(), String> {
    match conn.execute("ALTER TABLE messages ADD COLUMN remote_id TEXT", []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn ensure_message_sync_source_column(conn: &rusqlite::Connection) -> Result<(), String> {
    match conn.execute("ALTER TABLE messages ADD COLUMN sync_source TEXT", []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn ensure_message_acked_at_column(conn: &rusqlite::Connection) -> Result<(), String> {
    match conn.execute("ALTER TABLE messages ADD COLUMN acked_at TEXT", []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
