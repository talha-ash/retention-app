//! Roots, decks, and the inbox for the control card.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryChoice {
    Inherit,
    Override { start: i64, end: i64, gap: i64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckRow {
    pub name: String,
    pub delivery: DeliveryChoice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootRules {
    pub start: i64,
    pub end: i64,
    pub gap: i64,
    pub gap_seconds: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    BadWindow,
    MissingRoot,
    MissingDeck,
    BadFileName,
    Failed,
    Exists,
}

pub struct Catalog {
    data_dir: PathBuf,
    roots: Vec<String>,
    open: Option<String>,
    decks: Vec<DeckRow>,
}

impl Catalog {
    pub fn load(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            roots: list_roots(data_dir),
            open: None,
            decks: Vec::new(),
        }
    }

    pub fn open_id(&self) -> Option<&str> {
        self.open.as_deref()
    }

    pub fn root_ids(&self) -> &[String] {
        &self.roots
    }

    pub fn decks(&self) -> &[DeckRow] {
        &self.decks
    }

    pub fn inbox(&self) -> Vec<String> {
        let mut names = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.data_dir.join("inbox")) else {
            return names;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_file() {
                continue;
            }
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.ends_with(".txt") || name.ends_with(".tsv") || name.ends_with(".apkg") {
                names.push(name);
            }
        }
        names.sort();
        names
    }

    pub fn create_root(&mut self) -> Result<String, Error> {
        let id = fresh_root_id(&self.roots);
        retention_popup::open_root(&self.data_dir, &id).map_err(|_| Error::Failed)?;
        self.roots = list_roots(&self.data_dir);
        self.open(&id)?;
        Ok(id)
    }

    pub fn open(&mut self, id: &str) -> Result<(), Error> {
        if !self.roots.iter().any(|have| have == id) {
            return Err(Error::MissingRoot);
        }
        let decks = read_decks(&self.data_dir.join("roots").join(id).join("decks.json"))?;
        self.open = Some(id.to_string());
        self.decks = decks;
        Ok(())
    }

    pub fn create_deck(&mut self) -> Result<String, Error> {
        self.open.as_ref().ok_or(Error::MissingRoot)?;
        let name = fresh_deck_name(&self.decks);
        let mut decks = self.decks.clone();
        decks.push(DeckRow {
            name: name.clone(),
            delivery: DeliveryChoice::Inherit,
        });
        self.write_decks(&decks)?;
        self.decks = decks;
        Ok(name)
    }

    pub fn set_inherit(&mut self, deck: &str) -> Result<(), Error> {
        self.open.as_ref().ok_or(Error::MissingRoot)?;
        let mut decks = self.decks.clone();
        let row = decks
            .iter_mut()
            .find(|row| row.name == deck)
            .ok_or(Error::MissingDeck)?;
        row.delivery = DeliveryChoice::Inherit;
        self.write_decks(&decks)?;
        self.decks = decks;
        Ok(())
    }

    pub fn set_override(
        &mut self,
        deck: &str,
        start: i64,
        end: i64,
        gap: i64,
    ) -> Result<(), Error> {
        self.open.as_ref().ok_or(Error::MissingRoot)?;
        if !self.decks.iter().any(|row| row.name == deck) {
            return Err(Error::MissingDeck);
        }
        let (start_i, end_i, gap_i) = (
            i32::try_from(start).map_err(|_| Error::BadWindow)?,
            i32::try_from(end).map_err(|_| Error::BadWindow)?,
            i32::try_from(gap).map_err(|_| Error::BadWindow)?,
        );
        if retention_popup::Window::new(start_i, end_i).is_none()
            || retention_popup::Gap::new(gap_i).is_none()
        {
            return Err(Error::BadWindow);
        }
        let mut decks = self.decks.clone();
        let row = decks
            .iter_mut()
            .find(|row| row.name == deck)
            .ok_or(Error::MissingDeck)?;
        row.delivery = DeliveryChoice::Override { start, end, gap };
        self.write_decks(&decks)?;
        self.decks = decks;
        Ok(())
    }

    pub fn import_inbox(&mut self, file_name: &str) -> Result<(), Error> {
        if file_name.is_empty() || file_name.contains('/') || file_name.contains("..") {
            return Err(Error::BadFileName);
        }
        let id = self.open.clone().ok_or(Error::MissingRoot)?;
        let path = self.data_dir.join("inbox").join(file_name);
        let root = retention_popup::open_root(&self.data_dir, &id).map_err(|_| Error::Failed)?;
        if file_name.ends_with(".apkg") {
            let bytes = std::fs::read(&path).map_err(|_| Error::Failed)?;
            retention_popup::import_apkg(&root, &bytes).map_err(|_| Error::Failed)?;
        } else {
            let text = std::fs::read_to_string(&path).map_err(|_| Error::Failed)?;
            retention_popup::import_text(&root, &text).map_err(|_| Error::Failed)?;
        }
        self.decks = read_decks(&self.data_dir.join("roots").join(id).join("decks.json"))?;
        Ok(())
    }

    pub fn rename_root(&mut self, from: &str, to: &str) -> Result<(), Error> {
        self.root_dir(from)?;
        let to = clean_id(to)?;
        if self.roots.iter().any(|have| have == &to) {
            return Err(Error::Exists);
        }
        let roots = self.data_dir.join("roots");
        std::fs::rename(roots.join(from), roots.join(&to)).map_err(|_| Error::Failed)?;
        self.roots = list_roots(&self.data_dir);
        if self.open.as_deref() == Some(from) {
            self.open = Some(to);
        }
        Ok(())
    }

    pub fn delete_root(&mut self, id: &str) -> Result<(), Error> {
        let dir = self.root_dir(id)?;
        std::fs::remove_dir_all(dir).map_err(|_| Error::Failed)?;
        self.roots = list_roots(&self.data_dir);
        if self.open.as_deref() == Some(id) {
            self.open = None;
            self.decks.clear();
        }
        Ok(())
    }

    pub fn root_rules(&self) -> Result<RootRules, Error> {
        let id = self.open.as_deref().ok_or(Error::MissingRoot)?;
        read_root_rules(&self.data_dir.join("roots").join(id).join("root.json"))
    }

    pub fn set_root_rules(&mut self, start: i64, end: i64, gap: i64) -> Result<(), Error> {
        let id = self.open.clone().ok_or(Error::MissingRoot)?;
        let path = self.data_dir.join("roots").join(&id).join("root.json");
        let mut rules = read_root_rules(&path).unwrap_or(RootRules {
            start: 540,
            end: 1080,
            gap: 45,
            gap_seconds: None,
        });
        checked_window(start, end, gap)?;
        rules.start = start;
        rules.end = end;
        rules.gap = gap;
        atomic_write(
            path.parent().ok_or(Error::Failed)?,
            "root.json",
            render_root(&rules).as_bytes(),
        )
    }

    pub fn rename_deck(&mut self, from: &str, to: &str) -> Result<(), Error> {
        let to = clean_name(to)?;
        if self.decks.iter().any(|row| row.name == to) {
            return Err(Error::Exists);
        }
        if !self.decks.iter().any(|row| row.name == from) {
            return Err(Error::MissingDeck);
        }
        let id = self.open.clone().ok_or(Error::MissingRoot)?;
        let root = retention_popup::open_root(&self.data_dir, &id).map_err(|_| Error::Failed)?;
        retention_popup::map_notes(&root, |note| {
            if note.deck.as_str() == from {
                if let Some(name) = retention_popup::DeckName::new(to.clone()) {
                    note.deck = name;
                }
            }
        })
        .map_err(|_| Error::Failed)?;
        let mut decks = self.decks.clone();
        let row = decks
            .iter_mut()
            .find(|row| row.name == from)
            .ok_or(Error::MissingDeck)?;
        row.name = to;
        self.write_decks(&decks)?;
        self.decks = decks;
        Ok(())
    }

    pub fn delete_deck(&mut self, name: &str) -> Result<(), Error> {
        if !self.decks.iter().any(|row| row.name == name) {
            return Err(Error::MissingDeck);
        }
        let id = self.open.clone().ok_or(Error::MissingRoot)?;
        let root = retention_popup::open_root(&self.data_dir, &id).map_err(|_| Error::Failed)?;
        let target = name.to_string();
        retention_popup::retain_notes(&root, |note| note.deck.as_str() != target)
            .map_err(|_| Error::Failed)?;
        let decks: Vec<DeckRow> = self
            .decks
            .iter()
            .filter(|row| row.name != name)
            .cloned()
            .collect();
        self.write_decks(&decks)?;
        self.decks = decks;
        Ok(())
    }

    fn root_dir(&self, id: &str) -> Result<PathBuf, Error> {
        if !self.roots.iter().any(|have| have == id) {
            return Err(Error::MissingRoot);
        }
        Ok(self.data_dir.join("roots").join(id))
    }

    pub fn export_notes(&mut self) -> Result<PathBuf, Error> {
        let id = self.open.clone().ok_or(Error::MissingRoot)?;
        let root = retention_popup::open_root(&self.data_dir, &id).map_err(|_| Error::Failed)?;
        let text = retention_popup::export_text(&root).map_err(|_| Error::Failed)?;
        let dir = self.data_dir.join("exports");
        let name = format!("{id}.txt");
        atomic_write(&dir, &name, text.as_bytes())?;
        Ok(dir.join(name))
    }

    fn write_decks(&self, decks: &[DeckRow]) -> Result<(), Error> {
        let id = self.open.as_deref().ok_or(Error::MissingRoot)?;
        let dir = self.data_dir.join("roots").join(id);
        atomic_write(&dir, "decks.json", render_decks(decks).as_bytes())
    }
}

fn clean_id(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') || value.contains("..") {
        Err(Error::BadFileName)
    } else {
        Ok(value.to_string())
    }
}

fn clean_name(value: &str) -> Result<String, Error> {
    let value = value.trim();
    if value.is_empty() || value.contains('\n') || value.contains('\t') {
        Err(Error::BadFileName)
    } else {
        Ok(value.to_string())
    }
}

fn checked_window(start: i64, end: i64, gap: i64) -> Result<(), Error> {
    let (start_i, end_i, gap_i) = (
        i32::try_from(start).map_err(|_| Error::BadWindow)?,
        i32::try_from(end).map_err(|_| Error::BadWindow)?,
        i32::try_from(gap).map_err(|_| Error::BadWindow)?,
    );
    if retention_popup::Window::new(start_i, end_i).is_none()
        || retention_popup::Gap::new(gap_i).is_none()
    {
        Err(Error::BadWindow)
    } else {
        Ok(())
    }
}

fn read_root_rules(path: &Path) -> Result<RootRules, Error> {
    let text = std::fs::read_to_string(path).map_err(|_| Error::Failed)?;
    let conn = Connection::open_in_memory().map_err(|_| Error::Failed)?;
    let (start, end, gap, gap_seconds): (Option<i64>, Option<i64>, Option<i64>, Option<i64>) =
        conn.query_row(
            "SELECT json_extract(?1, '$.start'), json_extract(?1, '$.end'), json_extract(?1, '$.gap'), json_extract(?1, '$.gap_seconds')",
            params![text],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| Error::Failed)?;
    Ok(RootRules {
        start: start.ok_or(Error::Failed)?,
        end: end.ok_or(Error::Failed)?,
        gap: gap.ok_or(Error::Failed)?,
        gap_seconds,
    })
}

fn render_root(rules: &RootRules) -> String {
    match rules.gap_seconds {
        Some(seconds) => format!(
            r#"{{"start":{},"end":{},"gap":{},"gap_seconds":{seconds}}}"#,
            rules.start, rules.end, rules.gap
        ),
        None => format!(
            r#"{{"start":{},"end":{},"gap":{}}}"#,
            rules.start, rules.end, rules.gap
        ),
    }
}

fn list_roots(data_dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(data_dir.join("roots")) else {
        return names;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        names.push(name);
    }
    names.sort();
    names
}

fn fresh_root_id(roots: &[String]) -> String {
    let mut n = 1u32;
    loop {
        let id = format!("r{n}");
        if !roots.iter().any(|have| have == &id) {
            return id;
        }
        n += 1;
    }
}

fn fresh_deck_name(decks: &[DeckRow]) -> String {
    let mut n = 1u32;
    loop {
        let name = if n == 1 {
            "Deck".to_string()
        } else {
            format!("Deck {n}")
        };
        if !decks.iter().any(|row| row.name == name) {
            return name;
        }
        n += 1;
    }
}

fn read_decks(path: &Path) -> Result<Vec<DeckRow>, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_decks(&text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err(Error::Failed),
    }
}

fn parse_decks(text: &str) -> Result<Vec<DeckRow>, Error> {
    let conn = Connection::open_in_memory().map_err(|_| Error::Failed)?;
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
        .map_err(|_| Error::Failed)?;
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
        .map_err(|_| Error::Failed)?;
    let mut decks = Vec::new();
    for row in rows {
        let (name, kind, raw, start, end, gap) = row.map_err(|_| Error::Failed)?;
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Err(Error::Failed);
        };
        let delivery = match (kind.as_deref(), raw.as_deref(), start, end, gap) {
            (Some("text"), Some("inherit"), _, _, _) => DeliveryChoice::Inherit,
            (Some("object"), _, Some(start), Some(end), Some(gap)) => {
                DeliveryChoice::Override { start, end, gap }
            }
            _ => return Err(Error::Failed),
        };
        decks.push(DeckRow { name, delivery });
    }
    Ok(decks)
}

fn render_decks(decks: &[DeckRow]) -> String {
    let parts: Vec<String> = decks
        .iter()
        .map(|deck| {
            let delivery = match deck.delivery {
                DeliveryChoice::Inherit => r#""inherit""#.to_string(),
                DeliveryChoice::Override { start, end, gap } => {
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

fn atomic_write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), Error> {
    std::fs::create_dir_all(dir).map_err(|_| Error::Failed)?;
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(|_| Error::Failed)?;
    std::fs::rename(&tmp, dir.join(name)).map_err(|_| Error::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("retention-library-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn reopen(dir: &Path, id: &str) -> Catalog {
        let mut catalog = Catalog::load(dir);
        catalog.open(id).unwrap();
        catalog
    }

    #[test]
    fn override_round_trip_returns_to_inherit() {
        let dir = temp_dir();
        let mut catalog = Catalog::load(&dir);
        assert_eq!(catalog.create_root().unwrap(), "r1");
        assert_eq!(catalog.create_deck().unwrap(), "Deck");
        assert_eq!(
            catalog.decks(),
            &[DeckRow {
                name: "Deck".into(),
                delivery: DeliveryChoice::Inherit,
            }]
        );
        catalog.set_override("Deck", 540, 1080, 45).unwrap();
        assert_eq!(
            catalog.decks(),
            &[DeckRow {
                name: "Deck".into(),
                delivery: DeliveryChoice::Override {
                    start: 540,
                    end: 1080,
                    gap: 45,
                },
            }]
        );
        assert_eq!(reopen(&dir, "r1").decks(), catalog.decks());
        catalog.set_inherit("Deck").unwrap();
        assert_eq!(
            catalog.decks(),
            &[DeckRow {
                name: "Deck".into(),
                delivery: DeliveryChoice::Inherit,
            }]
        );
        assert_eq!(reopen(&dir, "r1").decks(), catalog.decks());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn equal_window_stays_inherit() {
        let dir = temp_dir();
        let mut catalog = Catalog::load(&dir);
        catalog.create_root().unwrap();
        catalog.create_deck().unwrap();
        assert_eq!(
            catalog.set_override("Deck", 540, 540, 45),
            Err(Error::BadWindow)
        );
        assert_eq!(
            catalog.decks(),
            &[DeckRow {
                name: "Deck".into(),
                delivery: DeliveryChoice::Inherit,
            }]
        );
        assert_eq!(reopen(&dir, "r1").decks(), catalog.decks());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn import_exports_one_deck_and_a_second_import_does_not_duplicate() {
        let dir = temp_dir();
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(
            inbox.join("basic.txt"),
            include_str!("../fixtures/basic.txt"),
        )
        .unwrap();
        let mut catalog = Catalog::load(&dir);
        catalog.create_root().unwrap();
        catalog.import_inbox("basic.txt").unwrap();
        let exported = catalog.export_notes().unwrap();
        let text = std::fs::read_to_string(&exported).unwrap();
        assert!(text.contains("abc123XYZ0"));
        assert_eq!(exported, dir.join("exports/r1.txt"));
        assert_eq!(
            catalog.decks(),
            &[DeckRow {
                name: "Spanish::Verbs".into(),
                delivery: DeliveryChoice::Inherit,
            }]
        );
        catalog.import_inbox("basic.txt").unwrap();
        assert_eq!(catalog.decks().len(), 1);
        assert_eq!(catalog.decks()[0].name, "Spanish::Verbs");
        assert_eq!(catalog.create_deck().unwrap(), "Deck");
        assert_eq!(
            catalog.decks(),
            &[
                DeckRow {
                    name: "Spanish::Verbs".into(),
                    delivery: DeliveryChoice::Inherit,
                },
                DeckRow {
                    name: "Deck".into(),
                    delivery: DeliveryChoice::Inherit,
                },
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn second_root_keeps_the_first() {
        let dir = temp_dir();
        let mut catalog = Catalog::load(&dir);
        assert_eq!(catalog.create_root().unwrap(), "r1");
        assert_eq!(catalog.create_root().unwrap(), "r2");
        assert!(catalog.root_ids().iter().any(|id| id == "r1"));
        assert!(catalog.root_ids().iter().any(|id| id == "r2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rename_and_delete_root() {
        let dir = temp_dir();
        let mut catalog = Catalog::load(&dir);
        catalog.create_root().unwrap();
        catalog.rename_root("r1", "notes").unwrap();
        assert_eq!(catalog.open_id(), Some("notes"));
        assert!(dir.join("roots/notes").is_dir());
        assert!(!dir.join("roots/r1").exists());
        catalog.set_root_rules(0, 1440, 10).unwrap();
        assert_eq!(
            catalog.root_rules().unwrap(),
            RootRules {
                start: 0,
                end: 1440,
                gap: 10,
                gap_seconds: None,
            }
        );
        assert_eq!(catalog.set_root_rules(10, 10, 10), Err(Error::BadWindow));
        assert_eq!(catalog.root_rules().unwrap().start, 0);
        catalog.delete_root("notes").unwrap();
        assert!(catalog.root_ids().is_empty());
        assert!(catalog.open_id().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rename_and_delete_deck_moves_notes() {
        let dir = temp_dir();
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(
            inbox.join("basic.txt"),
            include_str!("../fixtures/basic.txt"),
        )
        .unwrap();
        let mut catalog = Catalog::load(&dir);
        catalog.create_root().unwrap();
        catalog.import_inbox("basic.txt").unwrap();
        catalog
            .set_override("Spanish::Verbs", 600, 900, 20)
            .unwrap();
        catalog.rename_deck("Spanish::Verbs", "Speech").unwrap();
        assert_eq!(
            catalog.decks()[0].delivery,
            DeliveryChoice::Override {
                start: 600,
                end: 900,
                gap: 20,
            }
        );
        let text = std::fs::read_to_string(catalog.export_notes().unwrap()).unwrap();
        assert!(text.contains("abc123XYZ0"));
        assert!(text.contains("Speech"));
        assert!(!text.contains("Spanish::Verbs"));
        assert_eq!(catalog.decks().len(), 1);
        assert_eq!(catalog.decks()[0].name, "Speech");
        catalog.delete_deck("Speech").unwrap();
        let text = std::fs::read_to_string(catalog.export_notes().unwrap()).unwrap();
        assert!(!text.contains("abc123XYZ0"));
        assert!(catalog.decks().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn inbox_lists_tsv_and_skips_csv() {
        let dir = temp_dir();
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(inbox.join("notes.tsv"), "x").unwrap();
        std::fs::write(inbox.join("notes.csv"), "x").unwrap();
        let catalog = Catalog::load(&dir);
        assert_eq!(catalog.inbox(), vec!["notes.tsv".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
