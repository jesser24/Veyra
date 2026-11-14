use crate::crypto::{self, Key};
use crate::vault::Unlocked;
use crate::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{fs, path::{Path, PathBuf}};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Conversation { pub id: String, pub title: String, pub folder_id: Option<String>, pub favorite: bool, pub archived: bool, pub deleted_at: Option<i64>, pub created: i64, pub updated: i64, pub summary: Option<String> }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Message { pub id: String, pub conversation_id: String, pub role: String, pub content: String, pub status: String, pub created: i64 }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AttachmentInfo { pub id: String, pub conversation_id: String, pub name: String, pub mime: String, pub size: i64, pub sha256: String }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AttachmentMeta { pub kind: String, pub summary: String, pub notes_json: String, pub extracted: String, pub width: Option<i64>, pub height: Option<i64> }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AttachmentRow { pub id: String, pub message_id: Option<String>, pub name: String, pub mime: String, pub size: i64, pub kind: String, pub summary: String, pub notes: String, pub width: Option<i64>, pub height: Option<i64> }
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Folder { pub id: String, pub name: String }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View { Active, Favorites, Archived, Trash }

pub struct Store { conn: Connection, dir: PathBuf, file_key: Key, db_key_hex: String }

fn new_id() -> String { crypto::hex(&crypto::random::<16>()) }
const SCHEMA_VERSION: i32 = 2;

impl Store {
    pub fn db_path(dir: &Path) -> PathBuf { dir.join("veyra.db") }
    pub fn attachments_dir(dir: &Path) -> PathBuf { dir.join("attachments") }

    /// Open (or create) the encrypted database in `dir`. A wrong key is detected immediately.
    pub fn open(dir: &Path, u: &Unlocked) -> Result<Store> {
        fs::create_dir_all(Self::attachments_dir(dir))?;
        let conn = Connection::open(Self::db_path(dir))?;
        let db_key_hex = format!("x'{}'", crypto::hex(&u.db_key().0));
        conn.pragma_update(None, "key", &db_key_hex)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|_| Error::WrongSecret)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA secure_delete = ON;")?;
        let s = Store { conn, dir: dir.to_path_buf(), file_key: u.file_key(), db_key_hex };
        s.migrate()?;
        Ok(s)
    }
    fn migrate(&self) -> Result<()> {
        let v: i32 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if v > SCHEMA_VERSION { return Err(Error::Format(format!("database is from a newer Veyra (schema {v})"))); }
        if v < 1 {
            self.conn.execute_batch("
                CREATE TABLE folders(id TEXT PRIMARY KEY, name TEXT NOT NULL, created INTEGER NOT NULL);
                CREATE TABLE conversations(id TEXT PRIMARY KEY, title TEXT NOT NULL, folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL,
                    favorite INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0, deleted_at INTEGER, created INTEGER NOT NULL, updated INTEGER NOT NULL, summary TEXT);
                CREATE TABLE messages(id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    role TEXT NOT NULL CHECK(role IN ('user','assistant','system')), content TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'sent' CHECK(status IN ('sent','pending','error')), created INTEGER NOT NULL);
                CREATE INDEX messages_conv ON messages(conversation_id, created);
                CREATE TABLE attachments(id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                    name TEXT NOT NULL, mime TEXT NOT NULL, size INTEGER NOT NULL, sha256 TEXT NOT NULL, created INTEGER NOT NULL);
                PRAGMA user_version = 1;")?;
        }
        if v < 2 { // attachments belong to a message and carry what was extracted from them (all inside the encrypted database)
            self.conn.execute_batch("
                ALTER TABLE attachments ADD COLUMN message_id TEXT;
                ALTER TABLE attachments ADD COLUMN kind TEXT NOT NULL DEFAULT 'text';
                ALTER TABLE attachments ADD COLUMN summary TEXT NOT NULL DEFAULT '';
                ALTER TABLE attachments ADD COLUMN notes TEXT NOT NULL DEFAULT '[]';
                ALTER TABLE attachments ADD COLUMN extracted TEXT NOT NULL DEFAULT '';
                ALTER TABLE attachments ADD COLUMN width INTEGER;
                ALTER TABLE attachments ADD COLUMN height INTEGER;
                CREATE INDEX attachments_msg ON attachments(message_id);
                PRAGMA user_version = 2;")?;
        }
        Ok(())
    }

    pub fn integrity_check(&self) -> Result<()> {
        let r: String = self.conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if r == "ok" { Ok(()) } else { Err(Error::Integrity(r)) }
    }

    // ---- conversations ----
    pub fn create_conversation(&self, title: &str, now: i64) -> Result<Conversation> {
        let id = new_id();
        self.conn.execute("INSERT INTO conversations(id,title,created,updated) VALUES(?1,?2,?3,?3)", params![id, title, now])?;
        self.conversation(&id)
    }
    fn row(r: &rusqlite::Row) -> rusqlite::Result<Conversation> {
        Ok(Conversation { id: r.get(0)?, title: r.get(1)?, folder_id: r.get(2)?, favorite: r.get::<_, i64>(3)? != 0, archived: r.get::<_, i64>(4)? != 0, deleted_at: r.get(5)?, created: r.get(6)?, updated: r.get(7)?, summary: r.get(8)? })
    }
    const COLS: &'static str = "id,title,folder_id,favorite,archived,deleted_at,created,updated,summary";
    pub fn conversation(&self, id: &str) -> Result<Conversation> {
        self.conn.query_row(&format!("SELECT {} FROM conversations WHERE id=?1", Self::COLS), [id], Self::row).optional()?.ok_or_else(|| Error::NotFound(format!("conversation {id}")))
    }
    pub fn list(&self, view: View, folder: Option<&str>) -> Result<Vec<Conversation>> {
        let cond = match view { View::Active => "deleted_at IS NULL AND archived=0", View::Favorites => "deleted_at IS NULL AND favorite=1", View::Archived => "deleted_at IS NULL AND archived=1", View::Trash => "deleted_at IS NOT NULL" };
        let sql = format!("SELECT {} FROM conversations WHERE {cond} AND (?1 IS NULL OR folder_id=?1) ORDER BY updated DESC", Self::COLS);
        let mut st = self.conn.prepare(&sql)?;
        let v = st.query_map([folder], Self::row)?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }
    pub fn rename(&self, id: &str, title: &str) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET title=?2 WHERE id=?1", params![id, title.trim()])?, id) }
    pub fn set_favorite(&self, id: &str, v: bool) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET favorite=?2 WHERE id=?1", params![id, v])?, id) }
    pub fn set_archived(&self, id: &str, v: bool) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET archived=?2 WHERE id=?1", params![id, v])?, id) }
    pub fn move_to_folder(&self, id: &str, folder: Option<&str>) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET folder_id=?2 WHERE id=?1", params![id, folder])?, id) }
    pub fn set_summary(&self, id: &str, summary: &str) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET summary=?2 WHERE id=?1", params![id, summary])?, id) }
    fn touch(&self, changed: usize, id: &str) -> Result<()> { if changed == 0 { Err(Error::NotFound(format!("conversation {id}"))) } else { Ok(()) } }

    // ---- trash ----
    pub fn trash(&self, id: &str, now: i64) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET deleted_at=?2 WHERE id=?1", params![id, now])?, id) }
    pub fn restore(&self, id: &str) -> Result<()> { self.touch(self.conn.execute("UPDATE conversations SET deleted_at=NULL WHERE id=?1", [id])?, id) }
    /// Permanently delete a conversation, its messages AND its encrypted attachment files.
    pub fn delete_permanently(&self, id: &str) -> Result<()> {
        let files: Vec<String> = { let mut st = self.conn.prepare("SELECT id FROM attachments WHERE conversation_id=?1")?; let v = st.query_map([id], |r| r.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?; v };
        for f in files { let p = self.blob_path(&f); if p.exists() { fs::remove_file(p)?; } }
        self.touch(self.conn.execute("DELETE FROM conversations WHERE id=?1", [id])?, id)
    }
    pub fn purge_trash_older_than(&self, cutoff: i64) -> Result<usize> {
        let ids: Vec<String> = { let mut st = self.conn.prepare("SELECT id FROM conversations WHERE deleted_at IS NOT NULL AND deleted_at < ?1")?; let v = st.query_map([cutoff], |r| r.get(0))?.collect::<std::result::Result<Vec<_>, _>>()?; v };
        for id in &ids { self.delete_permanently(id)?; }
        Ok(ids.len())
    }

    // ---- messages ----
    /// Saved immediately, before any model call, so a crash or error never loses what the user typed. `status` is 'pending' until delivered.
    pub fn add_message(&self, conv: &str, role: &str, content: &str, status: &str, now: i64) -> Result<Message> {
        let id = new_id();
        self.conn.execute("INSERT INTO messages(id,conversation_id,role,content,status,created) VALUES(?1,?2,?3,?4,?5,?6)", params![id, conv, role, content, status, now])?;
        self.conn.execute("UPDATE conversations SET updated=?2 WHERE id=?1", params![conv, now])?;
        Ok(Message { id, conversation_id: conv.into(), role: role.into(), content: content.into(), status: status.into(), created: now })
    }
    pub fn set_message(&self, id: &str, content: &str, status: &str) -> Result<()> {
        if self.conn.execute("UPDATE messages SET content=?2,status=?3 WHERE id=?1", params![id, content, status])? == 0 { return Err(Error::NotFound(format!("message {id}"))); } Ok(())
    }
    pub fn messages(&self, conv: &str) -> Result<Vec<Message>> {
        let mut st = self.conn.prepare("SELECT id,conversation_id,role,content,status,created FROM messages WHERE conversation_id=?1 ORDER BY created, rowid")?;
        let v = st.query_map([conv], |r| Ok(Message { id: r.get(0)?, conversation_id: r.get(1)?, role: r.get(2)?, content: r.get(3)?, status: r.get(4)?, created: r.get(5)? }))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }
    pub fn pending_messages(&self) -> Result<Vec<Message>> {
        let mut st = self.conn.prepare("SELECT id,conversation_id,role,content,status,created FROM messages WHERE status='pending' ORDER BY created")?;
        let v = st.query_map([], |r| Ok(Message { id: r.get(0)?, conversation_id: r.get(1)?, role: r.get(2)?, content: r.get(3)?, status: r.get(4)?, created: r.get(5)? }))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }

    /// Case-insensitive search in titles and message contents (trashed conversations excluded). Literal match: `%`, `_` and `\` are escaped.
    pub fn search(&self, q: &str) -> Result<Vec<Conversation>> {
        let pat = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let sql = format!("SELECT {} FROM conversations c WHERE deleted_at IS NULL AND (title LIKE ?1 ESCAPE '\\' OR EXISTS(SELECT 1 FROM messages m WHERE m.conversation_id=c.id AND m.content LIKE ?1 ESCAPE '\\')) ORDER BY updated DESC", Self::COLS);
        let mut st = self.conn.prepare(&sql)?;
        let v = st.query_map([pat], Self::row)?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }

    // ---- folders ----
    pub fn create_folder(&self, name: &str, now: i64) -> Result<Folder> { let id = new_id(); self.conn.execute("INSERT INTO folders(id,name,created) VALUES(?1,?2,?3)", params![id, name.trim(), now])?; Ok(Folder { id, name: name.trim().into() }) }
    pub fn folders(&self) -> Result<Vec<Folder>> { let mut st = self.conn.prepare("SELECT id,name FROM folders ORDER BY name")?; let v = st.query_map([], |r| Ok(Folder { id: r.get(0)?, name: r.get(1)? }))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v) }
    pub fn delete_folder(&self, id: &str) -> Result<()> { self.conn.execute("DELETE FROM folders WHERE id=?1", [id])?; Ok(()) }

    // ---- attachments (encrypted at rest) ----
    fn blob_path(&self, id: &str) -> PathBuf { Self::attachments_dir(&self.dir).join(format!("{id}.bin")) }
    pub fn add_attachment(&self, conv: &str, name: &str, mime: &str, bytes: &[u8], now: i64) -> Result<AttachmentInfo> {
        self.conversation(conv)?;
        let id = new_id();
        let sealed = crypto::seal(&self.file_key, id.as_bytes(), bytes)?;
        let tmp = self.blob_path(&format!("{id}.tmp"));
        fs::write(&tmp, sealed)?; fs::rename(&tmp, self.blob_path(&id))?;
        let sha = crypto::sha256_hex(bytes);
        self.conn.execute("INSERT INTO attachments(id,conversation_id,name,mime,size,sha256,created) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![id, conv, name, mime, bytes.len() as i64, sha, now])?;
        Ok(AttachmentInfo { id, conversation_id: conv.into(), name: name.into(), mime: mime.into(), size: bytes.len() as i64, sha256: sha })
    }
    pub fn add_attachment_full(&self, conv: &str, name: &str, mime: &str, bytes: &[u8], meta: &AttachmentMeta, now: i64) -> Result<AttachmentInfo> {
        let info = self.add_attachment(conv, name, mime, bytes, now)?;
        self.conn.execute("UPDATE attachments SET kind=?2, summary=?3, notes=?4, extracted=?5, width=?6, height=?7 WHERE id=?1", params![info.id, meta.kind, meta.summary, meta.notes_json, meta.extracted, meta.width, meta.height])?; Ok(info)
    }
    pub fn link_attachment(&self, id: &str, message: &str) -> Result<()> { self.conn.execute("UPDATE attachments SET message_id=?2 WHERE id=?1", params![id, message])?; Ok(()) }
    pub fn attachment_rows(&self, conv: &str) -> Result<Vec<AttachmentRow>> {
        let mut st = self.conn.prepare("SELECT id,message_id,name,mime,size,kind,summary,notes,width,height FROM attachments WHERE conversation_id=?1 ORDER BY created")?;
        let v = st.query_map([conv], |r| Ok(AttachmentRow { id: r.get(0)?, message_id: r.get(1)?, name: r.get(2)?, mime: r.get(3)?, size: r.get(4)?, kind: r.get(5)?, summary: r.get(6)?, notes: r.get(7)?, width: r.get(8)?, height: r.get(9)? }))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }
    /// (message_id, name, kind, extracted text) for every attachment of a conversation, to be placed in front of the matching message when the prompt is built.
    pub fn attachment_texts(&self, conv: &str) -> Result<Vec<(String, String, String, String)>> {
        let mut st = self.conn.prepare("SELECT message_id,name,kind,extracted FROM attachments WHERE conversation_id=?1 AND message_id IS NOT NULL ORDER BY created")?;
        let v = st.query_map([conv], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }
    pub fn attachments(&self, conv: &str) -> Result<Vec<AttachmentInfo>> {
        let mut st = self.conn.prepare("SELECT id,conversation_id,name,mime,size,sha256 FROM attachments WHERE conversation_id=?1 ORDER BY created")?;
        let v = st.query_map([conv], |r| Ok(AttachmentInfo { id: r.get(0)?, conversation_id: r.get(1)?, name: r.get(2)?, mime: r.get(3)?, size: r.get(4)?, sha256: r.get(5)? }))?.collect::<std::result::Result<Vec<_>, _>>()?; Ok(v)
    }
    pub fn read_attachment(&self, id: &str) -> Result<Vec<u8>> {
        let sha: String = self.conn.query_row("SELECT sha256 FROM attachments WHERE id=?1", [id], |r| r.get(0)).optional()?.ok_or_else(|| Error::NotFound(format!("attachment {id}")))?;
        let data = crypto::open(&self.file_key, id.as_bytes(), &fs::read(self.blob_path(id))?).map_err(|_| Error::Integrity(format!("attachment {id} is damaged or was altered")))?;
        if crypto::sha256_hex(&data) != sha { return Err(Error::Integrity(format!("attachment {id} does not match its checksum"))); }
        Ok(data)
    }

    /// Consistent snapshot of the encrypted database into `dest`, encrypted with the same key (SQLite online backup API).
    pub fn snapshot_to(&self, dest: &Path) -> Result<()> {
        let _ = fs::remove_file(dest);
        let mut dst = Connection::open(dest)?;
        dst.pragma_update(None, "key", &self.db_key_hex)?;
        rusqlite::backup::Backup::new(&self.conn, &mut dst)?.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        Ok(())
    }
}
