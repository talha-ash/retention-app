//! The library screen. The saved rows stay in `Catalog`. This owns the unsaved draft.

use std::path::Path;

use crate::library::{self, Catalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Libraries,
    Inside(Focus),
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    Library,
    Deck(String),
    Notes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Root(String),
    Deck(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hours {
    Inherit { start: i64, end: i64, gap: i64 },
    Override { start: i64, end: i64, gap: i64 },
}

impl Hours {
    pub fn start(self) -> i64 {
        match self {
            Hours::Inherit { start, .. } | Hours::Override { start, .. } => start,
        }
    }

    pub fn end(self) -> i64 {
        match self {
            Hours::Inherit { end, .. } | Hours::Override { end, .. } => end,
        }
    }

    pub fn gap(self) -> i64 {
        match self {
            Hours::Inherit { gap, .. } | Hours::Override { gap, .. } => gap,
        }
    }

    pub fn own(self) -> bool {
        matches!(self, Hours::Override { .. })
    }

    fn with_clock(self, start: i64, end: i64, gap: i64) -> Self {
        match self {
            Hours::Inherit { .. } => Hours::Inherit { start, end, gap },
            Hours::Override { .. } => Hours::Override { start, end, gap },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Start,
    End,
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    Stay,
    Reload,
    ReloadAndClose,
}

pub struct Editor {
    catalog: Catalog,
    selected: Option<String>,
    hours: Hours,
    status: String,
    place: Place,
    name: String,
    pending: Option<Pending>,
}

impl Editor {
    pub fn load(data_dir: &Path) -> Self {
        Self {
            catalog: Catalog::load(data_dir),
            selected: None,
            hours: Hours::Inherit {
                start: 540,
                end: 1080,
                gap: 45,
            },
            status: String::new(),
            place: Place::Libraries,
            name: String::new(),
            pending: None,
        }
    }

    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    pub fn place(&self) -> &Place {
        &self.place
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hours(&self) -> Hours {
        self.hours
    }

    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    pub fn clear_status(&mut self) {
        self.status.clear();
    }

    pub fn open_saved(&mut self, id: &str) {
        let _ = self.catalog.open(id);
    }

    pub fn new_root(&mut self) -> Follow {
        match self.catalog.create_root() {
            Ok(id) => self.open_root(&id),
            Err(err) => self.fail(err),
        }
    }

    pub fn open_root(&mut self, id: &str) -> Follow {
        let opened = self.catalog.open(id);
        let follow = self.accept(opened);
        if follow == Follow::Reload {
            self.name = id.to_string();
            if let Ok(rules) = self.catalog.root_rules() {
                self.hours = self.hours.with_clock(rules.start, rules.end, rules.gap);
            }
            self.place = Place::Inside(Focus::Library);
            self.pending = None;
            self.selected = None;
        }
        follow
    }

    pub fn new_deck(&mut self) -> Follow {
        match self.catalog.create_deck() {
            Ok(name) => {
                let follow = self.accept(Ok(()));
                self.edit_deck(&name);
                follow
            }
            Err(err) => self.fail(err),
        }
    }

    pub fn edit_deck(&mut self, name: &str) {
        let row = self
            .catalog
            .decks()
            .iter()
            .find(|deck| deck.name == name)
            .cloned();
        let Some(row) = row else {
            return;
        };
        if let library::DeliveryChoice::Override { start, end, gap } = row.delivery {
            self.hours = Hours::Override { start, end, gap };
        } else {
            let start = self.hours.start();
            let end = self.hours.end();
            let gap = self.hours.gap();
            self.hours = Hours::Inherit { start, end, gap };
        }
        self.selected = Some(name.to_string());
        self.name = name.to_string();
        self.place = Place::Inside(Focus::Deck(name.to_string()));
        self.pending = None;
        self.status.clear();
    }

    pub fn libraries(&mut self) {
        self.place = Place::Libraries;
        self.pending = None;
    }

    pub fn settings(&mut self) {
        self.place = Place::Settings;
        self.pending = None;
    }

    pub fn notes(&mut self) {
        self.place = Place::Inside(Focus::Notes);
        self.pending = None;
    }

    pub fn set_name(&mut self, text: impl Into<String>) {
        self.name = text.into();
    }

    pub fn show_inherit(&mut self) {
        self.hours = Hours::Inherit {
            start: self.hours.start(),
            end: self.hours.end(),
            gap: self.hours.gap(),
        };
    }

    pub fn show_override(&mut self) {
        self.hours = Hours::Override {
            start: self.hours.start(),
            end: self.hours.end(),
            gap: self.hours.gap(),
        };
    }

    pub fn nudge(&mut self, step: Step, up: bool) {
        let (start, end, gap) = (self.hours.start(), self.hours.end(), self.hours.gap());
        let (start, end, gap) = match (step, up) {
            (Step::Start, true) => (slide(start, 30, 0, 1440), end, gap),
            (Step::Start, false) => (slide(start, -30, 0, 1440), end, gap),
            (Step::End, true) => (start, slide(end, 30, 0, 1440), gap),
            (Step::End, false) => (start, slide(end, -30, 0, 1440), gap),
            (Step::Gap, true) => (start, end, slide(gap, 5, 1, 1440)),
            (Step::Gap, false) => (start, end, slide(gap, -5, 1, 1440)),
        };
        self.hours = self.hours.with_clock(start, end, gap);
    }

    pub fn save_page(&mut self) -> Follow {
        if matches!(&self.place, Place::Inside(Focus::Library)) {
            let Some(id) = self.catalog.open_id().map(str::to_string) else {
                return Follow::Stay;
            };
            let saved = self.catalog.set_root_rules(
                self.hours.start(),
                self.hours.end(),
                self.hours.gap(),
            );
            if self.accept(saved) != Follow::Reload {
                return Follow::Stay;
            }
            if self.name.trim() != id {
                let renamed = self.catalog.rename_root(&id, &self.name);
                return self.accept(renamed);
            }
            return Follow::Reload;
        }
        if matches!(&self.place, Place::Inside(Focus::Deck(_))) {
            let Some(name) = self.selected.clone() else {
                return Follow::Stay;
            };
            let own = self.hours.own();
            let start = self.hours.start();
            let end = self.hours.end();
            let gap = self.hours.gap();
            let saved = if own {
                self.catalog.set_override(&name, start, end, gap)
            } else {
                self.catalog.set_inherit(&name)
            };
            if self.accept(saved) != Follow::Reload {
                return Follow::Stay;
            }
            if self.name.trim() != name {
                let renamed = self.catalog.rename_deck(&name, &self.name);
                if self.accept(renamed) != Follow::Reload {
                    return Follow::Stay;
                }
                let current = self.name.trim().to_string();
                self.selected = Some(current.clone());
                self.place = Place::Inside(Focus::Deck(current));
            }
            return Follow::Reload;
        }
        Follow::Stay
    }

    pub fn import(&mut self, name: &str) -> Follow {
        let imported = self.catalog.import_inbox(name);
        self.accept(imported)
    }

    pub fn export(&mut self) -> Follow {
        let exported = self.catalog.export_notes().map(|_| ());
        self.accept(exported)
    }

    pub fn ask_delete(&mut self) {
        self.pending = match &self.place {
            Place::Inside(Focus::Library) => self.catalog.open_id().map(|id| Pending::Root(id.to_string())),
            Place::Inside(Focus::Deck(_)) => self.selected.clone().map(Pending::Deck),
            _ => None,
        };
    }

    pub fn cancel_delete(&mut self) {
        self.pending = None;
    }

    pub fn confirm_delete(&mut self) -> Follow {
        let Some(pending) = self.pending.take() else {
            return Follow::Stay;
        };
        match pending {
            Pending::Root(id) => {
                let removed = self.catalog.delete_root(&id);
                let follow = self.accept(removed);
                if follow == Follow::Reload && self.catalog.open_id().is_none() {
                    self.place = Place::Libraries;
                    self.selected = None;
                    Follow::ReloadAndClose
                } else {
                    follow
                }
            }
            Pending::Deck(name) => {
                let removed = self.catalog.delete_deck(&name);
                let follow = self.accept(removed);
                if follow == Follow::Reload {
                    self.selected = None;
                    self.place = Place::Inside(Focus::Library);
                    if let Some(id) = self.catalog.open_id().map(str::to_string) {
                        let _ = self.open_root(&id);
                    }
                }
                follow
            }
        }
    }

    fn accept(&mut self, result: Result<(), library::Error>) -> Follow {
        match result {
            Ok(()) => {
                self.status.clear();
                Follow::Reload
            }
            Err(err) => self.fail(err),
        }
    }

    fn fail(&mut self, err: library::Error) -> Follow {
        self.status = status_line(err).to_string();
        Follow::Stay
    }
}

fn slide(value: i64, step: i64, low: i64, high: i64) -> i64 {
    value.saturating_add(step).clamp(low, high)
}

fn status_line(err: library::Error) -> &'static str {
    match err {
        library::Error::BadWindow => "The end has to be after the start.",
        library::Error::MissingRoot => "Choose a library first.",
        library::Error::MissingDeck => "That deck is gone.",
        library::Error::BadFileName => "That file cannot be imported.",
        library::Error::Failed => "That did not work.",
        library::Error::Exists => "That name is already used.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_screen_starts_at_nine_and_keeps_the_numbers_when_the_schedule_flips() {
        let dir = scratch();
        let mut editor = Editor::load(&dir);
        assert_eq!(
            editor.hours(),
            Hours::Inherit {
                start: 540,
                end: 1080,
                gap: 45
            }
        );
        editor.nudge(Step::Start, true);
        assert_eq!(editor.hours().start(), 570);
        editor.show_override();
        editor.nudge(Step::Gap, true);
        assert_eq!(
            editor.hours(),
            Hours::Override {
                start: 570,
                end: 1080,
                gap: 50
            }
        );
        editor.show_inherit();
        assert_eq!(
            editor.hours(),
            Hours::Inherit {
                start: 570,
                end: 1080,
                gap: 50
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_a_deck_writes_inherit_until_the_override_is_saved() {
        let dir = scratch();
        let mut editor = Editor::load(&dir);
        assert_eq!(editor.new_root(), Follow::Reload);
        assert!(matches!(editor.place(), Place::Inside(Focus::Library)));
        assert_eq!(editor.new_deck(), Follow::Reload);
        assert_eq!(editor.selected(), Some("Deck"));
        assert!(!editor.hours().own());
        assert_eq!(editor.save_page(), Follow::Reload);
        assert_eq!(
            editor.catalog().decks()[0].delivery,
            library::DeliveryChoice::Inherit
        );
        editor.show_override();
        editor.nudge(Step::Gap, true);
        assert_eq!(editor.save_page(), Follow::Reload);
        assert_eq!(
            editor.catalog().decks()[0].delivery,
            library::DeliveryChoice::Override {
                start: 540,
                end: 1080,
                gap: 50
            }
        );
        editor.edit_deck("Deck");
        assert_eq!(editor.hours().gap(), 50);
        assert!(editor.hours().own());
        editor.show_inherit();
        assert_eq!(editor.hours().gap(), 50);
        assert_eq!(editor.save_page(), Follow::Reload);
        assert_eq!(
            editor.catalog().decks()[0].delivery,
            library::DeliveryChoice::Inherit
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_window_that_ends_when_it_starts_stays_on_the_page() {
        let dir = scratch();
        let mut editor = Editor::load(&dir);
        assert_eq!(editor.new_root(), Follow::Reload);
        let id = editor.catalog().open_id().unwrap().to_string();
        editor.set_name("renamed");
        for _ in 0..18 {
            editor.nudge(Step::End, false);
        }
        assert_eq!(editor.hours().end(), 540);
        assert_eq!(editor.save_page(), Follow::Stay);
        assert_eq!(editor.status(), "The end has to be after the start.");
        assert_eq!(editor.catalog().open_id(), Some(id.as_str()));
        assert!(matches!(editor.place(), Place::Inside(Focus::Library)));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn deleting_the_open_library_leaves_the_library_list() {
        let dir = scratch();
        let mut editor = Editor::load(&dir);
        assert_eq!(editor.new_root(), Follow::Reload);
        let id = editor.catalog().open_id().unwrap().to_string();
        editor.ask_delete();
        assert_eq!(editor.confirm_delete(), Follow::ReloadAndClose);
        assert!(editor.catalog().open_id().is_none());
        assert!(!editor.catalog().root_ids().contains(&id));
        assert!(matches!(editor.place(), Place::Libraries));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("retention-editor-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
