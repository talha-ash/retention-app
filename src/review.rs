use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use crate::session;
use retention_popup::Grade;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Card {
    Prompt {
        text: String,
    },
    Answer {
        prompt: String,
        answer: String,
    },
    Choice {
        prompt: String,
        choices: Vec<(String, String)>,
        picked: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tick {
    Wait,
    Open(Card),
    Hold(Card),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Save {
    Done,
    MemoryOnly,
    Held,
}

pub struct Review {
    corner: Corner,
    session: Option<Loaded>,
    open: Option<OpenCard>,
}

impl Review {
    pub fn new() -> Self {
        Self {
            corner: Corner::TopRight,
            session: None,
            open: None,
        }
    }

    pub fn choose_root(data_dir: &Path, requested: Option<&str>) -> Option<String> {
        if let Some(id) = requested.filter(|id| !id.is_empty()) {
            return Some(id.to_string());
        }
        let mut names = Vec::new();
        let entries = std::fs::read_dir(data_dir.join("roots")).ok()?;
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
        if names.len() == 1 { names.pop() } else { None }
    }

    pub fn load(&mut self, data_dir: &Path, root_id: &str) -> bool {
        match load_state(data_dir, root_id) {
            Some(loaded) => {
                self.session = Some(loaded);
                true
            }
            None => {
                self.session = None;
                false
            }
        }
    }

    pub fn tick(&mut self, now: i64, offset: i64) -> Tick {
        if let Some(open) = &self.open {
            return Tick::Hold(view_card(open));
        }
        let Some(minute) = local_minute(now, offset) else {
            return Tick::Wait;
        };
        let face = {
            let Some(loaded) = self.session.as_ref() else {
                return Tick::Wait;
            };
            let Some(picked) = loaded.session.pick(session::Unix::new(now), minute) else {
                return Tick::Wait;
            };
            loaded
                .faces
                .iter()
                .find(|face| {
                    face.deck == picked.deck.as_str()
                        && face.guid == picked.guid.as_str()
                        && face.ord == picked.ord.get()
                })
                .cloned()
        };
        let Some(face) = face else {
            return Tick::Wait;
        };
        let open = OpenCard::from_face(face);
        let card = view_card(&open);
        self.open = Some(open);
        Tick::Open(card)
    }

    pub fn reveal(&mut self) -> Option<Card> {
        let open = self.open.as_mut()?;
        let (prompt, answer) = match &open.body {
            OpenBody::Prompt { prompt, answer } | OpenBody::Answer { prompt, answer } => {
                (prompt.clone(), answer.clone())
            }
            OpenBody::Choice { .. } => return None,
        };
        open.body = OpenBody::Answer {
            prompt: prompt.clone(),
            answer: answer.clone(),
        };
        Some(Card::Answer { prompt, answer })
    }

    pub fn toggle(&mut self, id: &str) -> Option<Card> {
        let open = self.open.as_mut()?;
        let OpenBody::Choice {
            prompt,
            choices,
            picked,
            ..
        } = &mut open.body
        else {
            return None;
        };
        if let Some(at) = picked.iter().position(|have| have == id) {
            picked.remove(at);
        } else {
            picked.push(id.to_string());
        }
        Some(Card::Choice {
            prompt: prompt.clone(),
            choices: choices.clone(),
            picked: picked.clone(),
        })
    }

    pub fn grade(&mut self, grade: Grade, now: i64) -> Save {
        if self.open.is_none() {
            return Save::Held;
        }
        if self.open.as_ref().is_some_and(|open| open.memory_only) {
            return self.retry_shown();
        }
        let Some(open) = self.open.as_ref() else {
            return Save::Held;
        };
        let deck = open.deck.clone();
        let guid = open.guid.clone();
        let ord = open.ord;
        let saved = {
            let Some(loaded) = self.session.as_mut() else {
                return Save::Held;
            };
            loaded.save_grade(&deck, &guid, ord, grade, now)
        };
        match saved {
            Save::Done => {
                self.open = None;
                Save::Done
            }
            Save::MemoryOnly => {
                if let Some(open) = self.open.as_mut() {
                    open.memory_only = true;
                }
                Save::MemoryOnly
            }
            Save::Held => Save::Held,
        }
    }

    pub fn commit(&mut self, now: i64) -> Save {
        let grade = {
            let Some(open) = self.open.as_ref() else {
                return Save::Held;
            };
            let OpenBody::Choice {
                correct, picked, ..
            } = &open.body
            else {
                return Save::Held;
            };
            let selected: Vec<&str> = picked.iter().map(String::as_str).collect();
            let correct: Vec<&str> = correct.iter().map(String::as_str).collect();
            retention_popup::grade_choice(&selected, &correct)
        };
        self.grade(grade, now)
    }

    pub fn dismiss(&mut self) {
        self.open = None;
    }

    pub fn corner(&self) -> Corner {
        self.corner
    }

    pub fn set_corner(&mut self, corner: Corner) {
        self.corner = corner;
    }

    pub fn card(&self) -> Option<Card> {
        self.open.as_ref().map(view_card)
    }

    fn retry_shown(&mut self) -> Save {
        let wrote = match self.session.as_ref() {
            Some(loaded) => write_shown(loaded).is_ok(),
            None => return Save::Held,
        };
        if !wrote {
            return Save::MemoryOnly;
        }
        self.open = None;
        Save::Done
    }
}

struct OpenCard {
    deck: String,
    guid: String,
    ord: i64,
    memory_only: bool,
    body: OpenBody,
}

enum OpenBody {
    Prompt {
        prompt: String,
        answer: String,
    },
    Answer {
        prompt: String,
        answer: String,
    },
    Choice {
        prompt: String,
        choices: Vec<(String, String)>,
        correct: Vec<String>,
        picked: Vec<String>,
    },
}

impl OpenCard {
    fn from_face(face: Face) -> Self {
        let body = match face.body {
            FaceBody::Sides { prompt, answer } => OpenBody::Prompt { prompt, answer },
            FaceBody::Choice {
                prompt,
                choices,
                correct,
            } => OpenBody::Choice {
                prompt,
                choices,
                correct,
                picked: Vec::new(),
            },
        };
        Self {
            deck: face.deck,
            guid: face.guid,
            ord: face.ord,
            memory_only: false,
            body,
        }
    }
}

#[derive(Clone)]
struct Face {
    deck: String,
    guid: String,
    ord: i64,
    body: FaceBody,
}

#[derive(Clone)]
enum FaceBody {
    Sides {
        prompt: String,
        answer: String,
    },
    Choice {
        prompt: String,
        choices: Vec<(String, String)>,
        correct: Vec<String>,
    },
}

struct Loaded {
    root: retention_popup::Root,
    dir: PathBuf,
    session: session::Session,
    faces: Vec<Face>,
    rows: Vec<retention_popup::MemoryRow>,
    decks: Vec<String>,
}

impl Loaded {
    fn save_grade(
        &mut self,
        deck_name: &str,
        guid_name: &str,
        ord_value: i64,
        grade: Grade,
        now: i64,
    ) -> Save {
        let previous = self
            .rows
            .iter()
            .find(|row| row.guid == guid_name && row.ord == ord_value)
            .map(memory_state);
        let Ok(scheduled) = retention_popup::schedule(previous, 0, grade, now) else {
            return Save::Held;
        };
        let Some(deck) = session::DeckName::new(deck_name.to_string()) else {
            return Save::Held;
        };
        let Some(guid) = session::Guid::new(guid_name.to_string()) else {
            return Save::Held;
        };
        let Some(ord) = session::CardOrd::new(ord_value) else {
            return Save::Held;
        };
        let applied = self.session.answer(
            &deck,
            &guid,
            ord,
            session::Review {
                grade,
                now: session::Unix::new(now),
                due_unix: session::Unix::new(scheduled.due_unix),
            },
        );
        if !applied {
            return Save::Held;
        }
        let row = retention_popup::MemoryRow {
            guid: guid_name.to_string(),
            ord: ord_value,
            stability: scheduled.memory.stability,
            difficulty: scheduled.memory.difficulty,
            stability_fast: scheduled.memory.stability_fast,
            due_unix: scheduled.due_unix,
        };
        if retention_popup::write_memory_row(&self.root, &row).is_err() {
            return Save::Held;
        }
        upsert_row(&mut self.rows, row);
        if write_shown(self).is_err() {
            return Save::MemoryOnly;
        }
        Save::Done
    }
}

fn view_card(open: &OpenCard) -> Card {
    match &open.body {
        OpenBody::Prompt { prompt, .. } => Card::Prompt {
            text: prompt.clone(),
        },
        OpenBody::Answer { prompt, answer } => Card::Answer {
            prompt: prompt.clone(),
            answer: answer.clone(),
        },
        OpenBody::Choice {
            prompt,
            choices,
            picked,
            ..
        } => Card::Choice {
            prompt: prompt.clone(),
            choices: choices.clone(),
            picked: picked.clone(),
        },
    }
}

fn memory_state(row: &retention_popup::MemoryRow) -> fsrs::MemoryState {
    fsrs::MemoryState {
        stability: row.stability,
        difficulty: row.difficulty,
        stability_fast: row.stability_fast,
    }
}

fn upsert_row(rows: &mut Vec<retention_popup::MemoryRow>, row: retention_popup::MemoryRow) {
    if let Some(existing) = rows
        .iter_mut()
        .find(|stored| stored.guid == row.guid && stored.ord == row.ord)
    {
        *existing = row;
    } else {
        rows.push(row);
    }
}

fn load_state(data_dir: &Path, root_id: &str) -> Option<Loaded> {
    let root = retention_popup::open_root(data_dir, root_id).ok()?;
    let dir = data_dir.join("roots").join(root_id);
    let exported = retention_popup::export_text(&root).ok()?;
    let import = retention_popup::accept_text(&exported).ok()?;
    let rows = retention_popup::memory_rows(&root).ok()?;
    let root_text = std::fs::read_to_string(dir.join("root.json")).ok()?;
    let rules = parse_root(&root_text)?;
    let deck_rules = read_decks(&dir.join("decks.json"))?;
    let shown = read_shown(&dir.join("shown.json"))?;
    let built = assemble(&import.notes, &rows, &rules, &deck_rules, &shown)?;
    Some(Loaded {
        root,
        dir,
        session: built.session,
        faces: built.faces,
        rows,
        decks: built.decks,
    })
}

struct RootRules {
    start: i32,
    end: i32,
    gap_secs: i64,
}

fn parse_root(text: &str) -> Option<RootRules> {
    let conn = Connection::open_in_memory().ok()?;
    let (start, end, gap, gap_seconds): (Option<i64>, Option<i64>, Option<i64>, Option<i64>) =
        conn.query_row(
            "SELECT json_extract(?1, '$.start'), json_extract(?1, '$.end'), json_extract(?1, '$.gap'), json_extract(?1, '$.gap_seconds')",
            params![text],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .ok()?;
    Some(RootRules {
        start: i32::try_from(start?).ok()?,
        end: i32::try_from(end?).ok()?,
        gap_secs: gap_seconds.unwrap_or(gap?.saturating_mul(60)),
    })
}

struct DeckRule {
    name: String,
    delivery: DeckDelivery,
}

enum DeckDelivery {
    Inherit,
    Override {
        start: i64,
        end: i64,
        gap_minutes: i64,
    },
}

fn read_decks(path: &Path) -> Option<Vec<DeckRule>> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_decks(&text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
        Err(_) => None,
    }
}

fn parse_decks(text: &str) -> Option<Vec<DeckRule>> {
    let conn = Connection::open_in_memory().ok()?;
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
        .ok()?;
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
        .ok()?;
    let mut decks = Vec::new();
    for row in rows {
        let (name, kind, raw, start, end, gap) = row.ok()?;
        let name = name.filter(|name| !name.is_empty())?;
        let delivery = match (kind.as_deref(), raw.as_deref(), start, end, gap) {
            (Some("text"), Some("inherit"), _, _, _) => DeckDelivery::Inherit,
            (Some("object"), _, Some(start), Some(end), Some(gap)) => DeckDelivery::Override {
                start,
                end,
                gap_minutes: gap,
            },
            _ => return None,
        };
        decks.push(DeckRule { name, delivery });
    }
    Some(decks)
}

fn read_shown(path: &Path) -> Option<Vec<(String, i64)>> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_shown(&text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(Vec::new()),
        Err(_) => None,
    }
}

fn parse_shown(text: &str) -> Option<Vec<(String, i64)>> {
    let conn = Connection::open_in_memory().ok()?;
    let mut stmt = conn
        .prepare(
            "SELECT json_extract(value, '$.deck'), json_extract(value, '$.next_allowed')
             FROM json_each(?1)",
        )
        .ok()?;
    let rows = stmt
        .query_map(params![text], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<i64>>(1)?,
            ))
        })
        .ok()?;
    let mut shown = Vec::new();
    for row in rows {
        let (deck, next_allowed) = row.ok()?;
        shown.push((deck.filter(|deck| !deck.is_empty())?, next_allowed?));
    }
    Some(shown)
}

fn write_shown(loaded: &Loaded) -> std::io::Result<()> {
    let tmp = loaded.dir.join("shown.json.tmp");
    std::fs::write(&tmp, render_shown(loaded))?;
    std::fs::rename(&tmp, loaded.dir.join("shown.json"))
}

fn render_shown(loaded: &Loaded) -> String {
    let mut parts = Vec::new();
    for name in &loaded.decks {
        let next = session::DeckName::new(name.clone())
            .and_then(|deck| loaded.session.next_allowed(&deck))
            .map(|unix| unix.get())
            .unwrap_or(0);
        parts.push(format!(
            r#"{{"deck":{},"next_allowed":{next}}}"#,
            json_string(name)
        ));
    }
    format!("[{}]\n", parts.join(","))
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
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
    out.push('"');
    out
}

struct Assembled {
    session: session::Session,
    faces: Vec<Face>,
    decks: Vec<String>,
}

fn assemble(
    notes: &[retention_popup::Note],
    rows: &[retention_popup::MemoryRow],
    root: &RootRules,
    deck_rules: &[DeckRule],
    shown: &[(String, i64)],
) -> Option<Assembled> {
    let mut names = Vec::new();
    for note in notes {
        let name = note.deck.as_str();
        if !names.iter().any(|have: &String| have == name) {
            names.push(name.to_string());
        }
    }
    names.sort();
    let mut decks = Vec::new();
    let mut faces = Vec::new();
    for name in &names {
        let (window, gap) = rules_for(name, root, deck_rules)?;
        let mut cards = Vec::new();
        for note in notes.iter().filter(|note| note.deck.as_str() == name) {
            for (card, face) in faces_of(note, rows)? {
                cards.push(card);
                faces.push(face);
            }
        }
        let next = shown
            .iter()
            .find(|(deck, _)| deck == name)
            .map(|(_, unix)| *unix)
            .unwrap_or(0);
        decks.push(session::Deck::new(
            session::DeckName::new(name.clone())?,
            window,
            gap,
            session::Unix::new(next),
            cards,
        )?);
    }
    Some(Assembled {
        session: session::Session::new(decks)?,
        faces,
        decks: names,
    })
}

fn rules_for(
    name: &str,
    root: &RootRules,
    decks: &[DeckRule],
) -> Option<(session::Window, session::Gap)> {
    let delivery = decks
        .iter()
        .find(|deck| deck.name == name)
        .map(|deck| &deck.delivery);
    match delivery {
        Some(DeckDelivery::Override {
            start,
            end,
            gap_minutes,
        }) => {
            let window =
                session::Window::new(i32::try_from(*start).ok()?, i32::try_from(*end).ok()?)?;
            let gap = session::Gap::from_seconds(gap_minutes.saturating_mul(60))?;
            Some((window, gap))
        }
        Some(DeckDelivery::Inherit) | None => {
            let window = session::Window::new(root.start, root.end)?;
            let gap = session::Gap::from_seconds(root.gap_secs)?;
            Some((window, gap))
        }
    }
}

fn faces_of(
    note: &retention_popup::Note,
    rows: &[retention_popup::MemoryRow],
) -> Option<Vec<(session::Card, Face)>> {
    let guid = session::Guid::new(note.guid.as_str())?;
    let deck = note.deck.as_str();
    let guid_str = note.guid.as_str();
    let mut out = Vec::new();
    match &note.body {
        retention_popup::NoteBody::Basic { front, back } => {
            out.push(side(&guid, deck, guid_str, 0, front, back, rows)?);
        }
        retention_popup::NoteBody::Reversed { front, back } => {
            out.push(side(&guid, deck, guid_str, 0, front, back, rows)?);
            out.push(side(&guid, deck, guid_str, 1, back, front, rows)?);
        }
        retention_popup::NoteBody::Cloze { text } => {
            let (prompt, answer) = cloze_sides(text);
            out.push(side(&guid, deck, guid_str, 0, &prompt, &answer, rows)?);
        }
        retention_popup::NoteBody::Choice {
            prompt,
            choices,
            correct,
            explain: _,
        } => {
            let ord = session::CardOrd::new(0)?;
            let card = session::Card::new(guid, ord, session::Unix::new(due_of(rows, guid_str, 0)));
            out.push((
                card,
                Face {
                    deck: deck.to_string(),
                    guid: guid_str.to_string(),
                    ord: 0,
                    body: FaceBody::Choice {
                        prompt: prompt.clone(),
                        choices: choices
                            .iter()
                            .map(|choice| (choice.id.as_str().to_string(), choice.text.clone()))
                            .collect(),
                        correct: correct.iter().map(|id| id.as_str().to_string()).collect(),
                    },
                },
            ));
        }
    }
    Some(out)
}

fn side(
    guid: &session::Guid,
    deck: &str,
    guid_str: &str,
    ord: i64,
    prompt: &str,
    answer: &str,
    rows: &[retention_popup::MemoryRow],
) -> Option<(session::Card, Face)> {
    let card = session::Card::new(
        guid.clone(),
        session::CardOrd::new(ord)?,
        session::Unix::new(due_of(rows, guid_str, ord)),
    );
    Some((
        card,
        Face {
            deck: deck.to_string(),
            guid: guid_str.to_string(),
            ord,
            body: FaceBody::Sides {
                prompt: prompt.to_string(),
                answer: answer.to_string(),
            },
        },
    ))
}

fn due_of(rows: &[retention_popup::MemoryRow], guid: &str, ord: i64) -> i64 {
    rows.iter()
        .find(|row| row.guid == guid && row.ord == ord)
        .map(|row| row.due_unix)
        .unwrap_or(0)
}

fn cloze_sides(text: &str) -> (String, String) {
    let mut prompt = String::new();
    let mut answer = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{c") {
        prompt.push_str(&rest[..start]);
        answer.push_str(&rest[..start]);
        let after = &rest[start + 3..];
        let Some(split_at) = after.find(|ch: char| !ch.is_ascii_digit()) else {
            prompt.push_str(&rest[start..]);
            answer.push_str(&rest[start..]);
            break;
        };
        if split_at == 0 || !after[split_at..].starts_with("::") {
            prompt.push_str("{{c");
            answer.push_str("{{c");
            rest = &rest[start + 3..];
            continue;
        }
        let inner_at = split_at + 2;
        let Some(end) = after[inner_at..].find("}}") else {
            prompt.push_str(&rest[start..]);
            answer.push_str(&rest[start..]);
            break;
        };
        let inner = &after[inner_at..inner_at + end];
        let revealed = inner
            .split_once("::")
            .map(|(answer, _)| answer)
            .unwrap_or(inner);
        prompt.push_str("[...]");
        answer.push_str(revealed);
        rest = &after[inner_at + end + 2..];
    }
    prompt.push_str(rest);
    answer.push_str(rest);
    (prompt, answer)
}

pub(crate) fn local_minute(unix: i64, offset: i64) -> Option<session::Minute> {
    let minute = (unix + offset).rem_euclid(86_400) / 60;
    session::Minute::new(i32::try_from(minute).ok()?)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    const NOW: i64 = 1_700_000_000;
    const ROOT_SECONDS: &str = r#"{"start":0,"end":1440,"gap":1,"gap_seconds":5}"#;

    #[test]
    fn choose_root_uses_the_only_directory_and_keeps_a_request() {
        let dir = scratch();
        std::fs::create_dir_all(dir.join("roots/only")).unwrap();
        std::fs::write(dir.join("roots/skip.txt"), "not a root").unwrap();
        assert_eq!(Review::choose_root(&dir, None).as_deref(), Some("only"));
        std::fs::create_dir_all(dir.join("roots/other")).unwrap();
        assert_eq!(Review::choose_root(&dir, None), None);
        assert_eq!(
            Review::choose_root(&dir, Some("kept")).as_deref(),
            Some("kept")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tick_opens_hablar_holds_and_then_waits_out_the_gap() {
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert!(
            retention_popup::memory_rows(&retention_popup::open_root(&dir, "fixture").unwrap())
                .unwrap()
                .is_empty()
        );
        assert_open_prompt(&mut review, 0, "hablar");
        review.dismiss();
        assert_open_prompt(&mut review, NOW, "hablar");
        assert_hold_prompt(&mut review, NOW, "hablar");
        assert_eq!(review.grade(Grade::Again, NOW), Save::Done);
        assert!(matches!(review.tick(NOW + 4, 0), Tick::Wait));
        assert_eq!(shown(&dir, "fixture"), NOW + 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn override_gap_is_minutes_and_inherit_uses_root_seconds() {
        let decks = r#"[{"name":"Spanish::Verbs","delivery":{"start":100,"end":200,"gap":2}}]"#;
        let overridden = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            Some(decks),
        );
        let mut review = Review::new();
        assert!(review.load(&overridden, "fixture"));
        assert!(matches!(review.tick(0, 0), Tick::Wait));
        assert_open_prompt(&mut review, 100 * 60, "hablar");
        assert_eq!(review.grade(Grade::Good, 50), Save::Done);
        assert_eq!(shown(&overridden, "fixture"), 170);

        let inherited = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&inherited, "fixture"));
        assert_open_prompt(&mut review, 50, "hablar");
        assert_eq!(review.grade(Grade::Good, 50), Save::Done);
        assert_eq!(shown(&inherited, "fixture"), 55);
        let _ = std::fs::remove_dir_all(overridden);
        let _ = std::fs::remove_dir_all(inherited);
    }

    #[test]
    fn reveal_reads_the_cloze_answer() {
        let notes = "\
#separator:Tab
#html:false
#notetype:Cloze
#deck:Words
#columns:Text\tguid
#guid column:2
The {{c1::capital::city}} is {{c2::Paris}}.\tclozeGuid01
";
        let dir = import("fixture", notes, ROOT_SECONDS, None);
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, NOW, "The [...] is [...].");
        match review.reveal() {
            Some(Card::Answer { prompt, answer }) => {
                assert_eq!(prompt, "The [...] is [...].");
                assert_eq!(answer, "The capital is Paris.");
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn grade_again_persists_the_scheduled_memory_row() {
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, NOW, "hablar");
        let scheduled = retention_popup::schedule(None, 0, Grade::Again, NOW).unwrap();
        assert_eq!(review.grade(Grade::Again, NOW), Save::Done);
        let root = retention_popup::open_root(&dir, "fixture").unwrap();
        let rows = retention_popup::memory_rows(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].guid, "abc123XYZ0");
        assert_eq!(rows[0].ord, 0);
        assert_eq!(rows[0].due_unix, scheduled.due_unix);
        assert_eq!(rows[0].stability, scheduled.memory.stability);
        assert_eq!(rows[0].difficulty, scheduled.memory.difficulty);
        assert_eq!(rows[0].stability_fast, scheduled.memory.stability_fast);
        assert_eq!(shown(&dir, "fixture"), NOW + 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn commit_stores_good_for_both_correct_ids_and_again_for_one() {
        let notes = include_str!("../fixtures/choice.txt");
        let good_dir = import("fixture", notes, ROOT_SECONDS, None);
        let mut review = Review::new();
        assert!(review.load(&good_dir, "fixture"));
        assert_choice_prompt(&mut review, NOW);
        assert!(review.toggle("again").is_some());
        assert!(review.toggle("good").is_some());
        let now = NOW;
        assert_eq!(review.commit(now), Save::Done);
        assert_due(&good_dir, "gradeGuid01", Grade::Good, now);

        let again_dir = import("fixture", notes, ROOT_SECONDS, None);
        let mut review = Review::new();
        assert!(review.load(&again_dir, "fixture"));
        assert_choice_prompt(&mut review, now);
        assert!(review.toggle("again").is_some());
        assert_eq!(review.commit(now), Save::Done);
        assert_due(&again_dir, "gradeGuid01", Grade::Again, now);
        let _ = std::fs::remove_dir_all(good_dir);
        let _ = std::fs::remove_dir_all(again_dir);
    }

    #[test]
    fn corner_starts_top_right_and_survives_load() {
        let mut review = Review::new();
        assert_eq!(review.corner(), Corner::TopRight);
        assert!(matches!(review.tick(NOW, 0), Tick::Wait));
        review.set_corner(Corner::BottomLeft);
        assert_eq!(review.corner(), Corner::BottomLeft);
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        assert!(review.load(&dir, "fixture"));
        assert_eq!(review.corner(), Corner::BottomLeft);
        assert_open_prompt(&mut review, NOW, "hablar");
        assert!(!review.load(&dir, ""));
        assert_eq!(review.corner(), Corner::BottomLeft);
        assert_hold_prompt(&mut review, NOW, "hablar");
        assert_eq!(review.grade(Grade::Again, NOW), Save::Held);
        let root = retention_popup::open_root(&dir, "fixture").unwrap();
        assert!(retention_popup::memory_rows(&root).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn dismiss_clears_the_open_card() {
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, NOW, "hablar");
        assert_hold_prompt(&mut review, NOW, "hablar");
        review.dismiss();
        assert_open_prompt(&mut review, NOW, "hablar");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn memory_only_retries_shown_without_changing_the_due_instant() {
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            ROOT_SECONDS,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, NOW, "hablar");
        let shown_path = dir.join("roots/fixture/shown.json");
        std::fs::create_dir(&shown_path).unwrap();
        assert_eq!(review.grade(Grade::Again, NOW), Save::MemoryOnly);
        let scheduled = retention_popup::schedule(None, 0, Grade::Again, NOW).unwrap();
        let root = retention_popup::open_root(&dir, "fixture").unwrap();
        let rows = retention_popup::memory_rows(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].due_unix, scheduled.due_unix);
        assert_hold_prompt(&mut review, NOW, "hablar");
        std::fs::remove_dir(&shown_path).unwrap();
        assert_eq!(review.grade(Grade::Again, NOW + 9), Save::Done);
        let rows = retention_popup::memory_rows(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].due_unix, scheduled.due_unix);
        assert_eq!(shown(&dir, "fixture"), NOW + 5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reversed_card_shows_the_back_after_the_front() {
        let notes = "\
#separator:Tab
#html:false
#notetype:Basic (and reversed card)
#deck:Words
#columns:Front\tBack\tguid
#guid column:3
front\tback\trevGuid0001
";
        let dir = import(
            "fixture",
            notes,
            r#"{"start":0,"end":1440,"gap":1,"gap_seconds":0}"#,
            None,
        );
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, NOW, "front");
        assert_eq!(review.grade(Grade::Again, NOW), Save::Done);
        assert_open_prompt(&mut review, NOW, "back");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_gap_seconds_waits_one_minute_and_ignores_another_deck() {
        let dir = import(
            "fixture",
            include_str!("../fixtures/basic.txt"),
            r#"{"start":0,"end":1440,"gap":1}"#,
            None,
        );
        std::fs::write(
            dir.join("roots/fixture/shown.json"),
            r#"[{"deck":"Other","next_allowed":9999999999}]"#,
        )
        .unwrap();
        let mut review = Review::new();
        assert!(review.load(&dir, "fixture"));
        assert_open_prompt(&mut review, 50, "hablar");
        assert_eq!(review.grade(Grade::Good, 50), Save::Done);
        assert_eq!(shown(&dir, "fixture"), 110);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn assert_open_prompt(review: &mut Review, now: i64, text: &str) {
        match review.tick(now, 0) {
            Tick::Open(Card::Prompt { text: got }) => assert_eq!(got, text),
            other => panic!("{other:?}"),
        }
    }

    fn assert_hold_prompt(review: &mut Review, now: i64, text: &str) {
        match review.tick(now, 0) {
            Tick::Hold(Card::Prompt { text: got }) => assert_eq!(got, text),
            other => panic!("{other:?}"),
        }
    }

    fn assert_choice_prompt(review: &mut Review, now: i64) {
        match review.tick(now, 0) {
            Tick::Open(Card::Choice { prompt, picked, .. }) => {
                assert_eq!(prompt, "Which of these are FSRS grades?");
                assert!(picked.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    fn assert_due(dir: &Path, guid: &str, grade: Grade, now: i64) {
        let scheduled = retention_popup::schedule(None, 0, grade, now).unwrap();
        let root = retention_popup::open_root(dir, "fixture").unwrap();
        let rows = retention_popup::memory_rows(&root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].guid, guid);
        assert_eq!(rows[0].ord, 0);
        assert_eq!(rows[0].due_unix, scheduled.due_unix);
    }

    fn shown(dir: &Path, id: &str) -> i64 {
        let text = std::fs::read_to_string(dir.join("roots").join(id).join("shown.json")).unwrap();
        let key = "\"next_allowed\":";
        let rest = text.split_once(key).unwrap().1;
        let end = rest
            .find(|ch: char| !ch.is_ascii_digit() && ch != '-')
            .unwrap();
        rest[..end].parse().unwrap()
    }

    fn import(id: &str, notes: &str, root_json: &str, decks: Option<&str>) -> PathBuf {
        let dir = scratch();
        let root = retention_popup::open_root(&dir, id).unwrap();
        retention_popup::import_text(&root, notes).unwrap();
        let base = dir.join("roots").join(id);
        std::fs::write(base.join("root.json"), root_json).unwrap();
        if let Some(text) = decks {
            std::fs::write(base.join("decks.json"), text).unwrap();
        }
        dir
    }

    fn scratch() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("retention-review-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
