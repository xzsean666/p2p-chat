use crate::domain::chat::{
    AdvancedPreferences, AppPreferences, ChatDomainSeed, CircleItem, CircleStatus, CircleType,
    ContactItem, GroupMember, GroupProfile, GroupRole, LanguagePreference, MessageAuthor,
    MessageItem, MessageKind, NotificationPreferences, PersistedShellState, SessionItem,
    SessionKind, TextSizePreference, ThemePreference,
};
use crate::domain::chat_repository::ChatRepository;
use crate::infra::seed_chat_repository::SeedChatRepository;
use crate::infra::sqlite_connection::open_connection;
use rusqlite::{params, OptionalExtension};
use std::collections::HashMap;

pub struct SqliteChatRepository {
    app_handle: tauri::AppHandle,
}

impl SqliteChatRepository {
    pub fn new(app_handle: &tauri::AppHandle) -> Self {
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
              meta TEXT
            );

            CREATE TABLE IF NOT EXISTS chat_meta (
              id INTEGER PRIMARY KEY CHECK (id = 1),
              initialized INTEGER NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())
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
        self.replace_chat_domain_seed(conn, seed_repository.load_chat_seed()?.into())
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
}

impl ChatRepository for SqliteChatRepository {
    fn load_chat_seed(&self) -> Result<PersistedShellState, String> {
        let circles = self.load_seed_circles()?;
        let sessions = self.load_seed_sessions()?;
        let contacts = self.load_seed_contacts()?;
        let groups = self.load_seed_groups()?;
        let message_store = self.load_seed_message_store()?;

        Ok(PersistedShellState {
            is_authenticated: false,
            circles: circles.clone(),
            app_preferences: AppPreferences {
                theme: ThemePreference::System,
                language: LanguagePreference::En,
                text_size: TextSizePreference::Default,
            },
            notification_preferences: NotificationPreferences {
                allow_send: true,
                allow_receive: false,
                show_badge: true,
                archive_summary: true,
                mentions_only: false,
            },
            advanced_preferences: AdvancedPreferences {
                show_message_info: false,
                use_tor_network: false,
                relay_diagnostics: true,
                experimental_transport: false,
            },
            active_circle_id: circles
                .first()
                .map(|circle| circle.id.clone())
                .unwrap_or_default(),
            selected_session_id: sessions
                .first()
                .map(|session| session.id.clone())
                .unwrap_or_default(),
            sessions,
            contacts,
            groups,
            message_store,
        })
    }

    fn load_seed_circles(&self) -> Result<Vec<CircleItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, relay, type, status, latency, description FROM circles ORDER BY rowid ASC",
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

    fn load_seed_contacts(&self) -> Result<Vec<ContactItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, initials, handle, pubkey, subtitle, bio, online, blocked FROM contacts ORDER BY rowid ASC",
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

    fn load_seed_sessions(&self) -> Result<Vec<SessionItem>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, circle_id, contact_id, name, initials, subtitle, time, unread_count, muted, pinned, draft, kind, category, members, archived FROM sessions ORDER BY rowid ASC",
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

    fn load_seed_groups(&self) -> Result<Vec<GroupProfile>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, name, description, muted FROM group_profiles ORDER BY rowid ASC",
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

    fn load_seed_message_store(&self) -> Result<HashMap<String, Vec<MessageItem>>, String> {
        self.with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, id, kind, author, body, time, meta FROM messages ORDER BY rowid ASC",
                )
                .map_err(|error| error.to_string())?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        MessageItem {
                            id: row.get(1)?,
                            kind: message_kind_from_str(&row.get::<_, String>(2)?)
                                .map_err(sqlite_user_error)?,
                            author: message_author_from_str(&row.get::<_, String>(3)?)
                                .map_err(sqlite_user_error)?,
                            body: row.get(4)?,
                            time: row.get(5)?,
                            meta: row.get(6)?,
                        },
                    ))
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

    fn save_chat_domain_seed(&self, seed: ChatDomainSeed) -> Result<(), String> {
        self.with_connection_mut(|conn| self.replace_chat_domain_seed(conn, seed))
    }
}

fn insert_chat_domain_seed(
    tx: &rusqlite::Transaction<'_>,
    seed: ChatDomainSeed,
) -> Result<(), String> {
    for circle in seed.circles {
        tx.execute(
            "INSERT INTO circles (id, name, relay, type, status, latency, description) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                circle.id,
                circle.name,
                circle.relay,
                circle_type_to_str(&circle.circle_type),
                circle_status_to_str(&circle.status),
                circle.latency,
                circle.description
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    for contact in seed.contacts {
        tx.execute(
            "INSERT INTO contacts (id, name, initials, handle, pubkey, subtitle, bio, online, blocked) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                contact.id,
                contact.name,
                contact.initials,
                contact.handle,
                contact.pubkey,
                contact.subtitle,
                contact.bio,
                optional_bool_to_i64(contact.online),
                optional_bool_to_i64(contact.blocked)
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    for session in seed.sessions {
        tx.execute(
            "INSERT INTO sessions (id, circle_id, contact_id, name, initials, subtitle, time, unread_count, muted, pinned, draft, kind, category, members, archived) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                session.id,
                session.circle_id,
                session.contact_id,
                session.name,
                session.initials,
                session.subtitle,
                session.time,
                session.unread_count.map(i64::from),
                optional_bool_to_i64(session.muted),
                optional_bool_to_i64(session.pinned),
                session.draft,
                session_kind_to_str(&session.kind),
                session.category,
                session.members.map(i64::from),
                optional_bool_to_i64(session.archived)
            ],
        )
        .map_err(|error| error.to_string())?;
    }

    for group in seed.groups {
        tx.execute(
            "INSERT INTO group_profiles (session_id, name, description, muted) VALUES (?1, ?2, ?3, ?4)",
            params![
                &group.session_id,
                group.name,
                group.description,
                optional_bool_to_i64(group.muted)
            ],
        )
        .map_err(|error| error.to_string())?;

        for member in group.members {
            tx.execute(
                "INSERT INTO group_members (session_id, contact_id, role) VALUES (?1, ?2, ?3)",
                params![
                    &group.session_id,
                    member.contact_id,
                    member.role.as_ref().map(group_role_to_str)
                ],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    for (session_id, messages) in seed.message_store {
        for message in messages {
            tx.execute(
                "INSERT INTO messages (id, session_id, kind, author, body, time, meta) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    message.id,
                    &session_id,
                    message_kind_to_str(&message.kind),
                    message_author_to_str(&message.author),
                    message.body,
                    message.time,
                    message.meta
                ],
            )
            .map_err(|error| error.to_string())?;
        }
    }

    Ok(())
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
