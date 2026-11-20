use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use rusqlite::Connection;
use zip::ZipArchive;

use crate::model::{Choice, ChoiceId, DeckName, Guid, Import, Note, NoteBody};

/// Failure while reading a text note file or an `.apkg`. Callers match on the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    EmptyCorrect { guid: Guid },
    UnknownChoiceId { guid: Guid, id: ChoiceId },
    EmptyGuid,
    MissingField { guid: Guid, field: &'static str },
    MissingDeck { guid: Guid },
    MissingNoteType { guid: Guid },
    BadChoices { guid: Guid },
    BadSeparator { value: String },
    BadColumn { name: &'static str },
    Utf8,
    NoCollection,
    Zip { message: String },
    Zstd { message: String },
    Sqlite { message: String },
    Io { message: String },
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::EmptyCorrect { guid } => write!(f, "empty Correct on {guid}"),
            FormatError::UnknownChoiceId { guid, id } => {
                write!(f, "unknown choice id {id} on {guid}")
            }
            FormatError::EmptyGuid => write!(f, "empty guid"),
            FormatError::MissingField { guid, field } => {
                write!(f, "missing {field} on {guid}")
            }
            FormatError::MissingDeck { guid } => write!(f, "missing deck on {guid}"),
            FormatError::MissingNoteType { guid } => write!(f, "missing note type on {guid}"),
            FormatError::BadChoices { guid } => write!(f, "bad Choices on {guid}"),
            FormatError::BadSeparator { value } => write!(f, "bad separator {value}"),
            FormatError::BadColumn { name } => write!(f, "bad {name} column"),
            FormatError::Utf8 => write!(f, "file is not utf-8"),
            FormatError::NoCollection => write!(f, "apkg has no collection"),
            FormatError::Zip { message } => write!(f, "zip: {message}"),
            FormatError::Zstd { message } => write!(f, "zstd: {message}"),
            FormatError::Sqlite { message } => write!(f, "sqlite: {message}"),
            FormatError::Io { message } => write!(f, "io: {message}"),
        }
    }
}

impl std::error::Error for FormatError {}

#[derive(Clone, Copy)]
enum Kind {
    Basic,
    Reversed,
    Cloze,
    Choice,
}

fn kind_of(name: &str) -> Option<Kind> {
    match name {
        "Basic" => Some(Kind::Basic),
        "Basic (and reversed card)" => Some(Kind::Reversed),
        "Cloze" => Some(Kind::Cloze),
        "Choice" => Some(Kind::Choice),
        _ => None,
    }
}

struct Headers {
    separator: char,
    notetype: Option<String>,
    deck: Option<String>,
    guid_column: Option<usize>,
    notetype_column: Option<usize>,
    deck_column: Option<usize>,
    columns: Option<Vec<String>>,
}

pub fn accept_text(text: &str) -> Result<Import, FormatError> {
    let (headers, rows) = split_document(text)?;
    let mut notes = Vec::new();
    let mut skipped = 0u32;
    for line in rows {
        let values = split_row(line, headers.separator);
        let guid_raw = cell(&values, headers.guid_column).trim();
        let Some(guid) = Guid::new(guid_raw) else {
            return Err(FormatError::EmptyGuid);
        };
        let notetype = resolved(&headers.notetype, headers.notetype_column, &values);
        if notetype.is_empty() {
            return Err(FormatError::MissingNoteType { guid });
        }
        let Some(kind) = kind_of(&notetype) else {
            skipped += 1;
            continue;
        };
        let deck_raw = resolved(&headers.deck, headers.deck_column, &values);
        let Some(deck) = DeckName::new(deck_raw) else {
            return Err(FormatError::MissingDeck { guid });
        };
        let fields = text_fields(&headers, kind, &values);
        let body = body_from_fields(&guid, kind, &fields)?;
        notes.push(Note { guid, deck, body });
    }
    Ok(Import {
        notes,
        skipped,
        scheduling_rows: 0,
    })
}

pub fn accept_apkg(bytes: &[u8]) -> Result<Import, FormatError> {
    let sqlite = collection_sqlite(bytes)?;
    read_sqlite(&sqlite)
}

pub fn accept_path(path: &Path) -> Result<Import, FormatError> {
    let bytes = fs::read(path).map_err(|err| FormatError::Io {
        message: err.to_string(),
    })?;
    let is_apkg = path.extension().and_then(|ext| ext.to_str()) == Some("apkg");
    if is_apkg {
        accept_apkg(&bytes)
    } else {
        let text = std::str::from_utf8(&bytes).map_err(|_| FormatError::Utf8)?;
        accept_text(text)
    }
}

fn split_document(text: &str) -> Result<(Headers, Vec<&str>), FormatError> {
    let mut header_lines = Vec::new();
    let mut rows = Vec::new();
    let mut seen_row = false;
    for line in text.lines() {
        if !seen_row && line.starts_with('#') {
            header_lines.push(line);
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        seen_row = true;
        rows.push(line);
    }
    Ok((parse_headers(&header_lines)?, rows))
}

fn header_pair(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix('#')?;
    let (key, value) = rest.split_once(':')?;
    Some((key.trim(), value.trim()))
}

fn parse_headers(lines: &[&str]) -> Result<Headers, FormatError> {
    let mut separator = '\t';
    for line in lines {
        let Some((key, value)) = header_pair(line) else {
            continue;
        };
        if key == "separator" {
            separator = match value {
                "Tab" => '\t',
                "Comma" => ',',
                other => {
                    return Err(FormatError::BadSeparator {
                        value: other.to_string(),
                    });
                }
            };
        }
    }
    let mut headers = Headers {
        separator,
        notetype: None,
        deck: None,
        guid_column: None,
        notetype_column: None,
        deck_column: None,
        columns: None,
    };
    for line in lines {
        let Some((key, value)) = header_pair(line) else {
            continue;
        };
        match key {
            "separator" | "html" => {}
            "notetype" => headers.notetype = Some(value.to_string()),
            "deck" => headers.deck = Some(value.to_string()),
            "guid column" => headers.guid_column = Some(column_index(value, "guid")?),
            "notetype column" => headers.notetype_column = Some(column_index(value, "notetype")?),
            "deck column" => headers.deck_column = Some(column_index(value, "deck")?),
            "columns" => {
                if !value.is_empty() {
                    headers.columns = Some(
                        value
                            .split(separator)
                            .map(|part| part.to_string())
                            .collect(),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(headers)
}

fn column_index(value: &str, name: &'static str) -> Result<usize, FormatError> {
    let parsed: usize = value.parse().map_err(|_| FormatError::BadColumn { name })?;
    if parsed == 0 {
        return Err(FormatError::BadColumn { name });
    }
    Ok(parsed - 1)
}

fn field_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn split_row(line: &str, separator: char) -> Vec<&str> {
    line.split(separator).collect()
}

fn cell<'a>(values: &[&'a str], index: Option<usize>) -> &'a str {
    index.and_then(|i| values.get(i)).copied().unwrap_or("")
}

/// A non-empty column cell overrides the header. An empty cell keeps the header.
fn resolved(header: &Option<String>, column: Option<usize>, values: &[&str]) -> String {
    if let Some(index) = column {
        let value = cell(values, Some(index)).trim();
        if !value.is_empty() {
            return value.to_string();
        }
    }
    header.clone().unwrap_or_default()
}

fn text_fields(headers: &Headers, kind: Kind, values: &[&str]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some(columns) = &headers.columns {
        for (index, name) in columns.iter().enumerate() {
            if map.contains_key(name) {
                continue;
            }
            let value = values.get(index).copied().unwrap_or("");
            map.insert(name.clone(), field_text(value));
        }
        return map;
    }
    if matches!(kind, Kind::Basic | Kind::Reversed) {
        let mut kept = Vec::new();
        for (index, value) in values.iter().enumerate() {
            if headers.guid_column == Some(index) {
                continue;
            }
            kept.push(*value);
        }
        if let Some(front) = kept.first() {
            map.insert("Front".to_string(), field_text(front));
        }
        if let Some(back) = kept.get(1) {
            map.insert("Back".to_string(), field_text(back));
        }
    }
    map
}

fn body_from_fields(
    guid: &Guid,
    kind: Kind,
    fields: &HashMap<String, String>,
) -> Result<NoteBody, FormatError> {
    match kind {
        Kind::Basic => Ok(NoteBody::Basic {
            front: require(fields, "Front", guid)?,
            back: require(fields, "Back", guid)?,
        }),
        Kind::Reversed => Ok(NoteBody::Reversed {
            front: require(fields, "Front", guid)?,
            back: require(fields, "Back", guid)?,
        }),
        Kind::Cloze => Ok(NoteBody::Cloze {
            text: require(fields, "Text", guid)?,
        }),
        Kind::Choice => {
            let prompt = require(fields, "Prompt", guid)?;
            let choices_raw = require(fields, "Choices", guid)?;
            let correct_raw = require(fields, "Correct", guid)?;
            let explain = require(fields, "Explain", guid)?;
            let choices = parse_choices(guid, &choices_raw)?;
            let correct = parse_correct(guid, &correct_raw, &choices)?;
            Ok(NoteBody::Choice {
                prompt,
                choices,
                correct,
                explain,
            })
        }
    }
}

fn require(
    fields: &HashMap<String, String>,
    name: &'static str,
    guid: &Guid,
) -> Result<String, FormatError> {
    fields
        .get(name)
        .cloned()
        .ok_or_else(|| FormatError::MissingField {
            guid: guid.clone(),
            field: name,
        })
}

fn parse_choices(guid: &Guid, raw: &str) -> Result<Vec<Choice>, FormatError> {
    let mut choices = Vec::new();
    if raw.is_empty() {
        return Ok(choices);
    }
    for item in raw.split(';') {
        if item.is_empty() {
            continue;
        }
        let Some((id, text)) = item.split_once('|') else {
            return Err(FormatError::BadChoices { guid: guid.clone() });
        };
        let Some(id) = ChoiceId::new(id.trim()) else {
            return Err(FormatError::BadChoices { guid: guid.clone() });
        };
        choices.push(Choice {
            id,
            text: text.to_string(),
        });
    }
    Ok(choices)
}

fn parse_correct(guid: &Guid, raw: &str, choices: &[Choice]) -> Result<Vec<ChoiceId>, FormatError> {
    let mut correct = Vec::new();
    for token in raw.split_whitespace() {
        if correct.iter().any(|id: &ChoiceId| id.as_str() == token) {
            continue;
        }
        let Some(id) = ChoiceId::new(token) else {
            return Err(FormatError::EmptyCorrect { guid: guid.clone() });
        };
        if !choices.iter().any(|choice| choice.id.as_str() == token) {
            return Err(FormatError::UnknownChoiceId {
                guid: guid.clone(),
                id,
            });
        }
        correct.push(id);
    }
    if correct.is_empty() {
        return Err(FormatError::EmptyCorrect { guid: guid.clone() });
    }
    Ok(correct)
}

const COLLECTIONS: [&str; 3] = [
    "collection.anki21b",
    "collection.anki21",
    "collection.anki2",
];

fn collection_sqlite(bytes: &[u8]) -> Result<Vec<u8>, FormatError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|err| FormatError::Zip {
        message: err.to_string(),
    })?;
    for name in COLLECTIONS {
        let Ok(mut file) = archive.by_name(name) else {
            continue;
        };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(|err| FormatError::Io {
            message: err.to_string(),
        })?;
        if name == "collection.anki21b" {
            return zstd::decode_all(Cursor::new(buf)).map_err(|err| FormatError::Zstd {
                message: err.to_string(),
            });
        }
        return Ok(buf);
    }
    Err(FormatError::NoCollection)
}

fn read_sqlite(bytes: &[u8]) -> Result<Import, FormatError> {
    let conn = open_sqlite(bytes)?;
    let models = load_models(&conn)?;
    let decks = load_decks(&conn)?;
    let scheduling_rows = count_cards(&conn)?;
    let cards = load_cards(&conn)?;
    let raw_notes = load_notes(&conn)?;
    let mut notes = Vec::new();
    let mut skipped = 0u32;
    for raw in raw_notes {
        match note_from_apkg(&raw, &models, &decks, &cards)? {
            Some(note) => notes.push(note),
            None => skipped += 1,
        }
    }
    Ok(Import {
        notes,
        skipped,
        scheduling_rows,
    })
}

fn open_sqlite(bytes: &[u8]) -> Result<Connection, FormatError> {
    let mut conn = Connection::open_in_memory().map_err(sqlite)?;
    conn.deserialize_read_exact(rusqlite::MAIN_DB, Cursor::new(bytes), bytes.len(), true)
        .map_err(sqlite)?;
    Ok(conn)
}

fn sqlite(err: rusqlite::Error) -> FormatError {
    FormatError::Sqlite {
        message: err.to_string(),
    }
}

struct Model {
    name: String,
    fields: Vec<(i64, String)>,
}

struct RawNote {
    id: i64,
    guid: String,
    mid: i64,
    flds: String,
}

fn load_models(conn: &Connection) -> Result<HashMap<String, Model>, FormatError> {
    let mut models = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT key, json_extract(value, '$.name')
             FROM json_each((SELECT models FROM col LIMIT 1))",
        )
        .map_err(sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sqlite)?;
    for row in rows {
        let (id, name) = row.map_err(sqlite)?;
        models.insert(
            id,
            Model {
                name: name.unwrap_or_default(),
                fields: Vec::new(),
            },
        );
    }
    drop(stmt);
    let mut stmt = conn
        .prepare(
            "SELECT m.key, json_extract(f.value, '$.name'), json_extract(f.value, '$.ord')
             FROM json_each((SELECT models FROM col LIMIT 1)) AS m
             JOIN json_each(m.value, '$.flds') AS f",
        )
        .map_err(sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(sqlite)?;
    for row in rows {
        let (id, name, ord) = row.map_err(sqlite)?;
        if let Some(model) = models.get_mut(&id) {
            model
                .fields
                .push((ord.unwrap_or(0), name.unwrap_or_default()));
        }
    }
    Ok(models)
}

fn load_decks(conn: &Connection) -> Result<HashMap<String, String>, FormatError> {
    let mut stmt = conn
        .prepare(
            "SELECT key, json_extract(value, '$.name')
             FROM json_each((SELECT decks FROM col LIMIT 1))",
        )
        .map_err(sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sqlite)?;
    let mut decks = HashMap::new();
    for row in rows {
        let (id, name) = row.map_err(sqlite)?;
        if let Some(name) = name.filter(|name| !name.is_empty()) {
            decks.insert(id, name);
        }
    }
    Ok(decks)
}

fn count_cards(conn: &Connection) -> Result<u32, FormatError> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cards", [], |row| row.get(0))
        .map_err(sqlite)?;
    u32::try_from(count).map_err(|_| FormatError::Sqlite {
        message: "card count does not fit in u32".to_string(),
    })
}

fn load_cards(conn: &Connection) -> Result<HashMap<i64, Vec<(i64, i64)>>, FormatError> {
    let mut stmt = conn
        .prepare("SELECT nid, did, ord FROM cards")
        .map_err(sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(sqlite)?;
    let mut cards: HashMap<i64, Vec<(i64, i64)>> = HashMap::new();
    for row in rows {
        let (nid, did, ord) = row.map_err(sqlite)?;
        cards.entry(nid).or_default().push((did, ord));
    }
    Ok(cards)
}

fn load_notes(conn: &Connection) -> Result<Vec<RawNote>, FormatError> {
    let mut stmt = conn
        .prepare("SELECT id, guid, mid, flds FROM notes ORDER BY id")
        .map_err(sqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RawNote {
                id: row.get(0)?,
                guid: row.get(1)?,
                mid: row.get(2)?,
                flds: row.get(3)?,
            })
        })
        .map_err(sqlite)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(sqlite)
}

fn note_from_apkg(
    raw: &RawNote,
    models: &HashMap<String, Model>,
    decks: &HashMap<String, String>,
    cards: &HashMap<i64, Vec<(i64, i64)>>,
) -> Result<Option<Note>, FormatError> {
    let Some(guid) = Guid::new(raw.guid.trim()) else {
        return Err(FormatError::EmptyGuid);
    };
    let Some(model) = models.get(&raw.mid.to_string()) else {
        return Ok(None);
    };
    if model.name.is_empty() {
        return Err(FormatError::MissingNoteType { guid });
    }
    let Some(kind) = kind_of(&model.name) else {
        return Ok(None);
    };
    let Some(deck_name) = deck_for_note(raw.id, cards, decks) else {
        return Ok(None);
    };
    let Some(deck) = DeckName::new(deck_name) else {
        return Ok(None);
    };
    let fields = apkg_fields(&model.fields, &raw.flds);
    let body = body_from_fields(&guid, kind, &fields)?;
    Ok(Some(Note { guid, deck, body }))
}

/// One shared deck wins. Several decks use the lowest card `ord`.
/// No cards uses deck id 1 when that deck exists.
fn deck_for_note(
    nid: i64,
    cards: &HashMap<i64, Vec<(i64, i64)>>,
    decks: &HashMap<String, String>,
) -> Option<String> {
    let Some(rows) = cards.get(&nid).filter(|rows| !rows.is_empty()) else {
        return decks.get("1").cloned();
    };
    let mut dids: Vec<i64> = rows.iter().map(|(did, _)| *did).collect();
    dids.sort_unstable();
    dids.dedup();
    let did = if dids.len() == 1 {
        dids[0]
    } else {
        let mut ordered = rows.clone();
        ordered.sort_by_key(|(did, ord)| (*ord, *did));
        ordered[0].0
    };
    decks.get(&did.to_string()).cloned()
}

fn apkg_fields(fields: &[(i64, String)], flds: &str) -> HashMap<String, String> {
    let mut ordered = fields.to_vec();
    ordered.sort_by_key(|(ord, name)| (*ord, name.clone()));
    let parts: Vec<&str> = flds.split('\u{1f}').collect();
    let mut map = HashMap::new();
    for (index, (_, name)) in ordered.iter().enumerate() {
        if name.is_empty() || map.contains_key(name) {
            continue;
        }
        let value = parts.get(index).copied().unwrap_or("");
        map.insert(name.clone(), value.to_string());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn basic_sqlite() -> Vec<u8> {
        let conn = Connection::open_in_memory().unwrap();
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
        .unwrap();
        let models = r#"{"1":{"name":"Basic","flds":[{"name":"Back","ord":1},{"name":"Front","ord":0}],"tmpls":[{"name":"Card 1","ord":0}]}}"#;
        let decks = r#"{"1":{"name":"Default"},"2":{"name":"Spanish::Verbs"}}"#;
        conn.execute(
            "INSERT INTO col (id, models, decks) VALUES (1, ?1, ?2)",
            rusqlite::params![models, decks],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes (id, guid, mid, flds) VALUES (1, 'abc123XYZ0', 1, ?1)",
            rusqlite::params!["hablar\u{1f}to speak"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, nid, did, ord) VALUES (1, 1, 2, 0)",
            [],
        )
        .unwrap();
        let data = conn.serialize(rusqlite::MAIN_DB).unwrap();
        data.to_vec()
    }

    fn zip_entry(name: &str, bytes: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn basic_text() -> Import {
        accept_text(include_str!("../fixtures/basic.txt")).unwrap()
    }

    #[test]
    fn basic_text_reads_the_spanish_verb() {
        let import = basic_text();
        assert_eq!(import.notes.len(), 1);
        assert_eq!(import.skipped, 0);
        assert_eq!(import.scheduling_rows, 0);
        let note = &import.notes[0];
        assert_eq!(note.guid.as_str(), "abc123XYZ0");
        assert_eq!(note.deck.as_str(), "Spanish::Verbs");
        match &note.body {
            NoteBody::Basic { front, back } => {
                assert_eq!(front, "hablar");
                assert_eq!(back, "to speak");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(note.body.study_cards(), 1);
    }

    #[test]
    fn legacy_apkg_matches_basic_text() {
        let packed = zip_entry("collection.anki21", &basic_sqlite());
        let from_apkg = accept_apkg(&packed).unwrap();
        let from_text = basic_text();
        assert_eq!(from_apkg.notes, from_text.notes);
        assert_eq!(from_apkg.skipped, from_text.skipped);
        assert_eq!(from_text.scheduling_rows, 0);
        assert_eq!(from_apkg.scheduling_rows, 1);
    }

    #[test]
    fn zstd_collection_keeps_the_basic_guid() {
        let sqlite = basic_sqlite();
        let compressed = zstd::encode_all(
            Cursor::new(sqlite.as_slice()),
            zstd::DEFAULT_COMPRESSION_LEVEL,
        )
        .unwrap();
        let packed = zip_entry("collection.anki21b", &compressed);
        let import = accept_apkg(&packed).unwrap();
        assert_eq!(import.notes[0].guid.as_str(), "abc123XYZ0");
        assert_eq!(import.notes, basic_text().notes);
    }

    #[test]
    fn unknown_notetype_is_skipped() {
        let import = accept_text(include_str!("../fixtures/unknown.txt")).unwrap();
        assert_eq!(import.skipped, 1);
        assert_eq!(import.notes.len(), 1);
        assert_eq!(import.notes[0].guid.as_str(), "abc123XYZ0");
        assert_eq!(import.notes[0].deck.as_str(), "Spanish::Verbs");
        match &import.notes[0].body {
            NoteBody::Basic { front, back } => {
                assert_eq!(front, "hablar");
                assert_eq!(back, "to speak");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn choice_fixture_has_two_correct_ids() {
        let import = accept_text(include_str!("../fixtures/choice.txt")).unwrap();
        assert_eq!(import.notes.len(), 1);
        let note = &import.notes[0];
        assert_eq!(note.guid.as_str(), "gradeGuid01");
        assert_eq!(note.deck.as_str(), "Popup");
        match &note.body {
            NoteBody::Choice {
                prompt,
                choices,
                correct,
                explain,
            } => {
                assert_eq!(prompt, "Which of these are FSRS grades?");
                assert_eq!(choices.len(), 4);
                assert_eq!(choices[0].id.as_str(), "again");
                assert_eq!(choices[0].text, "Again");
                assert_eq!(choices[1].id.as_str(), "hard");
                assert_eq!(choices[1].text, "Hard");
                assert_eq!(choices[2].id.as_str(), "good");
                assert_eq!(choices[2].text, "Good");
                assert_eq!(choices[3].id.as_str(), "easy");
                assert_eq!(choices[3].text, "Easy");
                assert_eq!(
                    correct.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                    vec!["again", "good"]
                );
                assert_eq!(explain, "The crate returns one next state per grade.");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(note.body.study_cards(), 1);
    }

    #[test]
    fn empty_correct_is_an_error() {
        let text = "\
#separator:Tab
#notetype:Choice
#deck:Popup
#columns:Prompt\tChoices\tCorrect\tExplain\tguid
#guid column:5
Which?\tagain|Again;hard|Hard;good|Good;easy|Easy\t\tBecause.\temptyGuid01
";
        let err = accept_text(text).unwrap_err();
        match err {
            FormatError::EmptyCorrect { guid } => assert_eq!(guid.as_str(), "emptyGuid01"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_choice_id_is_an_error() {
        let text = "\
#separator:Tab
#notetype:Choice
#deck:Popup
#columns:Prompt\tChoices\tCorrect\tExplain\tguid
#guid column:5
Which?\tagain|Again;good|Good\tagain nope\tBecause.\tbadGuid0001
";
        let err = accept_text(text).unwrap_err();
        match err {
            FormatError::UnknownChoiceId { guid, id } => {
                assert_eq!(guid.as_str(), "badGuid0001");
                assert_eq!(id.as_str(), "nope");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reversed_model_studies_two_cards() {
        let text = "\
#separator:Tab
#notetype:Basic (and reversed card)
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
hablar\tto speak\trevGuid0001
";
        let import = accept_text(text).unwrap();
        let note = &import.notes[0];
        match &note.body {
            NoteBody::Reversed { front, back } => {
                assert_eq!(front, "hablar");
                assert_eq!(back, "to speak");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(note.body.study_cards(), 2);
    }
}
