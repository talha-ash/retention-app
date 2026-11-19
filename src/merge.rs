use std::collections::BTreeMap;

use crate::accept_text;
use crate::format::FormatError;
use crate::model::Note;

/// Notes plus the due date the app owns. The due date is not in the text file.
pub struct Library {
    notes: Vec<Note>,
    due: BTreeMap<String, String>,
}

/// What one apply changed. `retired` stays 0 for a text file, because omission is not removal.
pub struct ApplyReport {
    pub added: u32,
    pub retired: u32,
    pub changed: u32,
}

impl Library {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            due: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, note: Note, due: Option<&str>) {
        let guid = note.guid.as_str().to_string();
        if let Some(due) = due {
            self.due.insert(guid, due.to_string());
        }
        self.notes.push(note);
    }

    pub fn due(&self, guid: &str) -> Option<&str> {
        self.due.get(guid).map(String::as_str)
    }

    pub fn note(&self, guid: &str) -> Option<&Note> {
        self.notes.iter().find(|note| note.guid.as_str() == guid)
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }
}

pub fn apply_text(library: &mut Library, text: &str) -> Result<ApplyReport, FormatError> {
    let import = accept_text(text)?;
    let report = apply_notes(library, import.notes);
    for guid in reset_guids(text) {
        library.due.remove(&guid);
    }
    Ok(report)
}

pub fn apply_apkg(library: &mut Library, bytes: &[u8]) -> Result<ApplyReport, FormatError> {
    let import = crate::accept_apkg(bytes)?;
    Ok(apply_notes(library, import.notes))
}

fn apply_notes(library: &mut Library, notes: Vec<Note>) -> ApplyReport {
    let mut added = 0;
    let mut changed = 0;
    for note in notes {
        let guid = note.guid.as_str().to_string();
        if let Some(existing) = library
            .notes
            .iter_mut()
            .find(|stored| stored.guid.as_str() == guid)
        {
            if existing.body != note.body || existing.deck != note.deck {
                existing.body = note.body;
                existing.deck = note.deck;
                changed += 1;
            }
        } else {
            library.notes.push(note);
            added += 1;
        }
    }
    ApplyReport {
        added,
        retired: 0,
        changed,
    }
}

/// `reset` is merge metadata, not a note field. `yes` clears that guid's due date.
fn reset_guids(text: &str) -> Vec<String> {
    let mut separator = '\t';
    for line in text.lines() {
        if !line.starts_with('#') {
            break;
        }
        let Some((key, value)) = line.trim_start_matches('#').split_once(':') else {
            continue;
        };
        if key.trim() == "separator" && value.trim() == "Comma" {
            separator = ',';
        }
    }
    let mut guid_col = None;
    let mut reset_col = None;
    for line in text.lines() {
        if !line.starts_with('#') {
            break;
        }
        let Some((key, value)) = line.trim_start_matches('#').split_once(':') else {
            continue;
        };
        match key.trim() {
            "guid column" => {
                guid_col = value
                    .trim()
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .map(|n| n - 1);
            }
            "columns" => {
                for (index, name) in value.split(separator).enumerate() {
                    if name.trim() == "reset" {
                        reset_col = Some(index);
                    }
                }
            }
            _ => {}
        }
    }
    let (Some(guid_col), Some(reset_col)) = (guid_col, reset_col) else {
        return Vec::new();
    };
    let mut seen_row = false;
    let mut guids = Vec::new();
    for line in text.lines() {
        if !seen_row && line.starts_with('#') {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        seen_row = true;
        let cells: Vec<&str> = line.split(separator).collect();
        if cells.get(reset_col).copied().unwrap_or("").trim() != "yes" {
            continue;
        }
        let guid = cells.get(guid_col).copied().unwrap_or("").trim();
        if !guid.is_empty() {
            guids.push(guid.to_string());
        }
    }
    guids
}

#[cfg(test)]
mod tests {
    use crate::accept_text;
    use crate::model::NoteBody;

    #[test]
    fn rephrased_front_keeps_the_due_date() {
        let import = accept_text(include_str!("../fixtures/basic.txt")).unwrap();
        let mut library = crate::Library::new();
        library.insert(import.notes[0].clone(), Some("2026-11-20"));

        let text = "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
hablar ahora\tto speak\tabc123XYZ0
";
        let report = crate::apply_text(&mut library, text).unwrap();

        assert_eq!(library.due("abc123XYZ0"), Some("2026-11-20"));
        match library.note("abc123XYZ0").unwrap().body {
            NoteBody::Basic { ref front, .. } => assert_eq!(front, "hablar ahora"),
            ref other => panic!("{other:?}"),
        }
        assert_eq!(report.retired, 0);
    }

    #[test]
    fn omitted_note_stays_and_a_new_guid_is_added() {
        let mut library = library_with_basic_due();
        let text = "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
comer\tto eat\tcomerGuid01
";
        let report = crate::apply_text(&mut library, text).unwrap();

        assert_eq!(library.due("abc123XYZ0"), Some("2026-11-20"));
        match library.note("abc123XYZ0").unwrap().body {
            NoteBody::Basic { ref front, .. } => assert_eq!(front, "hablar"),
            ref other => panic!("{other:?}"),
        }
        assert!(library.note("comerGuid01").is_some());
        assert_eq!(library.due("comerGuid01"), None);
        assert_eq!(report.added, 1);
        assert_eq!(report.retired, 0);
    }

    #[test]
    fn second_apply_reports_no_change() {
        let mut library = library_with_basic_due();
        let text = "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
hablar ahora\tto speak\tabc123XYZ0
";
        crate::apply_text(&mut library, text).unwrap();
        let again = crate::apply_text(&mut library, text).unwrap();

        assert_eq!(again.added, 0);
        assert_eq!(again.retired, 0);
        assert_eq!(again.changed, 0);
        assert_eq!(library.due("abc123XYZ0"), Some("2026-11-20"));
    }

    #[test]
    fn back_edit_keeps_the_due_date() {
        let mut library = library_with_basic_due();
        let text = "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
hablar\tto talk\tabc123XYZ0
";
        crate::apply_text(&mut library, text).unwrap();

        assert_eq!(library.due("abc123XYZ0"), Some("2026-11-20"));
        match library.note("abc123XYZ0").unwrap().body {
            NoteBody::Basic { ref back, .. } => assert_eq!(back, "to talk"),
            ref other => panic!("{other:?}"),
        }
    }

    #[test]
    fn reset_yes_clears_only_that_note() {
        let mut library = library_with_basic_due();
        let extra = accept_text(
            "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid
#guid column:3
comer\tto eat\tcomerGuid01
",
        )
        .unwrap();
        library.insert(extra.notes[0].clone(), Some("2026-12-01"));

        let text = "\
#separator:Tab
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front\tBack\tguid\treset
#guid column:3
hablar\tto speak\tabc123XYZ0\tyes
comer\tto eat\tcomerGuid01\t
";
        crate::apply_text(&mut library, text).unwrap();
        let again = crate::apply_text(&mut library, text).unwrap();

        assert_eq!(library.due("abc123XYZ0"), None);
        assert_eq!(library.due("comerGuid01"), Some("2026-12-01"));
        assert_eq!(again.changed, 0);
    }

    #[test]
    fn apkg_rephrase_keeps_the_due_date() {
        let mut library = library_with_basic_due();
        crate::apply_apkg(&mut library, &rephrased_apkg()).unwrap();

        assert_eq!(library.due("abc123XYZ0"), Some("2026-11-20"));
        match library.note("abc123XYZ0").unwrap().body {
            NoteBody::Basic { ref front, .. } => assert_eq!(front, "hablar ahora"),
            ref other => panic!("{other:?}"),
        }
    }

    fn rephrased_apkg() -> Vec<u8> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
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
        let models = r#"{"1":{"name":"Basic","flds":[{"name":"Front","ord":0},{"name":"Back","ord":1}],"tmpls":[{"name":"Card 1","ord":0}]}}"#;
        let decks = r#"{"1":{"name":"Default"},"2":{"name":"Spanish::Verbs"}}"#;
        conn.execute(
            "INSERT INTO col (id, models, decks) VALUES (1, ?1, ?2)",
            rusqlite::params![models, decks],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes (id, guid, mid, flds) VALUES (1, 'abc123XYZ0', 1, ?1)",
            rusqlite::params!["hablar ahora\u{1f}to speak"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, nid, did, ord) VALUES (1, 1, 2, 0)",
            [],
        )
        .unwrap();
        let sqlite = conn.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .start_file(
                "collection.anki21",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        std::io::Write::write_all(&mut writer, &sqlite).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn library_with_basic_due() -> crate::Library {
        let import = accept_text(include_str!("../fixtures/basic.txt")).unwrap();
        let mut library = crate::Library::new();
        library.insert(import.notes[0].clone(), Some("2026-11-20"));
        library
    }
}
