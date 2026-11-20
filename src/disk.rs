use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::format::{FormatError, accept_apkg};
use crate::merge::{ApplyReport, Library, apply_apkg, apply_text};
use crate::model::{Choice, ChoiceId, Note, NoteBody};

const ROOT_JSON: &str = r#"{"start":540,"end":1080,"gap":45}"#;

const MEMORY_SCHEMA: &str = "
CREATE TABLE memory (
  guid TEXT NOT NULL,
  ord INTEGER NOT NULL,
  stability REAL NOT NULL,
  difficulty REAL NOT NULL,
  stability_fast REAL NOT NULL,
  due_unix INTEGER NOT NULL,
  PRIMARY KEY (guid, ord)
);
";

#[derive(Clone, Copy)]
struct Kind {
    id: i64,
    name: &'static str,
    fields: &'static [&'static str],
}

const KINDS: [Kind; 4] = [
    Kind {
        id: 1,
        name: "Basic",
        fields: &["Front", "Back"],
    },
    Kind {
        id: 2,
        name: "Basic (and reversed card)",
        fields: &["Front", "Back"],
    },
    Kind {
        id: 3,
        name: "Cloze",
        fields: &["Text"],
    },
    Kind {
        id: 4,
        name: "Choice",
        fields: &["Prompt", "Choices", "Correct", "Explain"],
    },
];

#[derive(Debug)]
pub enum DiskError {
    RootId,
    RootHeader,
    Io(std::io::Error),
    Format(FormatError),
    Sqlite(rusqlite::Error),
    Zip(String),
    Json(String),
}

impl std::fmt::Display for DiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiskError::RootId => write!(f, "bad root id"),
            DiskError::RootHeader => write!(f, "text file names a root"),
            DiskError::Io(err) => write!(f, "io: {err}"),
            DiskError::Format(err) => write!(f, "{err}"),
            DiskError::Sqlite(err) => write!(f, "sqlite: {err}"),
            DiskError::Zip(message) => write!(f, "zip: {message}"),
            DiskError::Json(message) => write!(f, "json: {message}"),
        }
    }
}

impl std::error::Error for DiskError {}

pub struct Root {
    dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemoryRow {
    pub guid: String,
    pub ord: i64,
    pub stability: f32,
    pub difficulty: f32,
    pub stability_fast: f32,
    pub due_unix: i64,
}

pub fn open_root(data_dir: &Path, root_id: &str) -> Result<Root, DiskError> {
    if root_id.is_empty() || root_id.contains('/') || root_id.contains("..") {
        return Err(DiskError::RootId);
    }
    let dir = data_dir.join("roots").join(root_id);
    fs::create_dir_all(&dir).map_err(DiskError::Io)?;
    let marker = dir.join("root.json");
    if !marker.exists() {
        fs::write(&marker, ROOT_JSON).map_err(DiskError::Io)?;
    }
    Ok(Root { dir })
}

pub fn import_text(root: &Root, text: &str) -> Result<ApplyReport, DiskError> {
    if names_root(text) {
        return Err(DiskError::RootHeader);
    }
    commit(root, |library| apply_text(library, text))
}

pub fn import_apkg(root: &Root, bytes: &[u8]) -> Result<ApplyReport, DiskError> {
    commit(root, |library| apply_apkg(library, bytes))
}

pub fn export_text(root: &Root) -> Result<String, DiskError> {
    Ok(render_text(&load_notes(&root.dir)?))
}

pub fn memory_rows(root: &Root) -> Result<Vec<MemoryRow>, DiskError> {
    read_memory(&root.dir)
}

pub fn map_notes(root: &Root, mut change: impl FnMut(&mut Note)) -> Result<(), DiskError> {
    let mut notes = load_notes(&root.dir)?;
    for note in &mut notes {
        change(note);
    }
    store_notes(&root.dir, &notes)
}

pub fn retain_notes(root: &Root, mut keep: impl FnMut(&Note) -> bool) -> Result<(), DiskError> {
    let mut notes = load_notes(&root.dir)?;
    notes.retain(|note| keep(note));
    store_notes(&root.dir, &notes)
}

pub fn write_memory_row(root: &Root, row: &MemoryRow) -> Result<(), DiskError> {
    let mut rows = read_memory(&root.dir)?;
    if let Some(existing) = rows
        .iter_mut()
        .find(|stored| stored.guid == row.guid && stored.ord == row.ord)
    {
        *existing = row.clone();
    } else {
        rows.push(row.clone());
    }
    atomic_write(&root.dir, "memory.sqlite", &memory_sqlite(&rows)?)
}

pub fn export_apkg(root: &Root) -> Result<Vec<u8>, DiskError> {
    let path = root.dir.join("collection.anki21");
    let sqlite = if path.is_file() {
        fs::read(&path).map_err(DiskError::Io)?
    } else {
        notes_sqlite(&[])?
    };
    zip_sqlite(&sqlite)
}

fn names_root(text: &str) -> bool {
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with('#') {
            break;
        }
        if line.contains("#root:") {
            return true;
        }
    }
    false
}

fn commit(
    root: &Root,
    apply: impl FnOnce(&mut Library) -> Result<ApplyReport, FormatError>,
) -> Result<ApplyReport, DiskError> {
    let mut library = load_library(&root.dir)?;
    let report = apply(&mut library).map_err(DiskError::Format)?;
    store_notes(&root.dir, library.notes())?;
    ensure_decks(&root.dir, library.notes())?;
    ensure_memory(&root.dir)?;
    Ok(report)
}

fn load_library(dir: &Path) -> Result<Library, DiskError> {
    let mut library = Library::new();
    for note in load_notes(dir)? {
        library.insert(note, None);
    }
    Ok(library)
}

fn load_notes(dir: &Path) -> Result<Vec<Note>, DiskError> {
    let path = dir.join("collection.anki21");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let sqlite = fs::read(&path).map_err(DiskError::Io)?;
    let import = accept_apkg(&zip_sqlite(&sqlite)?).map_err(DiskError::Format)?;
    Ok(import.notes)
}

fn store_notes(dir: &Path, notes: &[Note]) -> Result<(), DiskError> {
    atomic_write(dir, "collection.anki21", &notes_sqlite(notes)?)
}

fn ensure_memory(dir: &Path) -> Result<(), DiskError> {
    if dir.join("memory.sqlite").is_file() {
        return Ok(());
    }
    atomic_write(dir, "memory.sqlite", &memory_sqlite(&[])?)
}

fn atomic_write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), DiskError> {
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, bytes).map_err(DiskError::Io)?;
    fs::rename(&tmp, dir.join(name)).map_err(DiskError::Io)?;
    Ok(())
}

fn zip_sqlite(sqlite: &[u8]) -> Result<Vec<u8>, DiskError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("collection.anki21", SimpleFileOptions::default())
        .map_err(|err| DiskError::Zip(err.to_string()))?;
    writer.write_all(sqlite).map_err(DiskError::Io)?;
    Ok(writer
        .finish()
        .map_err(|err| DiskError::Zip(err.to_string()))?
        .into_inner())
}

fn kind_of(body: &NoteBody) -> Kind {
    match body {
        NoteBody::Basic { .. } => KINDS[0],
        NoteBody::Reversed { .. } => KINDS[1],
        NoteBody::Cloze { .. } => KINDS[2],
        NoteBody::Choice { .. } => KINDS[3],
    }
}

fn notes_sqlite(notes: &[Note]) -> Result<Vec<u8>, DiskError> {
    let conn = Connection::open_in_memory().map_err(DiskError::Sqlite)?;
    conn.execute_batch(
        "
        CREATE TABLE col (
            id INTEGER PRIMARY KEY,
            models TEXT NOT NULL,
            decks TEXT NOT NULL
        );
        CREATE TABLE notes (
            id INTEGER PRIMARY KEY,
            guid TEXT NOT NULL,
            mid INTEGER NOT NULL,
            flds TEXT NOT NULL
        );
        CREATE TABLE cards (
            id INTEGER PRIMARY KEY,
            nid INTEGER NOT NULL,
            did INTEGER NOT NULL,
            ord INTEGER NOT NULL
        );
        ",
    )
    .map_err(DiskError::Sqlite)?;

    let mut ordered = notes.to_vec();
    ordered.sort_by(|left, right| left.guid.as_str().cmp(right.guid.as_str()));
    let deck_names = sorted_decks(&ordered);
    conn.execute(
        "INSERT INTO col (id, models, decks) VALUES (1, ?1, ?2)",
        params![models_json(&ordered), decks_object(&deck_names)],
    )
    .map_err(DiskError::Sqlite)?;

    let mut card_id = 1i64;
    for (index, note) in ordered.iter().enumerate() {
        let nid = (index as i64) + 1;
        let did = deck_id(&deck_names, note.deck.as_str());
        conn.execute(
            "INSERT INTO notes (id, guid, mid, flds) VALUES (?1, ?2, ?3, ?4)",
            params![
                nid,
                note.guid.as_str(),
                kind_of(&note.body).id,
                flds_of(&note.body)
            ],
        )
        .map_err(DiskError::Sqlite)?;
        let cards = i64::from(note.body.study_cards());
        for ord in 0..cards {
            conn.execute(
                "INSERT INTO cards (id, nid, did, ord) VALUES (?1, ?2, ?3, ?4)",
                params![card_id, nid, did, ord],
            )
            .map_err(DiskError::Sqlite)?;
            card_id += 1;
        }
    }

    let data = conn
        .serialize(rusqlite::MAIN_DB)
        .map_err(DiskError::Sqlite)?;
    Ok(data.to_vec())
}

fn sorted_decks(notes: &[Note]) -> Vec<String> {
    let mut names: Vec<String> = notes
        .iter()
        .map(|note| note.deck.as_str().to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn deck_id(names: &[String], deck: &str) -> i64 {
    names
        .iter()
        .position(|name| name == deck)
        .map(|index| (index as i64) + 1)
        .unwrap_or(1)
}

fn models_json(notes: &[Note]) -> String {
    let mut kinds = Vec::new();
    for note in notes {
        let kind = kind_of(&note.body);
        if kinds.iter().any(|stored: &Kind| stored.id == kind.id) {
            continue;
        }
        kinds.push(kind);
    }
    kinds.sort_by_key(|kind| kind.id);
    let parts: Vec<String> = kinds
        .iter()
        .map(|kind| {
            format!(
                r#""{}":{{"name":"{}","flds":{}}}"#,
                kind.id,
                kind.name,
                fields_json(kind.fields)
            )
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn fields_json(fields: &[&str]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .enumerate()
        .map(|(ord, name)| format!(r#"{{"name":"{name}","ord":{ord}}}"#))
        .collect();
    format!("[{}]", parts.join(","))
}

fn decks_object(names: &[String]) -> String {
    let parts: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(index, name)| format!(r#""{}":{{"name":"{}"}}"#, index + 1, json_escape(name)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn flds_of(body: &NoteBody) -> String {
    match body {
        NoteBody::Basic { front, back } | NoteBody::Reversed { front, back } => {
            format!("{front}\u{1f}{back}")
        }
        NoteBody::Cloze { text } => text.clone(),
        NoteBody::Choice {
            prompt,
            choices,
            correct,
            explain,
        } => format!(
            "{prompt}\u{1f}{}\u{1f}{}\u{1f}{explain}",
            choices_field(choices),
            correct_field(correct)
        ),
    }
}

fn choices_field(choices: &[Choice]) -> String {
    choices
        .iter()
        .map(|choice| format!("{}|{}", choice.id.as_str(), choice.text))
        .collect::<Vec<_>>()
        .join(";")
}

fn correct_field(correct: &[ChoiceId]) -> String {
    correct
        .iter()
        .map(ChoiceId::as_str)
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_memory(dir: &Path) -> Result<Vec<MemoryRow>, DiskError> {
    let path = dir.join("memory.sqlite");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(DiskError::Io)?;
    let mut conn = Connection::open_in_memory().map_err(DiskError::Sqlite)?;
    conn.deserialize_read_exact(
        rusqlite::MAIN_DB,
        Cursor::new(bytes.as_slice()),
        bytes.len(),
        true,
    )
    .map_err(DiskError::Sqlite)?;
    let mut stmt = conn
        .prepare(
            "SELECT guid, ord, stability, difficulty, stability_fast, due_unix
             FROM memory ORDER BY guid, ord",
        )
        .map_err(DiskError::Sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(MemoryRow {
                guid: row.get(0)?,
                ord: row.get(1)?,
                stability: row.get(2)?,
                difficulty: row.get(3)?,
                stability_fast: row.get(4)?,
                due_unix: row.get(5)?,
            })
        })
        .map_err(DiskError::Sqlite)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DiskError::Sqlite)
}

fn memory_sqlite(rows: &[MemoryRow]) -> Result<Vec<u8>, DiskError> {
    let conn = Connection::open_in_memory().map_err(DiskError::Sqlite)?;
    conn.execute_batch(MEMORY_SCHEMA)
        .map_err(DiskError::Sqlite)?;
    for row in rows {
        conn.execute(
            "INSERT INTO memory (guid, ord, stability, difficulty, stability_fast, due_unix)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.guid,
                row.ord,
                row.stability,
                row.difficulty,
                row.stability_fast,
                row.due_unix
            ],
        )
        .map_err(DiskError::Sqlite)?;
    }
    let data = conn
        .serialize(rusqlite::MAIN_DB)
        .map_err(DiskError::Sqlite)?;
    Ok(data.to_vec())
}

struct DeckFile {
    name: String,
    delivery: StoredDelivery,
}

enum StoredDelivery {
    Inherit,
    Override { start: i64, end: i64, gap: i64 },
}

fn ensure_decks(dir: &Path, notes: &[Note]) -> Result<(), DiskError> {
    let path = dir.join("decks.json");
    let mut decks = if path.is_file() {
        parse_decks(&fs::read_to_string(&path).map_err(DiskError::Io)?)?
    } else {
        Vec::new()
    };
    let mut added = false;
    for name in sorted_decks(notes) {
        if decks.iter().any(|deck| deck.name == name) {
            continue;
        }
        decks.push(DeckFile {
            name,
            delivery: StoredDelivery::Inherit,
        });
        added = true;
    }
    if path.is_file() && !added {
        return Ok(());
    }
    fs::write(&path, render_decks(&decks)).map_err(DiskError::Io)?;
    Ok(())
}

fn parse_decks(text: &str) -> Result<Vec<DeckFile>, DiskError> {
    let conn = Connection::open_in_memory().map_err(DiskError::Sqlite)?;
    let mut stmt = conn
        .prepare(
            "SELECT
                json_extract(value, '$.name'),
                json_type(value, '$.delivery'),
                json_extract(value, '$.delivery'),
                json_extract(value, '$.delivery.start'),
                json_extract(value, '$.delivery.end'),
                json_extract(value, '$.delivery.gap')
             FROM json_each(?1)",
        )
        .map_err(DiskError::Sqlite)?;
    let rows = stmt
        .query_map(params![text], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        })
        .map_err(DiskError::Sqlite)?;
    let mut decks = Vec::new();
    for row in rows {
        let (name, kind, raw, start, end, gap) = row.map_err(DiskError::Sqlite)?;
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Err(DiskError::Json("missing deck name".to_string()));
        };
        let delivery = match (kind.as_deref(), raw.as_deref(), start, end, gap) {
            (Some("text"), Some("inherit"), _, _, _) => StoredDelivery::Inherit,
            (Some("object"), _, Some(start), Some(end), Some(gap)) => {
                StoredDelivery::Override { start, end, gap }
            }
            _ => return Err(DiskError::Json("bad deck delivery".to_string())),
        };
        decks.push(DeckFile { name, delivery });
    }
    Ok(decks)
}

fn render_decks(decks: &[DeckFile]) -> String {
    let parts: Vec<String> = decks
        .iter()
        .map(|deck| {
            let delivery = match deck.delivery {
                StoredDelivery::Inherit => r#""inherit""#.to_string(),
                StoredDelivery::Override { start, end, gap } => {
                    format!(r#"{{"start":{start},"end":{end},"gap":{gap}}}"#)
                }
            };
            format!(
                r#"{{"name":"{}","delivery":{delivery}}}"#,
                json_escape(&deck.name)
            )
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", u32::from(ch))),
            ch => out.push(ch),
        }
    }
    out
}

fn render_text(notes: &[Note]) -> String {
    let mut notes = notes.to_vec();
    notes.sort_by(|left, right| left.guid.as_str().cmp(right.guid.as_str()));
    let uniform_type = notes.first().and_then(|note| {
        let name = kind_of(&note.body).name;
        notes
            .iter()
            .all(|other| kind_of(&other.body).name == name)
            .then_some(name)
    });
    let uniform_deck = notes.first().and_then(|note| {
        let deck = note.deck.as_str();
        notes
            .iter()
            .all(|other| other.deck.as_str() == deck)
            .then_some(deck)
    });
    let mut columns = if let Some(name) = uniform_type {
        KINDS
            .iter()
            .find(|kind| kind.name == name)
            .map(|kind| kind.fields.to_vec())
            .unwrap_or_default()
    } else {
        all_fields()
    };
    let notetype_column = if uniform_type.is_none() {
        columns.push("notetype");
        Some(columns.len())
    } else {
        None
    };
    let deck_column = if uniform_deck.is_none() {
        columns.push("deck");
        Some(columns.len())
    } else {
        None
    };
    columns.push("guid");
    let guid_column = columns.len();

    let mut lines = vec!["#separator:Tab".to_string(), "#html:false".to_string()];
    if let Some(name) = uniform_type {
        lines.push(format!("#notetype:{name}"));
    }
    if let Some(deck) = uniform_deck {
        lines.push(format!("#deck:{deck}"));
    }
    lines.push(format!("#columns:{}", columns.join("\t")));
    if let Some(index) = notetype_column {
        lines.push(format!("#notetype column:{index}"));
    }
    if let Some(index) = deck_column {
        lines.push(format!("#deck column:{index}"));
    }
    lines.push(format!("#guid column:{guid_column}"));
    for note in &notes {
        lines.push(note_cells(note, &columns).join("\t"));
    }
    format!("{}\n", lines.join("\n"))
}

fn all_fields() -> Vec<&'static str> {
    let mut fields = Vec::new();
    for kind in KINDS {
        for field in kind.fields {
            if !fields.contains(field) {
                fields.push(field);
            }
        }
    }
    fields
}

fn note_cells(note: &Note, columns: &[&str]) -> Vec<String> {
    columns
        .iter()
        .map(|column| match *column {
            "guid" => note.guid.as_str().to_string(),
            "deck" => note.deck.as_str().to_string(),
            "notetype" => kind_of(&note.body).name.to_string(),
            name => field_value(&note.body, name),
        })
        .collect()
}

fn field_value(body: &NoteBody, name: &str) -> String {
    match (body, name) {
        (NoteBody::Basic { front, .. } | NoteBody::Reversed { front, .. }, "Front") => {
            front.clone()
        }
        (NoteBody::Basic { back, .. } | NoteBody::Reversed { back, .. }, "Back") => back.clone(),
        (NoteBody::Cloze { text }, "Text") => text.clone(),
        (NoteBody::Choice { prompt, .. }, "Prompt") => prompt.clone(),
        (NoteBody::Choice { choices, .. }, "Choices") => choices_field(choices),
        (NoteBody::Choice { correct, .. }, "Correct") => correct_field(correct),
        (NoteBody::Choice { explain, .. }, "Explain") => explain.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::NoteBody;
    use crate::accept_apkg;
    use crate::accept_text;

    #[test]
    fn roundtrip_basic() {
        let dir = temp_dir();
        let root = super::open_root(&dir, "basic").unwrap();
        super::import_text(&root, include_str!("../fixtures/basic.txt")).unwrap();

        let exported = accept_text(&super::export_text(&root).unwrap()).unwrap();
        let fixture = accept_text(include_str!("../fixtures/basic.txt")).unwrap();
        assert_eq!(exported.notes, fixture.notes);
        let note = &exported.notes[0];
        assert_eq!(note.guid.as_str(), "abc123XYZ0");
        assert_eq!(note.deck.as_str(), "Spanish::Verbs");
        match &note.body {
            NoteBody::Basic { front, back } => {
                assert_eq!(front, "hablar");
                assert_eq!(back, "to speak");
            }
            other => panic!("{other:?}"),
        }

        let bytes = super::export_apkg(&root).unwrap();
        let from_apkg = accept_apkg(&bytes).unwrap();
        assert_eq!(from_apkg.notes, fixture.notes);
        assert_eq!(zip_names(&bytes), vec!["collection.anki21".to_string()]);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn second_import_keeps_memory_and_ignores_tmp() {
        let dir = temp_dir();
        let root = super::open_root(&dir, "basic").unwrap();
        let text = include_str!("../fixtures/basic.txt");
        super::import_text(&root, text).unwrap();
        super::write_memory_row(
            &root,
            &super::MemoryRow {
                guid: "abc123XYZ0".to_string(),
                ord: 0,
                stability: 1.0,
                difficulty: 1.0,
                stability_fast: 1.0,
                due_unix: 99,
            },
        )
        .unwrap();
        super::import_text(&root, text).unwrap();
        let rows = super::memory_rows(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].guid, "abc123XYZ0");
        assert_eq!(rows[0].ord, 0);
        assert_eq!(rows[0].stability, 1.0);
        assert_eq!(rows[0].difficulty, 1.0);
        assert_eq!(rows[0].stability_fast, 1.0);
        assert_eq!(rows[0].due_unix, 99);

        fs::write(root.dir.join("collection.anki21.tmp"), b"garbage").unwrap();
        super::import_text(&root, text).unwrap();
        let exported = accept_text(&super::export_text(&root).unwrap()).unwrap();
        match &exported.notes[0].body {
            NoteBody::Basic { front, back } => {
                assert_eq!(front, "hablar");
                assert_eq!(back, "to speak");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(super::memory_rows(&root).unwrap()[0].due_unix, 99);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn root_header_does_not_write_collection() {
        let dir = temp_dir();
        let root = super::open_root(&dir, "basic").unwrap();
        let text = "\
#root:somewhere
#separator:Tab
#html:false
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
hablar\tto speak\tabc123XYZ0
";
        match super::import_text(&root, text) {
            Err(super::DiskError::RootHeader) => {}
            Err(err) => panic!("{err}"),
            Ok(_) => panic!("imported a file that names a root"),
        }
        assert!(!root.dir.join("collection.anki21").exists());

        let _ = fs::remove_dir_all(dir);
    }

    fn temp_dir() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("retention-disk-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_string())
            .collect()
    }
}
