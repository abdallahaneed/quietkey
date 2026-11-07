//! Settings, dictionary, snippets, styles and data deletion. All validation that protects data lives here or in
//! the schema's CHECK constraints; the WebView is treated as untrusted input.

use crate::db::now_ms;
use rusqlite::{params, Connection, ErrorCode};
use serde::Serialize;
use serde_json::{Map, Value};

/// Turns SQLite constraint errors into messages a person can act on.
pub fn friendly(e: rusqlite::Error, what: &str) -> String {
    match &e {
        rusqlite::Error::SqliteFailure(f, msg) if f.code == ErrorCode::ConstraintViolation => {
            let m = msg.as_deref().unwrap_or("");
            if m.contains("UNIQUE") {
                format!("That {what} already exists")
            } else if m.contains("length") {
                format!("That {what} is too long or empty")
            } else {
                format!("That {what} is not allowed")
            }
        }
        _ => e.to_string(),
    }
}

/* Settings ------------------------------------------------------------------- */

const BOOL_COLS: &[&str] = &[
    "auto_detect_language", "launch_at_login", "show_bar", "show_in_dock", "mute_audio", "sounds",
    "press_enter_enabled", "command_mode_enabled", "onboarding_done",
];
const TEXT_COLS: &[&str] = &[
    "mic_id", "theme", "bar_edge", "storage_mode", "cleanup_level", "llm_provider", "groq_model", "ollama_model", "ollama_url",
];
const JSON_COLS: &[&str] = &["shortcuts", "mic_ranking", "languages"];

pub fn settings_get(conn: &Connection) -> rusqlite::Result<Value> {
    let mut stmt = conn.prepare("select * from settings where id = 1")?;
    let names: Vec<String> = stmt.column_names().map(|s| s.to_string()).collect();
    stmt.query_row([], |row| {
        let mut m = Map::new();
        for (i, name) in names.iter().enumerate() {
            if name == "id" {
                continue;
            }
            let v = if BOOL_COLS.contains(&name.as_str()) {
                Value::Bool(row.get::<_, i64>(i)? != 0)
            } else if JSON_COLS.contains(&name.as_str()) {
                serde_json::from_str(&row.get::<_, String>(i)?).unwrap_or(Value::Null)
            } else if name == "updated_at" {
                Value::from(row.get::<_, i64>(i)?)
            } else {
                row.get::<_, Option<String>>(i)?.map(Value::String).unwrap_or(Value::Null)
            };
            m.insert(name.clone(), v);
        }
        Ok(Value::Object(m))
    })
}

/// Partial update. Unknown keys and wrong types are rejected before touching the database; value ranges are
/// enforced by the schema's CHECK constraints (the whole update is one transaction, so nothing half-applies).
pub fn settings_update(conn: &Connection, patch: &Map<String, Value>) -> Result<Value, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (k, v) in patch {
        let col = k.as_str();
        let sql = format!("update settings set {col} = ?1 where id = 1");
        let res = if BOOL_COLS.contains(&col) {
            let b = v.as_bool().ok_or(format!("{col} must be true or false"))?;
            tx.execute(&sql, [b as i64])
        } else if TEXT_COLS.contains(&col) {
            let s = match v {
                Value::Null if col == "mic_id" => None,
                Value::String(s) => Some(s.trim().to_string()),
                _ => return Err(format!("{col} must be text")),
            };
            validate_text(col, s.as_deref())?;
            tx.execute(&sql, params![s])
        } else if JSON_COLS.contains(&col) {
            validate_json(col, v)?;
            tx.execute(&sql, [v.to_string()])
        } else {
            return Err(format!("unknown setting {col}"));
        };
        res.map_err(|e| friendly(e, "setting value"))?;
    }
    tx.execute("update settings set updated_at = ?1 where id = 1", [now_ms()]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    settings_get(conn).map_err(|e| e.to_string())
}

fn validate_text(col: &str, s: Option<&str>) -> Result<(), String> {
    match (col, s) {
        ("groq_model", Some(m)) if m.is_empty() || m.len() > 100 => Err("Enter a Groq model id".into()),
        ("ollama_model", Some(m)) if m.is_empty() || m.len() > 100 => Err("Enter an Ollama model name".into()),
        ("ollama_url", Some(u)) if !(u.starts_with("http://127.0.0.1") || u.starts_with("http://localhost")) => {
            Err("Ollama must run on this Mac (http://localhost or http://127.0.0.1)".into())
        }
        _ => Ok(()),
    }
}

fn validate_json(col: &str, v: &Value) -> Result<(), String> {
    match col {
        "languages" => {
            let arr = v.as_array().ok_or("languages must be a list")?;
            if arr.iter().any(|x| !x.as_str().is_some_and(|s| (2..=8).contains(&s.len()))) {
                return Err("languages must be language codes like \"en\"".into());
            }
            Ok(())
        }
        "mic_ranking" => v.as_array().map(|_| ()).ok_or_else(|| "mic_ranking must be a list".into()),
        "shortcuts" => push_to_talk(v).map(|_| ()),
        _ => Ok(()),
    }
}

/// The push-to-talk keys from the shortcuts setting, validated, as the listener's mask.
pub fn push_to_talk(shortcuts: &Value) -> Result<u64, String> {
    let keys: Vec<String> = shortcuts["push_to_talk"][0]
        .as_array()
        .ok_or("shortcuts.push_to_talk must be a list of keys")?
        .iter()
        .map(|k| k.as_str().map(String::from).ok_or("keys must be text"))
        .collect::<Result<_, _>>()?;
    crate::hotkey::parse_shortcut(&keys)
}

/// Fields the pipeline reads on every dictation.
pub struct PipelineSettings {
    pub cleanup_level: String,
    pub llm_provider: String,
    pub groq_model: String,
    pub ollama_model: String,
    pub ollama_url: String,
    pub storage_mode: String,
    pub language: Option<String>,
    /// Several languages picked with auto-detect off: Whisper chooses among these only.
    pub candidates: Vec<String>,
}

pub fn pipeline_settings(conn: &Connection) -> rusqlite::Result<PipelineSettings> {
    conn.query_row(
        "select cleanup_level, llm_provider, groq_model, ollama_model, ollama_url, storage_mode, languages, auto_detect_language
         from settings where id = 1",
        [],
        |r| {
            let langs: Vec<String> = serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default();
            let auto: i64 = r.get(7)?;
            // One language picked and no auto-detect: tell Whisper. Otherwise let it detect.
            let language = if auto == 0 && langs.len() == 1 { langs.first().cloned() } else { None };
            let candidates = if auto == 0 && langs.len() > 1 { langs } else { Vec::new() };
            Ok(PipelineSettings {
                cleanup_level: r.get(0)?, llm_provider: r.get(1)?, groq_model: r.get(2)?, ollama_model: r.get(3)?,
                ollama_url: r.get(4)?, storage_mode: r.get(5)?, language, candidates,
            })
        },
    )
}

/// The microphone the user picked, by device name. None = system default.
/// The picked Whisper model id. Written only by model_select once the model has loaded, never by settings_update,
/// so it always names a file that is on disk.
pub fn model_choice(conn: &Connection) -> Option<String> {
    conn.query_row("select model from settings where id = 1", [], |r| r.get::<_, String>(0)).ok()
}

pub fn set_model(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("update settings set model = ?1, updated_at = ?2 where id = 1", params![id, now_ms()]).map(|_| ())
}

pub fn mic_choice(conn: &Connection) -> Option<String> {
    conn.query_row("select mic_id from settings where id = 1", [], |r| r.get::<_, Option<String>>(0)).ok().flatten()
}

/* Dictionary ------------------------------------------------------------------ */

#[derive(Serialize, Debug, PartialEq)]
pub struct DictEntry {
    pub id: String,
    pub word: String,
    pub misspelling: Option<String>,
    pub source: String,
    pub created_at: i64,
}

fn clean_field(s: &str) -> Option<String> {
    let t = s.trim();
    // Entries made only of punctuation are rejected.
    if t.is_empty() || !t.chars().any(|c| c.is_alphanumeric()) { None } else { Some(t.to_string()) }
}

pub fn dict_list(conn: &Connection) -> rusqlite::Result<Vec<DictEntry>> {
    let mut s = conn.prepare("select id, word, misspelling, source, created_at from dictionary_entries order by lower(word)")?;
    let rows = s.query_map([], |r| Ok(DictEntry { id: r.get(0)?, word: r.get(1)?, misspelling: r.get(2)?, source: r.get(3)?, created_at: r.get(4)? }))?;
    rows.collect()
}

pub fn dict_add(conn: &Connection, word: &str, misspelling: Option<&str>, source: &str) -> Result<String, String> {
    let w = clean_field(word).ok_or("Enter a word")?;
    let m = misspelling.and_then(clean_field);
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    conn.execute(
        "insert into dictionary_entries (id, word, misspelling, source, created_at, updated_at) values (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, w, m, source, now],
    )
    .map_err(|e| friendly(e, "word"))?;
    Ok(id)
}

pub fn dict_update(conn: &Connection, id: &str, word: &str, misspelling: Option<&str>) -> Result<(), String> {
    let w = clean_field(word).ok_or("Enter a word")?;
    let n = conn
        .execute(
            "update dictionary_entries set word = ?2, misspelling = ?3, updated_at = ?4 where id = ?1",
            params![id, w, misspelling.and_then(clean_field), now_ms()],
        )
        .map_err(|e| friendly(e, "word"))?;
    if n == 0 { Err("That word no longer exists".into()) } else { Ok(()) }
}

pub fn delete_many(conn: &Connection, table: &str, ids: &[String]) -> rusqlite::Result<usize> {
    // table is never user input: only called with literals from lib.rs
    debug_assert!(matches!(table, "dictionary_entries" | "snippets" | "transcripts"));
    let tx = conn.unchecked_transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute(&format!("delete from {table} where id = ?1"), [id])?;
    }
    tx.commit()?;
    Ok(n)
}

#[derive(Serialize, Debug, PartialEq)]
pub struct ImportResult {
    pub added: usize,
    pub skipped: usize,
}

/// One entry per line: `word` or `wrong spelling => right spelling` (also `->`). Lines starting with # are ignored.
pub fn dict_import(conn: &Connection, content: &str) -> Result<ImportResult, String> {
    if content.len() > 3 * 1024 * 1024 {
        return Err("File is larger than 3 MB".into());
    }
    let lines: Vec<&str> = content.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if lines.len() > 1000 {
        return Err("File has more than 1,000 entries".into());
    }
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let (mut added, mut skipped) = (0, 0);
    for line in lines {
        let (word, wrong) = match line.split_once("=>").or_else(|| line.split_once("->")) {
            Some((wrong, right)) => (right, Some(wrong)),
            None => (line, None),
        };
        match dict_add(&tx, word, wrong, "import") {
            Ok(_) => added += 1,
            Err(_) => skipped += 1, // duplicates, too long, punctuation only
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ImportResult { added, skipped })
}

pub fn dict_words(conn: &Connection) -> Vec<String> {
    conn.prepare("select word from dictionary_entries order by updated_at desc limit 100")
        .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(0))?.collect())
        .unwrap_or_default()
}

pub fn dict_corrections(conn: &Connection) -> Vec<crate::cleanup::Correction> {
    conn.prepare("select misspelling, word from dictionary_entries where misspelling is not null")
        .and_then(|mut s| {
            s.query_map([], |r| Ok(crate::cleanup::Correction { wrong: r.get(0)?, right: r.get(1)? }))?.collect()
        })
        .unwrap_or_default()
}

/* Snippets ---------------------------------------------------------------------- */

#[derive(Serialize, Debug, PartialEq)]
pub struct SnippetRow {
    pub id: String,
    pub trigger: String,
    pub expansion: String,
    pub updated_at: i64,
}

pub fn snip_list(conn: &Connection) -> rusqlite::Result<Vec<SnippetRow>> {
    let mut s = conn.prepare("select id, trigger, expansion, updated_at from snippets order by lower(trigger)")?;
    let rows = s.query_map([], |r| Ok(SnippetRow { id: r.get(0)?, trigger: r.get(1)?, expansion: r.get(2)?, updated_at: r.get(3)? }))?;
    rows.collect()
}

pub fn snip_add(conn: &Connection, trigger: &str, expansion: &str) -> Result<String, String> {
    let t = clean_field(trigger).ok_or("Enter what you will say")?;
    if expansion.trim().is_empty() {
        return Err("Enter the text it should become".into());
    }
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    conn.execute(
        "insert into snippets (id, trigger, expansion, created_at, updated_at) values (?1, ?2, ?3, ?4, ?4)",
        params![id, t, expansion, now],
    )
    .map_err(|e| friendly(e, "snippet"))?;
    Ok(id)
}

pub fn snip_update(conn: &Connection, id: &str, trigger: &str, expansion: &str) -> Result<(), String> {
    let t = clean_field(trigger).ok_or("Enter what you will say")?;
    if expansion.trim().is_empty() {
        return Err("Enter the text it should become".into());
    }
    let n = conn
        .execute("update snippets set trigger = ?2, expansion = ?3, updated_at = ?4 where id = ?1", params![id, t, expansion, now_ms()])
        .map_err(|e| friendly(e, "snippet"))?;
    if n == 0 { Err("That snippet no longer exists".into()) } else { Ok(()) }
}

pub fn snippets_for_cleanup(conn: &Connection) -> Vec<crate::cleanup::Snippet> {
    conn.prepare("select trigger, expansion from snippets")
        .and_then(|mut s| s.query_map([], |r| Ok(crate::cleanup::Snippet { trigger: r.get(0)?, expansion: r.get(1)? }))?.collect())
        .unwrap_or_default()
}

/* Styles and app categories -------------------------------------------------------- */

#[derive(Serialize, Debug, PartialEq)]
pub struct StyleRow {
    pub category: String,
    pub tone: String,
}

pub fn styles_get(conn: &Connection) -> rusqlite::Result<Vec<StyleRow>> {
    let mut s = conn.prepare(
        "select category, tone from style_settings order by case category when 'personal' then 0 when 'work' then 1 when 'email' then 2 else 3 end",
    )?;
    let rows = s.query_map([], |r| Ok(StyleRow { category: r.get(0)?, tone: r.get(1)? }))?;
    rows.collect()
}

pub fn style_set(conn: &Connection, category: &str, tone: &str) -> Result<(), String> {
    let n = conn
        .execute("update style_settings set tone = ?2, updated_at = ?3 where category = ?1", params![category, tone, now_ms()])
        .map_err(|e| friendly(e, "tone for this kind of app"))?;
    if n == 0 { Err("Unknown app category".into()) } else { Ok(()) }
}

/// Built-in mapping for common Mac apps; user overrides in app_categories win.
fn builtin_category(bundle: &str) -> &'static str {
    const PERSONAL: &[&str] = &["com.apple.MobileSMS", "net.whatsapp.WhatsApp", "ru.keepcoder.Telegram", "org.whispersystems.signal-desktop", "com.facebook.archon"];
    const WORK: &[&str] = &["com.tinyspeck.slackmacgap", "com.microsoft.teams2", "com.microsoft.teams", "com.hnc.Discord", "notion.id", "com.linear"];
    const EMAIL: &[&str] = &["com.apple.mail", "com.microsoft.Outlook", "com.readdle.smartemail-Mac", "com.superhuman.electron"];
    if PERSONAL.contains(&bundle) {
        "personal"
    } else if WORK.contains(&bundle) {
        "work"
    } else if EMAIL.contains(&bundle) {
        "email"
    } else {
        "other"
    }
}

pub fn tone_for_app(conn: &Connection, bundle: Option<&str>) -> String {
    let cat: String = bundle
        .and_then(|b| {
            conn.query_row("select category from app_categories where bundle_id = ?1", [b], |r| r.get(0)).ok()
                .or_else(|| Some(builtin_category(b).to_string()))
        })
        .unwrap_or_else(|| "other".into());
    conn.query_row("select tone from style_settings where category = ?1", [cat], |r| r.get(0)).unwrap_or_else(|_| "formal".into())
}

/* Deletion and retention ---------------------------------------------------------- */

/// Deletes the chosen categories in one transaction. Requires the literal word DELETE from the UI.
pub fn data_delete(conn: &Connection, categories: &[String], confirm: &str) -> Result<Value, String> {
    if confirm != "DELETE" {
        return Err("Type DELETE to confirm".into());
    }
    let all = categories.iter().any(|c| c == "everything");
    let want = |c: &str| all || categories.iter().any(|x| x == c);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut counts = Map::new();
    let mut run = |key: &str, sql: &str| -> Result<(), String> {
        let n = tx.execute(sql, []).map_err(|e| e.to_string())?;
        counts.insert(key.into(), Value::from(n));
        Ok(())
    };
    if want("history") { run("history", "delete from transcripts")?; }
    if want("dictionary") { run("dictionary", "delete from dictionary_entries")?; }
    if want("snippets") { run("snippets", "delete from snippets")?; }
    if want("personalization") {
        run("personalization", "update style_settings set tone = 'formal'")?;
        run("app_categories", "delete from app_categories")?;
    }
    if want("usage") { run("usage", "delete from usage_days")?; }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Value::Object(counts))
}

/// Runs hourly and at startup. In auto-delete mode, history older than 24 h goes.
pub fn retention_purge(conn: &Connection, storage_mode: &str, now: i64) -> rusqlite::Result<usize> {
    match storage_mode {
        "auto_delete_24h" => conn.execute("delete from transcripts where created_at < ?1", [now - 24 * 3600 * 1000]),
        "never" => conn.execute("delete from transcripts", []),
        _ => Ok(0),
    }
}

/// For "never store" mode: the stats still count, the text is not kept.
pub fn record_usage(conn: &Connection, day: &str, words: i64, speaking_ms: i64, fixes: i64) -> rusqlite::Result<()> {
    conn.execute(
        "insert into usage_days (day, words, sessions, speaking_ms, fixes) values (?1, ?2, 1, ?3, ?4)
         on conflict(day) do update set words = words + ?2, sessions = sessions + 1, speaking_ms = speaking_ms + ?3, fixes = fixes + ?4",
        params![day, words, speaking_ms, fixes],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        // reuse the real migrations
        let dir = std::env::temp_dir().join(format!("fc-test-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::db::open(&dir.join("t.db")).unwrap()
    }

    #[test]
    fn settings_roundtrip_and_validation() {
        let c = mem();
        let s = settings_get(&c).unwrap();
        assert_eq!(s["llm_provider"], "none"); // offline by default
        assert_eq!(s["launch_at_login"], true);
        assert_eq!(s["languages"], serde_json::json!(["en"]));
        let mut p = Map::new();
        p.insert("cleanup_level".into(), "medium".into());
        p.insert("sounds".into(), false.into());
        let s = settings_update(&c, &p).unwrap();
        assert_eq!(s["cleanup_level"], "medium");
        assert_eq!(s["sounds"], false);
        // CHECK constraint rejects, and nothing half-applies
        let mut bad = Map::new();
        bad.insert("sounds".into(), true.into());
        bad.insert("cleanup_level".into(), "extreme".into());
        assert!(settings_update(&c, &bad).is_err());
        assert_eq!(settings_get(&c).unwrap()["sounds"], false);
        // unknown key, wrong type, remote ollama
        let mut k = Map::new(); k.insert("id".into(), 2.into());
        assert!(settings_update(&c, &k).is_err());
        let mut t = Map::new(); t.insert("sounds".into(), "yes".into());
        assert!(settings_update(&c, &t).is_err());
        let mut u = Map::new(); u.insert("ollama_url".into(), "http://10.0.0.5:11434".into());
        assert!(settings_update(&c, &u).is_err());
    }

    #[test]
    fn dictionary_crud_import_and_duplicates() {
        let c = mem();
        let id = dict_add(&c, "  Aneed ", Some("a need"), "manual").unwrap();
        assert_eq!(dict_add(&c, "aneed", Some("A Need"), "manual").unwrap_err(), "That word already exists");
        assert!(dict_add(&c, "...", None, "manual").is_err());
        assert!(dict_add(&c, &"x".repeat(61), None, "manual").is_err());
        dict_update(&c, &id, "Aneed", None).unwrap();
        let r = dict_import(&c, "# names\nTimo\nbest impact => best-impact\nTimo\n!!!\n").unwrap();
        assert_eq!(r, ImportResult { added: 2, skipped: 2 });
        assert_eq!(dict_list(&c).unwrap().len(), 3);
        assert_eq!(dict_corrections(&c).len(), 1);
        assert_eq!(delete_many(&c, "dictionary_entries", &[id]).unwrap(), 1);
    }

    #[test]
    fn snippets_styles_and_tone_lookup() {
        let c = mem();
        snip_add(&c, "my email", "abdallah@example.com").unwrap();
        assert!(snip_add(&c, "My Email", "x").is_err());
        assert!(snip_add(&c, "sig", "  ").is_err());
        style_set(&c, "personal", "very_casual").unwrap();
        assert!(style_set(&c, "work", "very_casual").is_err()); // schema rule
        assert_eq!(tone_for_app(&c, Some("com.apple.MobileSMS")), "very_casual");
        assert_eq!(tone_for_app(&c, Some("com.unknown.app")), "formal");
        assert_eq!(tone_for_app(&c, None), "formal");
    }

    #[test]
    fn delete_requires_confirm_and_retention() {
        let c = mem();
        let day = "2026-11-06";
        let t = crate::db::NewTranscript { raw_text: "a", final_text: "a", app_bundle_id: None, app_name: None, language: None,
            duration_ms: 1, cleanup_level: "none", status: "pasted", error: None, fixes: 0 };
        crate::db::insert_transcript(&c, &t, day).unwrap();
        assert!(data_delete(&c, &["history".into()], "delete").is_err());
        assert_eq!(retention_purge(&c, "auto_delete_24h", now_ms()).unwrap(), 0);
        assert_eq!(retention_purge(&c, "auto_delete_24h", now_ms() + 25 * 3600 * 1000).unwrap(), 1);
        crate::db::insert_transcript(&c, &t, day).unwrap();
        let out = data_delete(&c, &["everything".into()], "DELETE").unwrap();
        assert_eq!(out["history"], 1);
        assert_eq!(out["usage"], 1);
    }
}
