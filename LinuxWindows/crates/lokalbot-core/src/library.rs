//! The library handle shared by the app, engine and CLI.
//!
//! Files follow the macOS layout. Every method is synchronous and cheap enough
//! to call from a background task; the UI must not call them on its main
//! thread for whole-library operations (lists, search, rebuilds).

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use chrono::{DateTime, Datelike as _, Local, Utc};
use parking_lot::Mutex;
use rusqlite::Connection;
use uuid::Uuid;

use crate::ids;
use crate::model::*;
use crate::paths::LibraryPaths;

pub const META_FILE: &str = "meta.json";
pub const TRANSCRIPT_FILE: &str = "transcript.json";
pub const TRANSCRIPT_MD_FILE: &str = "transcript.md";
pub const SUMMARY_FILE: &str = "summary.md";
pub const NOTES_FILE: &str = "notes.md";
pub const OUTCOMES_FILE: &str = "outcomes.json";
pub const OUTCOME_STATE_FILE: &str = "outcome-state.json";
pub const MIC_TRACK: &str = "mic.wav";
pub const SYSTEM_TRACK: &str = "system.wav";

/// SQLite schema shared with the macOS app (`SearchIndex.swift`,
/// `ActivityTracker.swift`, `EmbeddingIndex.swift`, `PipelineJobStore.swift`).
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS indexed_meetings (meeting_id TEXT PRIMARY KEY, source_mtime REAL NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS docs USING fts5(
    text, meeting_id UNINDEXED, kind UNINDEXED, start UNINDEXED, speaker UNINDEXED,
    tokenize='unicode61 remove_diacritics 2');
CREATE TABLE IF NOT EXISTS search_document_rows (doc_rowid INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_search_document_rows_meeting_id ON search_document_rows(meeting_id);
CREATE TABLE IF NOT EXISTS search_index_migrations (name TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS deleted_meetings (meeting_id TEXT PRIMARY KEY, deleted_at REAL NOT NULL);

CREATE TABLE IF NOT EXISTS embeddings (meeting_id TEXT NOT NULL, start REAL NOT NULL, text TEXT NOT NULL, vec BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS embedded_meetings (meeting_id TEXT PRIMARY KEY, source_mtime REAL NOT NULL, model_id TEXT NOT NULL DEFAULT '');
CREATE INDEX IF NOT EXISTS idx_embeddings_meeting_id ON embeddings(meeting_id);

CREATE TABLE IF NOT EXISTS activity_blocks (id INTEGER PRIMARY KEY AUTOINCREMENT,
    app TEXT NOT NULL, title TEXT NOT NULL, start REAL NOT NULL, end REAL NOT NULL);
CREATE INDEX IF NOT EXISTS idx_activity_start ON activity_blocks(start);
CREATE TABLE IF NOT EXISTS screenshots (id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts REAL NOT NULL, path TEXT NOT NULL, app TEXT NOT NULL,
    window_title TEXT NOT NULL DEFAULT '', capture_trigger TEXT NOT NULL DEFAULT 'interval',
    perceptual_hash TEXT NOT NULL DEFAULT '', similarity_group INTEGER NOT NULL DEFAULT 0,
    source_url TEXT NOT NULL DEFAULT '', document_name TEXT NOT NULL DEFAULT '',
    meeting_id TEXT NOT NULL DEFAULT '', privacy_redactions INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS screen_bookmarks (snapshot_id INTEGER PRIMARY KEY,
    note TEXT NOT NULL DEFAULT '', created_at REAL NOT NULL,
    FOREIGN KEY(snapshot_id) REFERENCES screenshots(id) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_screenshot_ts ON screenshots(ts);
CREATE INDEX IF NOT EXISTS idx_screenshot_app_ts ON screenshots(app, ts);
CREATE INDEX IF NOT EXISTS idx_screen_bookmarks_created ON screen_bookmarks(created_at);
CREATE TABLE IF NOT EXISTS coding_agent_bursts (id TEXT PRIMARY KEY, agent TEXT NOT NULL,
    start REAL NOT NULL, end REAL NOT NULL, payload TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_coding_agent_bursts_start ON coding_agent_bursts(start);
CREATE VIRTUAL TABLE IF NOT EXISTS ocr_fts USING fts5(
    text, window_title, ts UNINDEXED, app UNINDEXED,
    text_source UNINDEXED, snapshot_id UNINDEXED,
    tokenize='unicode61 remove_diacritics 2');
CREATE TABLE IF NOT EXISTS screen_embeddings (
    snapshot_id INTEGER PRIMARY KEY, ts REAL NOT NULL, app TEXT NOT NULL, text TEXT NOT NULL,
    vec BLOB NOT NULL, model_id TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(snapshot_id) REFERENCES screenshots(id) ON DELETE CASCADE);
CREATE INDEX IF NOT EXISTS idx_screen_embeddings_ts ON screen_embeddings(ts);
CREATE INDEX IF NOT EXISTS idx_screen_embeddings_app_ts ON screen_embeddings(app, ts);

CREATE TABLE IF NOT EXISTS pipeline_jobs (meeting_id TEXT PRIMARY KEY, transcribe INTEGER NOT NULL,
    transcription_completed INTEGER NOT NULL DEFAULT 0, summarize INTEGER NOT NULL,
    summary_follows_setting INTEGER NOT NULL DEFAULT 1, attempts INTEGER NOT NULL DEFAULT 0,
    enqueued_at REAL NOT NULL, last_error TEXT);
"#;

/// A cheap-to-clone handle on one library root.
#[derive(Clone)]
pub struct Library {
    inner: Arc<Inner>,
}

struct Inner {
    paths: LibraryPaths,
    db: Mutex<Connection>,
    read_only: bool,
}

/// The two audio tracks of a meeting, if present on disk.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeetingAudio {
    pub mic: Option<PathBuf>,
    pub system: Option<PathBuf>,
}

/// Result of resolving a CLI/MCP meeting reference.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum MeetingLookup {
    Found(Meeting),
    NotFound,
    Ambiguous(Vec<String>),
}

impl Library {
    /// Opens (creating if needed) the library and its SQLite schema.
    pub fn open(paths: LibraryPaths) -> Result<Self> {
        fs::create_dir_all(paths.root())
            .with_context(|| format!("creating {}", paths.root().display()))?;
        fs::create_dir_all(paths.meetings_dir())?;
        let db = Connection::open(paths.database()).context("opening lokalbotv3.sqlite")?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
        )?;
        db.execute_batch(SCHEMA).context("creating schema")?;
        // Older pipeline_jobs tables predate last_error.
        let _ = db.execute("ALTER TABLE pipeline_jobs ADD COLUMN last_error TEXT", []);
        crate::screen::migrate(&db).context("updating the screen context tables")?;
        migrate_ocr_metadata(&db).context("indexing screen text metadata")?;
        crate::search::migrate(&db).context("migrating the search index")?;
        Ok(Self {
            inner: Arc::new(Inner {
                paths,
                db: Mutex::new(db),
                read_only: false,
            }),
        })
    }

    /// Opens an existing library for reading only (CLI, MCP). Missing database
    /// files are tolerated: file-based reads still work.
    pub fn open_read_only(paths: LibraryPaths) -> Result<Self> {
        let db = if paths.database().is_file() {
            Connection::open_with_flags(
                paths.database(),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?
        } else {
            Connection::open_in_memory()?
        };
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        Ok(Self {
            inner: Arc::new(Inner {
                paths,
                db: Mutex::new(db),
                read_only: true,
            }),
        })
    }

    pub fn paths(&self) -> &LibraryPaths {
        &self.inner.paths
    }

    pub fn is_read_only(&self) -> bool {
        self.inner.read_only
    }

    /// Runs `f` with the shared SQLite connection.
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let db = self.inner.db.lock();
        f(&db)
    }

    // ---------------------------------------------------------------- meetings

    /// Every visible meeting (merged sources hidden), newest first.
    pub fn list_meetings(&self) -> Result<Vec<Meeting>> {
        let mut meetings = Vec::new();
        for meta in meeting_meta_files(&self.paths().meetings_dir()) {
            match read_json::<Meeting>(&meta) {
                Ok(Some(mut meeting)) => {
                    if let Some(folder) = meta.parent()
                        && let Ok(relative) = folder.strip_prefix(self.paths().root())
                    {
                        meeting.relative_path = relative.to_string_lossy().replace('\\', "/");
                    }
                    meetings.push(meeting);
                }
                Ok(None) => {}
                Err(err) => log::warn!("skipping unreadable {}: {err:#}", meta.display()),
            }
        }
        let parents: std::collections::HashMap<Uuid, Uuid> = meetings
            .iter()
            .flat_map(|m| {
                m.merged_source_meeting_ids
                    .iter()
                    .flatten()
                    .map(move |s| (*s, m.id))
            })
            .collect();
        meetings.retain(|m| !m.is_merged_source() && !parents.contains_key(&m.id));
        meetings.sort_by_key(|m| std::cmp::Reverse(m.started_at));
        Ok(meetings)
    }

    pub fn meeting(&self, id: Uuid) -> Result<Option<Meeting>> {
        Ok(self.list_all_meetings()?.into_iter().find(|m| m.id == id))
    }

    /// Including merged sources, unsorted.
    pub fn list_all_meetings(&self) -> Result<Vec<Meeting>> {
        let mut out = Vec::new();
        for meta in meeting_meta_files(&self.paths().meetings_dir()) {
            if let Ok(Some(mut meeting)) = read_json::<Meeting>(&meta) {
                if let Some(folder) = meta.parent()
                    && let Ok(relative) = folder.strip_prefix(self.paths().root())
                {
                    meeting.relative_path = relative.to_string_lossy().replace('\\', "/");
                }
                out.push(meeting);
            }
        }
        Ok(out)
    }

    /// `latest`, a full UUID, or a unique short-id prefix.
    pub fn find_meeting(&self, reference: &str) -> Result<MeetingLookup> {
        let reference = reference.trim().to_lowercase();
        let meetings = self.list_meetings()?;
        if reference == "latest" {
            return Ok(meetings
                .into_iter()
                .next()
                .map(MeetingLookup::Found)
                .unwrap_or(MeetingLookup::NotFound));
        }
        if let Ok(id) = Uuid::parse_str(&reference) {
            return Ok(meetings
                .into_iter()
                .find(|m| m.id == id)
                .map(MeetingLookup::Found)
                .unwrap_or(MeetingLookup::NotFound));
        }
        let matches: Vec<Meeting> = meetings
            .into_iter()
            .filter(|m| {
                m.id.simple()
                    .to_string()
                    .starts_with(&reference.replace('-', ""))
            })
            .collect();
        Ok(match matches.len() {
            0 => MeetingLookup::NotFound,
            1 => MeetingLookup::Found(matches.into_iter().next().unwrap()),
            _ => MeetingLookup::Ambiguous(matches.iter().map(|m| m.short_id()).collect()),
        })
    }

    /// Creates `meetings/YYYY/MM/DD-<slug>[-N]/meta.json` for a new recording.
    pub fn create_meeting(
        &self,
        title: &str,
        app_name: &str,
        started_at: DateTime<Utc>,
    ) -> Result<Meeting> {
        let local = started_at.with_timezone(&Local);
        let base = format!(
            "meetings/{:04}/{:02}/{:02}-{}",
            local.year(),
            local.month(),
            local.day(),
            ids::slugify(title)
        );
        let mut relative = base.clone();
        let mut n = 2;
        while self.paths().meeting_dir(&relative).exists() {
            relative = format!("{base}-{n}");
            n += 1;
        }
        let meeting = Meeting {
            id: Uuid::new_v4(),
            title: title.to_string(),
            app_name: app_name.to_string(),
            started_at,
            ended_at: None,
            relative_path: relative,
            has_system_track: false,
            calendar_provider: None,
            calendar_event_id: None,
            calendar_title: None,
            scheduled_start_at: None,
            scheduled_end_at: None,
            meeting_url: None,
            participant_name_hints: None,
            calendar_participant_identities: None,
            calendar_agenda: None,
            merged_source_meeting_ids: None,
            merged_into_meeting_id: None,
            recorded_duration: None,
            content_range: None,
            content_range_source: None,
            capture_intent: None,
        };
        fs::create_dir_all(self.meeting_dir(&meeting))?;
        self.save_meeting(&meeting)?;
        Ok(meeting)
    }

    pub fn save_meeting(&self, meeting: &Meeting) -> Result<()> {
        write_json(&self.meeting_dir(meeting).join(META_FILE), meeting)
    }

    pub fn meeting_dir(&self, meeting: &Meeting) -> PathBuf {
        self.paths().meeting_dir(&meeting.relative_path)
    }

    pub fn audio(&self, meeting: &Meeting) -> MeetingAudio {
        let dir = self.meeting_dir(meeting);
        let pick = |names: &[&str]| names.iter().map(|n| dir.join(n)).find(|p| p.is_file());
        MeetingAudio {
            mic: pick(&[MIC_TRACK, "mic.flac", "mic.m4a"]),
            system: pick(&[SYSTEM_TRACK, "system.flac", "system.m4a"]),
        }
    }

    /// Tombstones the meeting in the indexes, then removes its folder.
    pub fn delete_meeting(&self, meeting: &Meeting) -> Result<()> {
        if self.is_read_only() {
            bail!("the library is open read-only");
        }
        let id = ids::uuid_upper(&meeting.id);
        self.with_db(|db| {
            db.execute(
                "INSERT OR REPLACE INTO deleted_meetings(meeting_id, deleted_at) VALUES (?1, ?2)",
                rusqlite::params![id, crate::time::to_epoch(Utc::now())],
            )?;
            crate::search::delete_documents(db, &id)?;
            db.execute("DELETE FROM indexed_meetings WHERE meeting_id = ?1", [&id])?;
            db.execute("DELETE FROM embeddings WHERE meeting_id = ?1", [&id])?;
            db.execute("DELETE FROM embedded_meetings WHERE meeting_id = ?1", [&id])?;
            db.execute("DELETE FROM pipeline_jobs WHERE meeting_id = ?1", [&id])?;
            Ok(())
        })?;
        let dir = self.meeting_dir(meeting);
        if dir.starts_with(self.paths().meetings_dir()) && dir.is_dir() {
            fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        }
        Ok(())
    }

    // ------------------------------------------------------------- artifacts

    pub fn transcript(&self, meeting: &Meeting) -> Result<Option<Transcript>> {
        read_json(&self.meeting_dir(meeting).join(TRANSCRIPT_FILE))
    }

    /// Writes `transcript.json` and the matching `transcript.md`.
    pub fn save_transcript(&self, meeting: &Meeting, transcript: &Transcript) -> Result<()> {
        let dir = self.meeting_dir(meeting);
        write_json(&dir.join(TRANSCRIPT_FILE), transcript)?;
        write_atomic(
            &dir.join(TRANSCRIPT_MD_FILE),
            transcript.markdown().as_bytes(),
        )?;
        Ok(())
    }

    pub fn summary(&self, meeting: &Meeting) -> Result<Option<String>> {
        read_text(&self.meeting_dir(meeting).join(SUMMARY_FILE))
    }

    pub fn save_summary(&self, meeting: &Meeting, markdown: &str) -> Result<()> {
        write_atomic(
            &self.meeting_dir(meeting).join(SUMMARY_FILE),
            markdown.as_bytes(),
        )?;
        Ok(())
    }

    /// The user's own notes; empty when there are none.
    pub fn notes(&self, meeting: &Meeting) -> Result<String> {
        Ok(read_text(&self.meeting_dir(meeting).join(NOTES_FILE))?.unwrap_or_default())
    }

    /// Saves notes, deleting the file when they are blank.
    pub fn save_notes(&self, meeting: &Meeting, notes: &str) -> Result<()> {
        let path = self.meeting_dir(meeting).join(NOTES_FILE);
        if notes.trim().is_empty() {
            match fs::remove_file(&path) {
                Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err.into()),
                _ => return Ok(()),
            }
        }
        write_atomic(&path, notes.as_bytes())?;
        Ok(())
    }

    /// Extracted outcomes in display order (`MeetingOutcomes.load`).
    ///
    /// macOS treats outcomes whose `transcriptRevision` no longer matches the
    /// transcript's SHA-256 (over Swift's exact JSON bytes) as stale. This
    /// port cannot reproduce those bytes, so it treats outcomes as current.
    pub fn outcomes(&self, meeting: &Meeting) -> Result<Option<MeetingOutcomes>> {
        Ok(
            read_json::<MeetingOutcomes>(&self.meeting_dir(meeting).join(OUTCOMES_FILE))?
                .map(MeetingOutcomes::prioritized),
        )
    }

    pub fn save_outcomes(&self, meeting: &Meeting, outcomes: &MeetingOutcomes) -> Result<()> {
        write_json(&self.meeting_dir(meeting).join(OUTCOMES_FILE), outcomes)
    }

    pub fn outcome_state(&self, meeting: &Meeting) -> Result<OutcomeState> {
        Ok(read_json(&self.meeting_dir(meeting).join(OUTCOME_STATE_FILE))?.unwrap_or_default())
    }

    pub fn save_outcome_state(&self, meeting: &Meeting, state: &OutcomeState) -> Result<()> {
        write_json(&self.meeting_dir(meeting).join(OUTCOME_STATE_FILE), state)
    }

    /// The saved follow-up draft (`follow-up-draft.json`), if any.
    pub fn follow_up_draft(&self, meeting: &Meeting) -> Result<Option<FollowUpDraft>> {
        read_json(&self.meeting_dir(meeting).join(FOLLOW_UP_FILE))
    }

    /// Saves the draft the user reviewed, as macOS does (`saveFollowUp`).
    pub fn save_follow_up_draft(&self, meeting: &Meeting, draft: &FollowUpDraft) -> Result<()> {
        let mut draft = draft.clone();
        draft.updated_at = crate::time::now_millis();
        draft.seeded = false;
        draft.source_meeting_id = Some(meeting.id);
        write_json(&self.meeting_dir(meeting).join(FOLLOW_UP_FILE), &draft)
    }

    /// Marks one extracted action open, done or deferred.
    pub fn set_action_status(
        &self,
        meeting: &Meeting,
        action_id: &str,
        status: ActionStatus,
    ) -> Result<()> {
        let mut state = self.outcome_state(meeting)?;
        let entry = state
            .actions
            .entry(action_id.to_string())
            .or_insert_with(|| ActionState::new(status));
        entry.status = status;
        entry.updated_at = Utc::now();
        entry.user_edited = true;
        self.save_outcome_state(meeting, &state)
    }

    // ---------------------------------------------------------------- chats

    pub fn conversations(&self) -> Result<Vec<Conversation>> {
        let dir = self.paths().chats_dir();
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match read_json::<Conversation>(&path) {
                    Ok(Some(conversation)) => out.push(conversation),
                    Ok(None) => {}
                    Err(err) => log::warn!("skipping unreadable chat {}: {err:#}", path.display()),
                }
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
        Ok(out)
    }

    pub fn save_conversation(&self, conversation: &Conversation) -> Result<()> {
        let path = self
            .paths()
            .chats_dir()
            .join(format!("{}.json", conversation.id.hyphenated()));
        write_json(&path, conversation)
    }

    pub fn delete_conversation(&self, id: Uuid) -> Result<()> {
        let path = self
            .paths()
            .chats_dir()
            .join(format!("{}.json", id.hyphenated()));
        match fs::remove_file(path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        }
    }

    // --------------------------------------------------------------- digest

    pub fn journal_path(&self, day: chrono::NaiveDate) -> PathBuf {
        self.paths()
            .journal_dir()
            .join(format!("{}.md", day.format("%Y-%m-%d")))
    }

    pub fn journal_markdown(&self, day: chrono::NaiveDate) -> Result<Option<String>> {
        read_text(&self.journal_path(day))
    }

    pub fn digest_meta(&self, day: chrono::NaiveDate) -> Result<Option<DigestMeta>> {
        read_json(
            &self
                .paths()
                .journal_dir()
                .join(format!("{}.meta.json", day.format("%Y-%m-%d"))),
        )
    }

    /// Writes the journal and its sidecar (journal first, as on macOS). An
    /// empty `journal_digest` and a missing `journal_modified_at` are filled
    /// in from the written journal.
    pub fn save_digest(
        &self,
        day: chrono::NaiveDate,
        markdown: &str,
        meta: &DigestMeta,
    ) -> Result<()> {
        let journal = self.journal_path(day);
        write_atomic(&journal, markdown.as_bytes())?;
        let mut meta = meta.clone();
        if meta.journal_digest.is_empty() {
            meta.journal_digest = sha256_hex(markdown.as_bytes());
        }
        if meta.journal_modified_at.is_none() {
            meta.journal_modified_at = fs::metadata(&journal)
                .and_then(|m| m.modified())
                .ok()
                .map(DateTime::<Utc>::from);
        }
        write_json(
            &self
                .paths()
                .journal_dir()
                .join(format!("{}.meta.json", day.format("%Y-%m-%d"))),
            &meta,
        )
    }
}

// ------------------------------------------------------------------ helpers

/// SHA-256 hex, as `ContentFingerprint.digest`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(bytes))
}

/// Creates and backfills `ocr_metadata` once (`OCRMetadataIndex.migrate`).
fn migrate_ocr_metadata(db: &Connection) -> Result<()> {
    let exists = db
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE name = 'ocr_metadata'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if exists {
        return Ok(());
    }
    db.execute_batch(
        "BEGIN IMMEDIATE;
         CREATE TABLE ocr_metadata (rowid INTEGER PRIMARY KEY, snapshot_id INTEGER NOT NULL, ts REAL NOT NULL);
         CREATE INDEX idx_ocr_metadata_snapshot ON ocr_metadata(snapshot_id, rowid);
         CREATE INDEX idx_ocr_metadata_ts ON ocr_metadata(ts, snapshot_id);
         INSERT INTO ocr_metadata SELECT rowid, CAST(snapshot_id AS INTEGER), CAST(ts AS REAL) FROM ocr_fts;
         COMMIT;",
    )?;
    Ok(())
}

/// `meetings/YYYY/MM/<folder>/meta.json` paths.
fn meeting_meta_files(meetings_dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(years) = fs::read_dir(meetings_dir) else {
        return out;
    };
    for year in years.flatten().filter(|e| e.path().is_dir()) {
        let Ok(months) = fs::read_dir(year.path()) else {
            continue;
        };
        for month in months.flatten().filter(|e| e.path().is_dir()) {
            let Ok(folders) = fs::read_dir(month.path()) else {
                continue;
            };
            for folder in folders.flatten().filter(|e| e.path().is_dir()) {
                let meta = folder.path().join(META_FILE);
                if meta.is_file() {
                    out.push(meta);
                }
            }
        }
    }
    out
}

pub fn read_text(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("decoding {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| format!("reading {}", path.display())),
    }
}

/// Pretty-printed with sorted keys, like the macOS encoder.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let value = serde_json::to_value(value)?;
    let bytes = serde_json::to_vec_pretty(&sort_keys(value))?;
    write_atomic(path, &bytes)?;
    Ok(())
}

fn sort_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: std::collections::BTreeMap<String, serde_json::Value> =
                map.into_iter().map(|(k, v)| (k, sort_keys(v))).collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_keys).collect())
        }
        other => other,
    }
}

/// Write to a sibling temp file, flush, then rename over the target.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(".{file_name}.{}.tmp", Uuid::new_v4().simple()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../LokalBotTests/Fixtures/upgrade/0.10.9/library")
    }

    #[test]
    fn reads_the_macos_upgrade_fixture() {
        let root = fixture_root();
        if !root.is_dir() {
            eprintln!("fixture missing at {}", root.display());
            return;
        }
        let library = Library::open_read_only(LibraryPaths::new(root)).unwrap();
        let meetings = library.list_meetings().unwrap();
        assert!(meetings.len() >= 10, "found {} meetings", meetings.len());
        let northwind = meetings
            .iter()
            .find(|m| m.title == "Customer call - Northwind")
            .unwrap();
        let transcript = library.transcript(northwind).unwrap().unwrap();
        assert_eq!(transcript.segments.len(), 3);
        let outcomes = library.outcomes(northwind).unwrap().unwrap();
        assert_eq!(outcomes.action_items.len(), 2);
        assert!(
            library
                .summary(northwind)
                .unwrap()
                .unwrap()
                .contains("## TL;DR")
        );
        let chats = library.conversations().unwrap();
        assert_eq!(chats.len(), 2);
    }

    #[test]
    fn create_save_and_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::open(LibraryPaths::new(dir.path())).unwrap();
        let started = crate::time::now_seconds();
        let meeting = library
            .create_meeting("Design review", "Zoom", started)
            .unwrap();
        let second = library
            .create_meeting("Design review", "Zoom", started)
            .unwrap();
        assert!(second.relative_path.ends_with("-2"));
        library
            .save_notes(&meeting, "Remember the caching layer")
            .unwrap();
        assert_eq!(
            library.notes(&meeting).unwrap(),
            "Remember the caching layer"
        );
        library.save_notes(&meeting, "   ").unwrap();
        assert_eq!(library.notes(&meeting).unwrap(), "");
        let meta = fs::read_to_string(library.meeting_dir(&meeting).join(META_FILE)).unwrap();
        assert!(meta.contains(&ids::uuid_upper(&meeting.id)));
        assert_eq!(library.list_meetings().unwrap().len(), 2);
        library.delete_meeting(&meeting).unwrap();
        assert_eq!(library.list_meetings().unwrap().len(), 1);
        assert!(
            matches!(library.find_meeting("latest").unwrap(), MeetingLookup::Found(m) if m.id == second.id)
        );
    }
}
