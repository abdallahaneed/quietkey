use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_model.sql"),
    include_str!("../migrations/0003_stt_engine.sql"),
    include_str!("../migrations/0004_parakeet.sql"),
    include_str!("../migrations/0005_history_cards.sql"),
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    conn.pragma_update(None, "foreign_keys", "on")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

/// The last dictated text, in memory only. "Paste last" works even when history is switched off.
static LAST_TEXT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_last_text(t: &str) -> Option<()> {
    *LAST_TEXT.lock().ok()? = Some(t.to_string());
    Some(())
}

pub fn last_text(conn: &Connection) -> rusqlite::Result<Option<String>> {
    if let Some(t) = LAST_TEXT.lock().ok().and_then(|g| g.clone()) {
        return Ok(Some(t));
    }
    last_pasteable(conn)
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
    pub id: String,
    pub created_at: i64,
    pub raw_text: String,
    pub final_text: String,
    pub app_bundle_id: Option<String>,
    pub app_name: Option<String>,
    pub language: Option<String>,
    pub duration_ms: i64,
    pub word_count: i64,
    pub cleanup_level: String,
    pub status: String,
    pub error: Option<String>,
    pub favorite: bool,
    /// A recording is saved for this entry (play it, transcribe it again).
    pub audio: bool,
}

pub struct NewTranscript<'a> {
    pub raw_text: &'a str,
    pub final_text: &'a str,
    pub app_bundle_id: Option<&'a str>,
    pub app_name: Option<&'a str>,
    pub language: Option<&'a str>,
    pub duration_ms: i64,
    pub cleanup_level: &'a str,
    pub status: &'a str,
    pub error: Option<&'a str>,
    /// Words the cleanup removed or changed, for Insights.
    pub fixes: i64,
}

pub fn word_count(text: &str) -> i64 {
    text.split_whitespace().count() as i64
}

/// Writes the transcript and bumps today's usage in one transaction.
/// `local_day` is passed in (YYYY-MM-DD, local time) so this stays testable.
pub fn insert_transcript(conn: &Connection, t: &NewTranscript, local_day: &str) -> rusqlite::Result<Transcript> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    let words = word_count(t.final_text);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "insert into transcripts (id, created_at, raw_text, final_text, app_bundle_id, app_name, language,
           duration_ms, word_count, audio_path, cleanup_level, tone, status, error, updated_at)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, null, ?10, null, ?11, ?12, ?2)",
        params![id, now, t.raw_text, t.final_text, t.app_bundle_id, t.app_name, t.language,
                t.duration_ms, words, t.cleanup_level, t.status, t.error],
    )?;
    if t.status == "pasted" || t.status == "paste_failed" {
        tx.execute(
            "insert into usage_days (day, words, sessions, speaking_ms, fixes) values (?1, ?2, 1, ?3, ?4)
             on conflict(day) do update set words = words + ?2, sessions = sessions + 1, speaking_ms = speaking_ms + ?3, fixes = fixes + ?4",
            params![local_day, words, t.duration_ms, t.fixes],
        )?;
    }
    tx.commit()?;
    get_transcript(conn, &id)
}

const COLS: &str = "id, created_at, raw_text, final_text, app_bundle_id, app_name, language, duration_ms, word_count, cleanup_level, status, error, favorite, audio_path is not null";

fn row_to_transcript(r: &rusqlite::Row) -> rusqlite::Result<Transcript> {
    Ok(Transcript {
        id: r.get(0)?, created_at: r.get(1)?, raw_text: r.get(2)?, final_text: r.get(3)?,
        app_bundle_id: r.get(4)?, app_name: r.get(5)?, language: r.get(6)?, duration_ms: r.get(7)?,
        word_count: r.get(8)?, cleanup_level: r.get(9)?, status: r.get(10)?, error: r.get(11)?,
        favorite: r.get(12)?, audio: r.get(13)?,
    })
}

pub fn get_transcript(conn: &Connection, id: &str) -> rusqlite::Result<Transcript> {
    conn.query_row(&format!("select {COLS} from transcripts where id = ?1"), [id], row_to_transcript)
}

/// Where the entry's recording is saved, if one is.
pub fn audio_path(conn: &Connection, id: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("select audio_path from transcripts where id = ?1", [id], |r| r.get(0))
}

pub fn set_audio_path(conn: &Connection, id: &str, path: &str) -> rusqlite::Result<usize> {
    conn.execute("update transcripts set audio_path = ?2, updated_at = ?3 where id = ?1", params![id, path, now_ms()])
}

pub fn set_favorite(conn: &Connection, id: &str, on: bool) -> rusqlite::Result<usize> {
    conn.execute("update transcripts set favorite = ?2, updated_at = ?3 where id = ?1", params![id, on, now_ms()])
}

/// After transcribing a recording again: new text, same entry. Usage stats keep the first count.
pub fn update_text(conn: &Connection, id: &str, raw: &str, final_text: &str, language: Option<&str>, cleanup_level: &str) -> rusqlite::Result<Transcript> {
    conn.execute(
        "update transcripts set raw_text = ?2, final_text = ?3, word_count = ?4, language = ?5, cleanup_level = ?6, updated_at = ?7 where id = ?1",
        params![id, raw, final_text, word_count(final_text), language, cleanup_level, now_ms()],
    )?;
    get_transcript(conn, id)
}

/// Every saved recording path, to remove files whose entry is gone.
pub fn audio_paths(conn: &Connection) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut stmt = conn.prepare("select audio_path from transcripts where audio_path is not null")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

pub fn list_transcripts(conn: &Connection, before: Option<i64>, limit: u32) -> rusqlite::Result<Vec<Transcript>> {
    let limit = limit.clamp(1, 100);
    let mut stmt = conn.prepare(&format!(
        "select {COLS} from transcripts where created_at < ?1 order by created_at desc, id desc limit ?2"
    ))?;
    let rows = stmt.query_map(params![before.unwrap_or(i64::MAX), limit], row_to_transcript)?;
    rows.collect()
}

pub fn last_pasteable(conn: &Connection) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare(
        "select final_text from transcripts where status in ('pasted','paste_failed') order by created_at desc, id desc limit 1",
    )?;
    let mut rows = stmt.query([])?;
    Ok(match rows.next()? { Some(r) => Some(r.get(0)?), None => None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        migrate(&c).unwrap();
        c
    }

    fn sample<'a>(text: &'a str, status: &'a str) -> NewTranscript<'a> {
        NewTranscript { raw_text: text, final_text: text, app_bundle_id: Some("com.apple.Notes"), app_name: Some("Notes"),
            language: Some("en"), duration_ms: 3000, cleanup_level: "none", status, error: None, fixes: 0 }
    }

    #[test]
    fn migrate_is_idempotent_and_seeds() {
        let c = mem();
        migrate(&c).unwrap();
        let n: i64 = c.query_row("select count(*) from style_settings", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 4);
        let v: i64 = c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        // 0002: existing installs keep the model they had; only the three offered ones are accepted
        assert_eq!(crate::store::model_choice(&c).as_deref(), Some("large-v3-turbo"));
        crate::store::set_model(&c, "small").unwrap();
        assert_eq!(crate::store::model_choice(&c).as_deref(), Some("small"));
        assert!(crate::store::set_model(&c, "tiny").is_err());
        crate::store::set_model(&c, "parakeet-v3").unwrap(); // 0004
        assert_eq!(crate::store::model_choice(&c).as_deref(), Some("parakeet-v3"));
        crate::store::set_model(&c, "cohere-ar").unwrap(); // 0005
        assert_eq!(crate::store::model_choice(&c).as_deref(), Some("cohere-ar"));
    }

    #[test]
    fn favorite_audio_and_new_text() {
        let c = mem();
        let t = insert_transcript(&c, &sample("first take", "pasted"), "2026-11-08").unwrap();
        assert!(!t.favorite && !t.audio);
        set_favorite(&c, &t.id, true).unwrap();
        set_audio_path(&c, &t.id, "/r/a.wav").unwrap();
        let u = update_text(&c, &t.id, "second take here", "Second take here.", Some("en"), "light").unwrap();
        assert!(u.favorite && u.audio);
        assert_eq!((u.final_text.as_str(), u.word_count), ("Second take here.", 3));
        assert_eq!(audio_path(&c, &t.id).unwrap().as_deref(), Some("/r/a.wav"));
        assert!(audio_paths(&c).unwrap().contains("/r/a.wav"));
    }

    #[test]
    fn insert_list_and_usage() {
        let c = mem();
        insert_transcript(&c, &sample("hello there world", "pasted"), "2026-11-06").unwrap();
        insert_transcript(&c, &sample("second one", "paste_failed"), "2026-11-06").unwrap();
        insert_transcript(&c, &sample("gone", "cancelled"), "2026-11-06").unwrap();
        let list = list_transcripts(&c, None, 50).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].final_text, "gone"); // newest first; UUIDv7 breaks ms ties
        let (w, s): (i64, i64) = c
            .query_row("select words, sessions from usage_days where day='2026-11-06'", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!((w, s), (5, 2)); // cancelled does not count
        assert_eq!(last_pasteable(&c).unwrap().as_deref(), Some("second one"));
    }
}
