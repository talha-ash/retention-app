//! Control card, and a due card in the corner you picked.

mod desk;
mod library;
mod markdown;
mod session;
mod smoke;
mod tray;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use iced::gradient::Linear;
use iced::widget::shader::Shader;
use iced::widget::{
    Column, Space, Stack, button, column, container, row, scrollable, text, text_input,
};
use iced::{
    Background, Border, Color, Element, Length, Radians, Shadow, Subscription, Task, Theme, Vector,
};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;
use rusqlite::{Connection, params};

const BG: Color = Color::from_rgb8(0x1a, 0x1b, 0x26);
const FG: Color = Color::from_rgb8(0xc0, 0xca, 0xf5);
const MUTED: Color = Color::from_rgb8(0x56, 0x5f, 0x89);
const ACCENT: Color = Color::from_rgb8(0x7a, 0xa2, 0xf7);
const LINE: Color = Color::from_rgb8(0x41, 0x48, 0x68);
const GOLD: Color = Color::from_rgb8(0xe8, 0xc3, 0x72);
const GOLD_HOT: Color = Color::from_rgb8(0xff, 0xe1, 0xa8);
const INK: Color = Color::from_rgb8(0x1a, 0x12, 0x08);

fn boot_stream() -> impl iced::futures::Stream<Item = Message> + Send + 'static {
    iced::stream::channel(1, async |mut sender| {
        let _ = sender.try_send(Message::Boot);
        std::future::pending::<()>().await;
    })
}

fn listen(event: iced::Event, _: iced::event::Status, id: iced::window::Id) -> Option<Message> {
    match event {
        iced::Event::Window(iced::window::Event::Opened { .. }) => Some(Message::Opened(id)),
        iced::Event::Window(iced::window::Event::CloseRequested) => {
            Some(Message::CloseRequested(id))
        }
        _ => None,
    }
}

const POPUP_SIZE: (u32, u32) = (652, 552);
const MARGIN: i32 = 16;

fn main() -> iced_layershell::Result {
    let Some(pid_file) = claim_process() else {
        std::process::exit(0);
    };
    let result = daemon(App::new, "retention-controls", App::update, App::view)
        .subscription(App::subscription)
        .theme(|_: &App, _: iced::window::Id| Theme::Dark)
        .style(|_, theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .settings(Settings {
            layer_settings: LayerShellSettings {
                anchor: Anchor::Top | Anchor::Left,
                layer: Layer::Top,
                exclusive_zone: 0,
                size: None,
                margin: (0, 0, 0, 0),
                keyboard_interactivity: KeyboardInteractivity::None,
                start_mode: StartMode::Background,
                events_transparent: false,
            },
            ..Default::default()
        })
        .antialiasing(false)
        .run();
    drop(pid_file);
    result
}

struct PidFile {
    path: PathBuf,
    pid: u32,
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let owned = std::fs::read_to_string(&self.path)
            .ok()
            .is_some_and(|text| text.trim() == self.pid.to_string());
        if owned {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn claim_process() -> Option<PidFile> {
    let path = pid_path();
    let pid = std::process::id();
    let recorded = std::fs::read_to_string(&path)
        .ok()
        .as_deref()
        .and_then(parse_pid);
    if pid_busy(recorded, pid, proc_alive) {
        return None;
    }
    let _ = std::fs::write(&path, format!("{pid}\n"));
    Some(PidFile { path, pid })
}

fn pid_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("retention-popup.pid"),
        _ => PathBuf::from("/tmp/retention-popup.pid"),
    }
}

fn parse_pid(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

fn proc_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn pid_busy(recorded: Option<u32>, self_pid: u32, alive: impl Fn(u32) -> bool) -> bool {
    match recorded {
        Some(pid) if pid != self_pid && alive(pid) => true,
        _ => false,
    }
}

struct App {
    corner: Corner,
    offset: i64,
    study: Option<Study>,
    data: Option<PathBuf>,
    catalog: library::Catalog,
    selected: Option<String>,
    draft_open: bool,
    draft_start: i64,
    draft_end: i64,
    draft_gap: i64,
    status: String,
    page: Page,
    name_draft: String,
    pending: Option<Pending>,
    shell_id: Option<iced::window::Id>,
    desk: desk::Desk<iced::window::Id>,
    popup_id: Option<iced::window::Id>,
    tray: tray::TrayHandle,
    open: Option<OpenCard>,
    opened_at: Option<Instant>,
    smoke_time: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn label(self) -> &'static str {
        match self {
            Corner::TopLeft => "Top left",
            Corner::TopRight => "Top right",
            Corner::BottomLeft => "Bottom left",
            Corner::BottomRight => "Bottom right",
        }
    }

    fn anchor(self) -> Anchor {
        match self {
            Corner::TopLeft => Anchor::Top | Anchor::Left,
            Corner::TopRight => Anchor::Top | Anchor::Right,
            Corner::BottomLeft => Anchor::Bottom | Anchor::Left,
            Corner::BottomRight => Anchor::Bottom | Anchor::Right,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Roots,
    Root,
    Decks,
    Deck,
    Import,
    Settings,
}

enum Pending {
    Root(String),
    Deck(String),
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Pick(Corner),
    Tick,
    ShowAnswer,
    Toggle(String),
    Continue,
    Grade(retention_popup::Grade),
    Glow,
    Quit,
    OpenRoot(String),
    NewRoot,
    SelectDeck(String),
    NewDeck,
    SetInherit,
    OpenDraft,
    StartUp,
    StartDown,
    EndUp,
    EndDown,
    GapUp,
    GapDown,
    Save,
    Import(String),
    Export,
    Go(Page),
    NameChanged(String),
    SavePage,
    EditRoot(String),
    EditDeck(String),
    AskDelete,
    ConfirmDelete,
    CancelDelete,
    Opened(iced::window::Id),
    CloseRequested(iced::window::Id),
    Boot,
    Background,
    Tray(tray::TrayEvent),
}

impl App {
    fn new() -> Self {
        let requested = std::env::var("RETENTION_ROOT").ok();
        let data = data_dir();
        let chosen = data
            .as_ref()
            .and_then(|dir| choose_root(dir, requested.as_deref().filter(|id| !id.is_empty())));
        let study = data
            .as_ref()
            .zip(chosen.as_ref())
            .and_then(|(dir, id)| load_study(dir, id));
        let mut catalog = library::Catalog::load(data.as_deref().unwrap_or(Path::new("")));
        if let Some(id) = &chosen {
            let _ = catalog.open(id);
        }
        Self {
            corner: Corner::TopRight,
            offset: startup_offset(),
            study,
            data,
            catalog,
            selected: None,
            draft_open: false,
            draft_start: 540,
            draft_end: 1080,
            draft_gap: 45,
            status: String::new(),
            page: Page::Roots,
            name_draft: String::new(),
            pending: None,
            shell_id: None,
            desk: desk::Desk::Absent,
            popup_id: None,
            tray: tray::TrayHandle::start(),
            open: None,
            opened_at: None,
            smoke_time: 0.0,
        }
    }

    fn subscription(state: &Self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);
        let smoke = state
            .popup_id
            .is_some()
            .then(|| iced::time::every(Duration::from_millis(32)).map(|_| Message::Glow));
        Subscription::batch(
            std::iter::once(tick)
                .chain(smoke)
                .chain(std::iter::once(iced::event::listen_with(listen)))
                .chain(std::iter::once(Subscription::run(boot_stream)))
                .chain(std::iter::once(tray::subscription().map(Message::Tray))),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Pick(corner) => {
                self.corner = corner;
                Task::none()
            }
            Message::Tick => self.open_due(),
            Message::ShowAnswer => {
                if let Some(open) = self.open.as_mut() {
                    open.revealed = true;
                }
                Task::none()
            }
            Message::Toggle(id) => {
                if let Some(open) = self.open.as_mut() {
                    if let Some(at) = open.selected.iter().position(|have| have == &id) {
                        open.selected.remove(at);
                    } else {
                        open.selected.push(id);
                    }
                }
                Task::none()
            }
            Message::Continue => {
                let grade = self.choice_grade();
                match grade {
                    Some(grade) => self.grade(grade),
                    None => Task::none(),
                }
            }
            Message::Grade(grade) => self.grade(grade),
            Message::Glow => {
                if let Some(start) = self.opened_at {
                    self.smoke_time = start.elapsed().as_secs_f32();
                }
                Task::none()
            }
            Message::Quit => iced::exit(),
            Message::OpenRoot(id) => {
                let same = self.catalog.open_id() == Some(id.as_str());
                let opened = self.catalog.open(&id);
                if self.finish(opened) {
                    if !same {
                        self.selected = None;
                        self.draft_open = false;
                    }
                    self.page = Page::Decks;
                    self.pending = None;
                }
                Task::none()
            }
            Message::NewRoot => {
                let created = self.catalog.create_root().map(|_| ());
                if self.finish(created) {
                    self.selected = None;
                    self.draft_open = false;
                    self.page = Page::Decks;
                    self.pending = None;
                }
                Task::none()
            }
            Message::SelectDeck(name) => {
                self.selected = Some(name);
                self.draft_open = false;
                Task::none()
            }
            Message::NewDeck => {
                let created = self.catalog.create_deck().map(|_| ());
                let _ = self.finish(created);
                Task::none()
            }
            Message::SetInherit => {
                self.draft_open = false;
                Task::none()
            }
            Message::OpenDraft => {
                self.draft_open = true;
                Task::none()
            }
            Message::StartUp => {
                self.draft_start = slide(self.draft_start, 30, 0, 1440);
                Task::none()
            }
            Message::StartDown => {
                self.draft_start = slide(self.draft_start, -30, 0, 1440);
                Task::none()
            }
            Message::EndUp => {
                self.draft_end = slide(self.draft_end, 30, 0, 1440);
                Task::none()
            }
            Message::EndDown => {
                self.draft_end = slide(self.draft_end, -30, 0, 1440);
                Task::none()
            }
            Message::GapUp => {
                self.draft_gap = slide(self.draft_gap, 5, 1, 1440);
                Task::none()
            }
            Message::GapDown => {
                self.draft_gap = slide(self.draft_gap, -5, 1, 1440);
                Task::none()
            }
            Message::Save => {
                let Some(deck) = self.selected.clone() else {
                    return Task::none();
                };
                let saved = self.catalog.set_override(
                    &deck,
                    self.draft_start,
                    self.draft_end,
                    self.draft_gap,
                );
                if self.finish(saved) {
                    self.draft_open = false;
                }
                Task::none()
            }
            Message::Import(name) => {
                let imported = self.catalog.import_inbox(&name);
                let _ = self.finish(imported);
                Task::none()
            }
            Message::Export => {
                let exported = self.catalog.export_notes().map(|_| ());
                let _ = self.finish(exported);
                Task::none()
            }
            Message::Go(page) => self.go(page),
            Message::NameChanged(value) => {
                self.name_draft = value;
                Task::none()
            }
            Message::SavePage => self.save_page(),
            Message::EditRoot(id) => self.edit_root(id),
            Message::EditDeck(name) => self.edit_deck(name),
            Message::AskDelete => {
                self.pending = match self.page {
                    Page::Root => self
                        .catalog
                        .open_id()
                        .map(|id| Pending::Root(id.to_string())),
                    Page::Deck => self.selected.clone().map(Pending::Deck),
                    _ => None,
                };
                Task::none()
            }
            Message::CancelDelete => {
                self.pending = None;
                Task::none()
            }
            Message::ConfirmDelete => self.confirm_delete(),
            Message::Opened(id) => self.opened(id),
            Message::Boot => self.apply_desk(desk::DeskInput::ShellReady),
            Message::Background => self.apply_desk(desk::DeskInput::Background),
            Message::Tray(tray::TrayEvent::Open) => self.apply_desk(desk::DeskInput::Foreground),
            Message::Tray(tray::TrayEvent::Quit) => iced::exit(),
            Message::CloseRequested(id) => {
                if self.popup_id == Some(id) {
                    self.dismiss()
                } else {
                    self.apply_desk(desk::DeskInput::Close(id))
                }
            }
            _ => Task::none(),
        }
    }

    fn finish(&mut self, result: Result<(), library::Error>) -> bool {
        match result {
            Ok(()) => {
                self.status.clear();
                self.reload_study();
                true
            }
            Err(err) => {
                self.status = catalog_status(err).to_string();
                false
            }
        }
    }

    fn reload_study(&mut self) {
        let Some(dir) = self.data.clone() else {
            return;
        };
        let Some(id) = self.catalog.open_id().map(str::to_string) else {
            self.study = None;
            return;
        };
        self.study = load_study(&dir, &id);
    }

    fn choice_grade(&self) -> Option<retention_popup::Grade> {
        let open = self.open.as_ref()?;
        let FaceBody::Choice { correct, .. } = &open.face.body else {
            return None;
        };
        let selected: Vec<&str> = open.selected.iter().map(String::as_str).collect();
        let correct: Vec<&str> = correct.iter().map(String::as_str).collect();
        Some(retention_popup::grade_choice(&selected, &correct))
    }

    fn grade(&mut self, grade: retention_popup::Grade) -> Task<Message> {
        if self.open.as_ref().is_some_and(|open| open.persisted) {
            return self.close_after_shown();
        }
        let Some(face) = self.open.as_ref().map(|open| open.face.clone()) else {
            return Task::none();
        };
        let now = unix_now();
        let saved = if let Some(study) = self.study.as_mut() {
            study.save_grade(&face, grade, now)
        } else {
            return Task::none();
        };
        match saved {
            Save::Done => self.dismiss(),
            Save::MemoryOnly => {
                if let Some(open) = self.open.as_mut() {
                    open.persisted = true;
                }
                Task::none()
            }
            Save::Held => Task::none(),
        }
    }

    fn close_after_shown(&mut self) -> Task<Message> {
        let wrote = self
            .study
            .as_ref()
            .is_some_and(|study| write_shown(study).is_ok());
        if !wrote {
            return Task::none();
        }
        self.dismiss()
    }

    fn dismiss(&mut self) -> Task<Message> {
        self.open = None;
        self.opened_at = None;
        self.smoke_time = 0.0;
        match self.popup_id.take() {
            Some(id) => Task::done(Message::RemoveWindow(id)),
            None => Task::none(),
        }
    }

    fn open_due(&mut self) -> Task<Message> {
        if self.popup_id.is_some() {
            return Task::none();
        }
        let now = unix_now();
        let Some(minute) = local_minute(now, self.offset) else {
            return Task::none();
        };
        let Some(study) = self.study.as_ref() else {
            return Task::none();
        };
        let Some(picked) = study.session.pick(session::Unix::new(now), minute) else {
            return Task::none();
        };
        let Some(face) = study
            .faces
            .iter()
            .find(|face| {
                face.deck == picked.deck.as_str()
                    && face.guid == picked.guid.as_str()
                    && face.ord == picked.ord.get()
            })
            .cloned()
        else {
            return Task::none();
        };
        self.open = Some(OpenCard {
            face,
            revealed: false,
            selected: Vec::new(),
            persisted: false,
        });
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: Some(POPUP_SIZE),
            layer: Layer::Top,
            anchor: self.corner.anchor(),
            exclusive_zone: Some(0),
            margin: Some((MARGIN, MARGIN, MARGIN, MARGIN)),
            keyboard_interactivity: KeyboardInteractivity::None,
            output_option: OutputOption::Active,
            events_transparent: false,
            namespace: Some("retention-card".into()),
        });
        self.popup_id = Some(id);
        self.opened_at = Some(Instant::now());
        self.smoke_time = 0.0;
        task
    }

    fn view(&self, id: iced::window::Id) -> Element<'_, Message> {
        if self.popup_id == Some(id) {
            Stack::with_children([
                Shader::new(smoke::Smoke {
                    time: self.smoke_time,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
                container(quest_card(self.study_card()))
                    .padding(smoke::PAD)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            self.desktop()
        }
    }

    fn go(&mut self, page: Page) -> Task<Message> {
        let needs_root = matches!(page, Page::Decks | Page::Deck | Page::Import);
        if needs_root && self.catalog.open_id().is_none() {
            self.status = "Choose a library first.".to_string();
            self.page = Page::Roots;
            return Task::none();
        }
        self.page = page;
        self.pending = None;
        self.status.clear();
        Task::none()
    }

    fn edit_root(&mut self, id: String) -> Task<Message> {
        let opened = self.catalog.open(&id);
        if self.finish(opened) {
            self.name_draft = id;
            if let Ok(rules) = self.catalog.root_rules() {
                self.draft_start = rules.start;
                self.draft_end = rules.end;
                self.draft_gap = rules.gap;
            }
            self.page = Page::Root;
            self.pending = None;
            self.selected = None;
        }
        Task::none()
    }

    fn edit_deck(&mut self, name: String) -> Task<Message> {
        let row = self
            .catalog
            .decks()
            .iter()
            .find(|deck| deck.name == name)
            .cloned();
        let Some(row) = row else {
            return Task::none();
        };
        if let library::DeliveryChoice::Override { start, end, gap } = row.delivery {
            self.draft_start = start;
            self.draft_end = end;
            self.draft_gap = gap;
            self.draft_open = true;
        } else {
            self.draft_open = false;
        }
        self.selected = Some(name.clone());
        self.name_draft = name;
        self.page = Page::Deck;
        self.pending = None;
        self.status.clear();
        Task::none()
    }

    fn save_page(&mut self) -> Task<Message> {
        match self.page {
            Page::Root => {
                let Some(id) = self.catalog.open_id().map(str::to_string) else {
                    return Task::none();
                };
                let renamed = if self.name_draft.trim() != id {
                    self.catalog.rename_root(&id, &self.name_draft)
                } else {
                    Ok(())
                };
                if !self.finish(renamed) {
                    return Task::none();
                }
                let saved =
                    self.catalog
                        .set_root_rules(self.draft_start, self.draft_end, self.draft_gap);
                let _ = self.finish(saved);
            }
            Page::Deck => {
                let Some(name) = self.selected.clone() else {
                    return Task::none();
                };
                let renamed = if self.name_draft.trim() != name {
                    self.catalog.rename_deck(&name, &self.name_draft)
                } else {
                    Ok(())
                };
                if !self.finish(renamed) {
                    return Task::none();
                }
                let current = if self.name_draft.trim() != name {
                    self.name_draft.trim().to_string()
                } else {
                    name
                };
                self.selected = Some(current.clone());
                let saved = if self.draft_open {
                    self.catalog.set_override(
                        &current,
                        self.draft_start,
                        self.draft_end,
                        self.draft_gap,
                    )
                } else {
                    self.catalog.set_inherit(&current)
                };
                let _ = self.finish(saved);
            }
            _ => {}
        }
        Task::none()
    }

    fn confirm_delete(&mut self) -> Task<Message> {
        let Some(pending) = self.pending.take() else {
            return Task::none();
        };
        match pending {
            Pending::Root(id) => {
                let removed = self.catalog.delete_root(&id);
                if self.finish(removed) && self.catalog.open_id().is_none() {
                    self.page = Page::Roots;
                    self.selected = None;
                    return self.dismiss();
                }
            }
            Pending::Deck(name) => {
                let removed = self.catalog.delete_deck(&name);
                if self.finish(removed) {
                    self.selected = None;
                    self.page = Page::Decks;
                }
            }
        }
        Task::none()
    }

    fn opened(&mut self, id: iced::window::Id) -> Task<Message> {
        if self.popup_id == Some(id) {
            return Task::none();
        }
        let desk_window = match self.desk {
            desk::Desk::Opening(open) if open == id => true,
            desk::Desk::Shown(shown) if shown == id => true,
            _ => false,
        };
        if desk_window {
            return self.apply_desk(desk::DeskInput::WindowOpened(id));
        }
        if self.shell_id.is_none() {
            self.shell_id = Some(id);
        }
        if matches!(self.desk, desk::Desk::Shown(_)) {
            return self.remove_shell();
        }
        self.apply_desk(desk::DeskInput::ShellReady)
    }

    fn apply_desk(&mut self, input: desk::DeskInput<iced::window::Id>) -> Task<Message> {
        let before = self.desk;
        let (next, effects) = desk::step(before, input, self.tray.ready());
        self.desk = next;
        let mut tasks = Vec::new();
        let mut refused = false;
        for effect in effects {
            match effect {
                desk::DeskEffect::OpenWindow => {
                    if matches!(before, desk::Desk::Absent | desk::Desk::Hidden) {
                        let (id, task) = Message::base_window_open(
                            iced_layershell::actions::IcedXdgWindowSettings {
                                size: Some((960, 680)),
                                client_side_decorations: false,
                            },
                        );
                        self.desk = desk::Desk::Opening(id);
                        tasks.push(task);
                    }
                }
                desk::DeskEffect::CloseWindow(id) => {
                    tasks.push(Task::done(Message::RemoveWindow(id)));
                }
                desk::DeskEffect::RemoveShell => {
                    if let Some(shell) = self.shell_id.take() {
                        tasks.push(Task::done(Message::RemoveWindow(shell)));
                    }
                }
                desk::DeskEffect::Exit => tasks.push(iced::exit()),
                desk::DeskEffect::Refuse => {
                    refused = true;
                    self.status = "Tray is not available".into();
                }
            }
        }
        if matches!(input, desk::DeskInput::Background) && !refused {
            self.status.clear();
        }
        if !refused {
            self.tray.set_visible(desk::tray_visible(&self.desk));
        }
        Task::batch(tasks)
    }

    fn remove_shell(&mut self) -> Task<Message> {
        match self.shell_id.take() {
            Some(shell) => Task::done(Message::RemoveWindow(shell)),
            None => Task::none(),
        }
    }

    fn desktop(&self) -> Element<'_, Message> {
        row![self.rail(), self.page_body()]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn rail(&self) -> Element<'_, Message> {
        container(
            column![
                text("Retention").size(20).color(GOLD),
                text(self.catalog.open_id().unwrap_or("No library"))
                    .size(13)
                    .color(MUTED),
                self.nav("Libraries", Page::Roots),
                self.nav("Decks", Page::Decks),
                self.nav("Notes", Page::Import),
                self.nav("Settings", Page::Settings),
                Space::new().height(Length::Fill),
                wide("Quit", Message::Quit),
            ]
            .spacing(10)
            .height(Length::Fill)
            .width(Length::Fill),
        )
        .width(Length::Fixed(196.0))
        .height(Length::Fill)
        .padding(18)
        .style(|_| container::Style {
            background: Some(Background::Color(Color::from_rgb8(0x12, 0x14, 0x1e))),
            text_color: Some(FG),
            ..Default::default()
        })
        .into()
    }

    fn nav(&self, label: &'static str, page: Page) -> Element<'_, Message> {
        let on = match page {
            Page::Roots => matches!(self.page, Page::Roots | Page::Root),
            Page::Decks => matches!(self.page, Page::Decks | Page::Deck),
            other => self.page == other,
        };
        marked(label, on, Message::Go(page))
    }

    fn page_body(&self) -> Element<'_, Message> {
        let mut body = Column::new().spacing(12).width(Length::Fill);
        body = body.push(text(self.page_title()).size(26).color(FG));
        body = body.push(text(self.page_line()).size(14).color(MUTED));
        if !self.status.is_empty() {
            body = body.push(text(&self.status).size(14).color(GOLD));
        }
        body = body.push(self.page_content());
        container(scrollable(body))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(28)
            .style(|_| container::Style {
                background: Some(Background::Color(BG)),
                text_color: Some(FG),
                ..Default::default()
            })
            .into()
    }

    fn page_title(&self) -> String {
        match self.page {
            Page::Roots => "Libraries".to_string(),
            Page::Root => "Library".to_string(),
            Page::Decks => self
                .catalog
                .open_id()
                .map(|id| format!("Decks in {id}"))
                .unwrap_or_else(|| "Decks".to_string()),
            Page::Deck => "Deck".to_string(),
            Page::Import => "Notes".to_string(),
            Page::Settings => "Settings".to_string(),
        }
    }

    fn page_line(&self) -> &'static str {
        match self.page {
            Page::Roots => "Each library has its own cards and schedule.",
            Page::Root => "Cards from this library appear between these times.",
            Page::Decks => "A deck can keep the library schedule or use its own.",
            Page::Deck => "Use the library schedule, or set times for this deck only.",
            Page::Import => "Put a .txt, .tsv, or .apkg file in the inbox, then choose it here.",
            Page::Settings => "Due cards open in this corner.",
        }
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.page {
            Page::Roots => self.roots_page(),
            Page::Root => self.root_page(),
            Page::Decks => self.decks_page(),
            Page::Deck => self.deck_page(),
            Page::Import => self.import_page(),
            Page::Settings => self.settings_page(),
        }
    }

    fn roots_page(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(8).width(Length::Fill);
        if self.catalog.root_ids().is_empty() {
            list = list.push(text("No library yet.").size(14).color(MUTED));
        }
        for id in self.catalog.root_ids() {
            let on = self.catalog.open_id() == Some(id.as_str());
            list = list.push(
                row![
                    marked(id.clone(), on, Message::OpenRoot(id.clone())),
                    action("Edit", Message::EditRoot(id.clone())),
                ]
                .spacing(8),
            );
        }
        list = list.push(wide("New library", Message::NewRoot));
        list.into()
    }

    fn root_page(&self) -> Element<'_, Message> {
        column![
            text_input("Name", &self.name_draft)
                .on_input(Message::NameChanged)
                .padding(10)
                .size(16)
                .width(Length::Fill),
            self.schedule_editor(),
            wide("Save", Message::SavePage),
            self.delete_row(),
        ]
        .spacing(12)
        .width(Length::Fill)
        .into()
    }

    fn decks_page(&self) -> Element<'_, Message> {
        let mut list = Column::new().spacing(8).width(Length::Fill);
        if self.catalog.open_id().is_none() {
            return text("Choose a library first.").size(14).color(MUTED).into();
        }
        if self.catalog.decks().is_empty() {
            list = list.push(text("No decks yet.").size(14).color(MUTED));
        }
        for deck in self.catalog.decks() {
            let kind = match deck.delivery {
                library::DeliveryChoice::Inherit => "library schedule",
                library::DeliveryChoice::Override { .. } => "its own schedule",
            };
            list = list.push(marked(
                format!("{} · {kind}", deck.name),
                false,
                Message::EditDeck(deck.name.clone()),
            ));
        }
        list = list.push(wide("New deck", Message::NewDeck));
        list.into()
    }

    fn deck_page(&self) -> Element<'_, Message> {
        let mut body = column![
            text_input("Name", &self.name_draft)
                .on_input(Message::NameChanged)
                .padding(10)
                .size(16)
                .width(Length::Fill),
            row![
                marked("Library schedule", !self.draft_open, Message::SetInherit),
                marked("Its own schedule", self.draft_open, Message::OpenDraft),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .width(Length::Fill);
        if self.draft_open {
            body = body.push(self.schedule_editor());
        }
        body.push(wide("Save", Message::SavePage))
            .push(self.delete_row())
            .into()
    }

    fn import_page(&self) -> Element<'_, Message> {
        if self.catalog.open_id().is_none() {
            return text("Choose a library first.").size(14).color(MUTED).into();
        }
        let mut list = Column::new().spacing(8).width(Length::Fill);
        let inbox = self.catalog.inbox();
        if inbox.is_empty() {
            list = list.push(text("Nothing is waiting in the inbox.").size(14).color(MUTED));
        }
        for name in inbox {
            list = list.push(wide(format!("Add {name}"), Message::Import(name)));
        }
        list = list.push(wide("Export notes", Message::Export));
        list.into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        column![
            text("Corner").size(13).color(MUTED),
            row![
                chip(
                    Corner::TopLeft.label(),
                    self.corner == Corner::TopLeft,
                    Message::Pick(Corner::TopLeft),
                ),
                chip(
                    Corner::TopRight.label(),
                    self.corner == Corner::TopRight,
                    Message::Pick(Corner::TopRight),
                ),
            ]
            .spacing(8),
            row![
                chip(
                    Corner::BottomLeft.label(),
                    self.corner == Corner::BottomLeft,
                    Message::Pick(Corner::BottomLeft),
                ),
                chip(
                    Corner::BottomRight.label(),
                    self.corner == Corner::BottomRight,
                    Message::Pick(Corner::BottomRight),
                ),
            ]
            .spacing(8),
            text("Hide this window. The tray icon brings it back.")
                .size(14)
                .color(MUTED),
            wide("Run in background", Message::Background),
        ]
        .spacing(10)
        .into()
    }

    fn schedule_editor(&self) -> Element<'_, Message> {
        column![
            draft_row(
                format!("From {}", clock(self.draft_start)),
                "Earlier",
                Message::StartDown,
                "Later",
                Message::StartUp,
            ),
            draft_row(
                format!("Until {}", clock(self.draft_end)),
                "Earlier",
                Message::EndDown,
                "Later",
                Message::EndUp,
            ),
            draft_row(
                every(self.draft_gap),
                "Shorter",
                Message::GapDown,
                "Longer",
                Message::GapUp,
            ),
        ]
        .spacing(8)
        .into()
    }

    fn delete_row(&self) -> Element<'_, Message> {
        let asking = match (&self.pending, self.page) {
            (Some(Pending::Root(_)), Page::Root) | (Some(Pending::Deck(_)), Page::Deck) => true,
            _ => false,
        };
        let (label, confirm) = if self.page == Page::Deck {
            ("Delete deck", "Delete this deck")
        } else {
            ("Delete library", "Delete this library")
        };
        if asking {
            row![
                wide(confirm, Message::ConfirmDelete),
                wide("Cancel", Message::CancelDelete),
            ]
            .spacing(8)
            .into()
        } else {
            wide(label, Message::AskDelete)
        }
    }

    fn study_card(&self) -> Element<'_, Message> {
        let Some(open) = &self.open else {
            return Column::new().into();
        };
        match &open.face.body {
            FaceBody::Sides { prompt, answer } => {
                if open.revealed {
                    column![
                        reading(&[prompt, answer]),
                        row![
                            gold_button("Again", Message::Grade(retention_popup::Grade::Again)),
                            gold_button("Hard", Message::Grade(retention_popup::Grade::Hard)),
                            gold_button("Good", Message::Grade(retention_popup::Grade::Good)),
                            gold_button("Easy", Message::Grade(retention_popup::Grade::Easy)),
                        ]
                        .spacing(8),
                    ]
                    .spacing(12)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                } else {
                    column![
                        reading(&[prompt]),
                        gold_button("Show answer", Message::ShowAnswer),
                    ]
                    .spacing(12)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                }
            }
            FaceBody::Choice {
                prompt, choices, ..
            } => {
                let mut body = Column::new().spacing(12).width(Length::Fill);
                for block in markdown::blocks(prompt) {
                    body = body.push(block_line(&block));
                }
                for (id, label) in choices {
                    let on = open.selected.iter().any(|picked| picked == id);
                    body = body.push(choice_row(label, on, Message::Toggle(id.clone())));
                }
                column![
                    scrollable(body).width(Length::Fill).height(Length::Fill),
                    gold_button("Continue", Message::Continue),
                ]
                .spacing(12)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            }
        }
    }
}

fn reading(parts: &[&str]) -> Element<'static, Message> {
    let mut col = Column::new().spacing(10).width(Length::Fill);
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            col = col.push(Space::new().height(Length::Fixed(8.0)));
        }
        for block in markdown::blocks(part) {
            col = col.push(block_line(&block));
        }
    }
    scrollable(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn block_line(block: &markdown::Block) -> Element<'static, Message> {
    let (body, size, bullet) = match block {
        markdown::Block::Heading { level: 1, text } => (text.clone(), 26.0, false),
        markdown::Block::Heading { level: 2, text } => (text.clone(), 22.0, false),
        markdown::Block::Heading { text, .. } => (text.clone(), 20.0, false),
        markdown::Block::Paragraph(text) => (text.clone(), 22.0, false),
        markdown::Block::Item(text) => (text.clone(), 22.0, true),
    };
    let mut spans = Vec::new();
    if bullet {
        spans.push(mark_span("• ", size, iced::Font::default()));
    }
    for mark in markdown::marks(&body) {
        let (value, font) = match mark {
            markdown::Mark::Plain(value) => (value, iced::Font::default()),
            markdown::Mark::Strong(value) => (
                value,
                iced::Font {
                    weight: iced::font::Weight::Bold,
                    ..iced::Font::default()
                },
            ),
            markdown::Mark::Em(value) => (
                value,
                iced::Font {
                    style: iced::font::Style::Italic,
                    ..iced::Font::default()
                },
            ),
        };
        spans.push(mark_span(value, size, font));
    }
    if spans.is_empty() {
        spans.push(mark_span("", size, iced::Font::default()));
    }
    iced::widget::text::Rich::with_spans(spans)
        .size(size)
        .width(Length::Fill)
        .into()
}

fn mark_span(
    value: impl Into<String>,
    size: f32,
    font: iced::Font,
) -> iced::widget::text::Span<'static, (), iced::Font> {
    iced::widget::text::Span::new(value.into())
        .size(size)
        .font(font)
        .color(FG)
}

fn night_fill() -> Linear {
    Linear::new(Radians(1.15))
        .add_stop(0.0, Color::from_rgb8(0x3a, 0x22, 0x58))
        .add_stop(0.45, Color::from_rgb8(0x1c, 0x16, 0x32))
        .add_stop(1.0, Color::from_rgb8(0x10, 0x12, 0x1e))
}

fn gold_style(on: bool, status: button::Status) -> button::Style {
    let hot = on || status == button::Status::Hovered;
    button::Style {
        background: Some(Background::Color(if on {
            GOLD
        } else if hot {
            Color::from_rgba8(0xe8, 0xc3, 0x72, 0.16)
        } else {
            Color::from_rgba8(0, 0, 0, 0.28)
        })),
        text_color: if on { INK } else { GOLD },
        border: Border {
            radius: 12.0.into(),
            width: 1.0,
            color: if hot { GOLD_HOT } else { GOLD },
        },
        ..button::Style::default()
    }
}

fn action(label: impl Into<String>, message: Message) -> Element<'static, Message> {
    button(text(label.into()).size(14).color(GOLD))
        .on_press(message)
        .padding([8, 12])
        .style(|_, status| gold_style(false, status))
        .into()
}

fn wide(label: impl Into<String>, message: Message) -> Element<'static, Message> {
    button(text(label.into()).size(14).color(GOLD))
        .on_press(message)
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_, status| gold_style(false, status))
        .into()
}

fn marked(label: impl Into<String>, on: bool, message: Message) -> Element<'static, Message> {
    button(
        text(label.into())
            .size(14)
            .color(if on { INK } else { GOLD }),
    )
    .on_press(message)
    .width(Length::Fill)
    .padding([8, 12])
    .style(move |_, status| gold_style(on, status))
    .into()
}

fn draft_row(
    label: String,
    lower: &'static str,
    lower_message: Message,
    higher: &'static str,
    higher_message: Message,
) -> Element<'static, Message> {
    row![
        text(label).size(14).color(FG),
        action(lower, lower_message),
        action(higher, higher_message),
    ]
    .spacing(8)
    .into()
}

fn slide(value: i64, step: i64, low: i64, high: i64) -> i64 {
    value.saturating_add(step).clamp(low, high)
}

fn clock(minutes: i64) -> String {
    let minutes = minutes.clamp(0, 1440);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

fn every(minutes: i64) -> String {
    format!("Every {minutes} min")
}

fn catalog_status(err: library::Error) -> &'static str {
    match err {
        library::Error::BadWindow => "The end has to be after the start.",
        library::Error::MissingRoot => "Choose a library first.",
        library::Error::MissingDeck => "That deck is gone.",
        library::Error::BadFileName => "That file cannot be imported.",
        library::Error::Failed => "That did not work.",
        library::Error::Exists => "That name is already used.",
    }
}

fn chip(label: &str, on: bool, message: Message) -> Element<'_, Message> {
    button(text(label).size(14))
        .on_press(message)
        .style(move |_, status| {
            let fill = if on {
                ACCENT
            } else if status == button::Status::Hovered {
                Color::from_rgb8(0x2f, 0x35, 0x4d)
            } else {
                BG
            };
            button::Style {
                background: Some(Background::Color(fill)),
                text_color: if on { BG } else { FG },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if on { ACCENT } else { LINE },
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn quest_card(inner: Element<'_, Message>) -> Element<'_, Message> {
    let fill = night_fill();

    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(22)
        .style(move |_| container::Style {
            background: Some(Background::from(fill)),
            text_color: Some(FG),
            border: Border {
                color: GOLD,
                width: 1.5,
                radius: 22.0.into(),
            },
            shadow: Shadow {
                color: Color {
                    r: 0.91,
                    g: 0.76,
                    b: 0.45,
                    a: 0.28,
                },
                offset: Vector::new(0.0, 14.0),
                blur_radius: 32.0,
            },
            ..Default::default()
        })
        .into()
}

fn gold_button<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(16).color(GOLD))
        .on_press(message)
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_, status| button::Style {
            background: Some(Background::Color(Color::from_rgba8(
                0xe8,
                0xc3,
                0x72,
                if status == button::Status::Hovered {
                    0.18
                } else {
                    0.0
                },
            ))),
            text_color: GOLD,
            border: Border {
                radius: 12.0.into(),
                width: 1.0,
                color: GOLD,
            },
            ..button::Style::default()
        })
        .into()
}

fn choice_row<'a>(label: &'a str, on: bool, message: Message) -> Element<'a, Message> {
    button(text(label).size(16).color(if on { INK } else { GOLD }))
        .on_press(message)
        .width(Length::Fill)
        .padding(10)
        .style(move |_, status| {
            let hot = on || status == button::Status::Hovered;
            button::Style {
                background: Some(Background::Color(if on {
                    GOLD
                } else if hot {
                    Color::from_rgba8(0xe8, 0xc3, 0x72, 0.14)
                } else {
                    Color::from_rgba8(0, 0, 0, 0.28)
                })),
                text_color: if on { INK } else { GOLD },
                border: Border {
                    radius: 14.0.into(),
                    width: 1.0,
                    color: if hot { GOLD_HOT } else { GOLD },
                },
                ..button::Style::default()
            }
        })
        .into()
}

#[derive(Clone)]
struct OpenCard {
    face: Face,
    revealed: bool,
    selected: Vec<String>,
    persisted: bool,
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

struct Study {
    root: retention_popup::Root,
    dir: PathBuf,
    session: session::Session,
    faces: Vec<Face>,
    rows: Vec<retention_popup::MemoryRow>,
    decks: Vec<String>,
}

enum Save {
    Done,
    MemoryOnly,
    Held,
}

impl Study {
    fn save_grade(&mut self, face: &Face, grade: retention_popup::Grade, now: i64) -> Save {
        let previous = self
            .rows
            .iter()
            .find(|row| row.guid == face.guid && row.ord == face.ord)
            .map(memory_state);
        let Ok(scheduled) = retention_popup::schedule(previous, 0, grade, now) else {
            return Save::Held;
        };
        let Some(deck) = session::DeckName::new(face.deck.clone()) else {
            return Save::Held;
        };
        let Some(guid) = session::Guid::new(face.guid.clone()) else {
            return Save::Held;
        };
        let Some(ord) = session::CardOrd::new(face.ord) else {
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
            guid: face.guid.clone(),
            ord: face.ord,
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

fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("retention"));
    }
    std::env::var_os("HOME")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(".local/share/retention"))
}

fn choose_root(data_dir: &Path, requested: Option<&str>) -> Option<String> {
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

fn load_study(data_dir: &Path, root_id: &str) -> Option<Study> {
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
    Some(Study {
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

fn write_shown(study: &Study) -> std::io::Result<()> {
    let tmp = study.dir.join("shown.json.tmp");
    std::fs::write(&tmp, render_shown(study))?;
    std::fs::rename(&tmp, study.dir.join("shown.json"))
}

fn render_shown(study: &Study) -> String {
    let mut parts = Vec::new();
    for name in &study.decks {
        let next = session::DeckName::new(name.clone())
            .and_then(|deck| study.session.next_allowed(&deck))
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

fn startup_offset() -> i64 {
    let Ok(output) = std::process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(&output.stdout) else {
        return 0;
    };
    parse_offset(text.trim()).unwrap_or(0)
}

fn parse_offset(text: &str) -> Option<i64> {
    let sign = match text.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    if text.len() != 5 {
        return None;
    }
    let hours: i64 = text[1..3].parse().ok()?;
    let mins: i64 = text[3..5].parse().ok()?;
    if mins >= 60 {
        return None;
    }
    Some(sign * (hours * 3600 + mins * 60))
}

fn local_minute(unix: i64, offset: i64) -> Option<session::Minute> {
    let minute = (unix + offset).rem_euclid(86_400) / 60;
    session::Minute::new(i32::try_from(minute).ok()?)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| i64::try_from(dur.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minute(value: i32) -> session::Minute {
        session::Minute::new(value).expect("minute")
    }

    fn rules(text: &str) -> RootRules {
        parse_root(text).expect("root")
    }

    fn basic_notes() -> Vec<retention_popup::Note> {
        retention_popup::accept_text(include_str!("../fixtures/basic.txt"))
            .expect("notes")
            .notes
    }

    #[test]
    fn offset_and_local_minute() {
        assert_eq!(parse_offset("+0500"), Some(5 * 3600));
        assert_eq!(parse_offset("-0530"), Some(-(5 * 3600 + 30 * 60)));
        assert_eq!(
            local_minute(0, 5 * 3600).map(session::Minute::get),
            Some(300)
        );
        assert_eq!(
            local_minute(0, -5 * 3600).map(session::Minute::get),
            Some(19 * 60)
        );
    }

    #[test]
    fn cloze_hides_answer_and_hint() {
        let (prompt, answer) = cloze_sides("The {{c1::capital::city}} is {{c2::Paris}}.");
        assert_eq!(prompt, "The [...] is [...].");
        assert_eq!(answer, "The capital is Paris.");
    }

    #[test]
    fn gap_seconds_win_and_a_missing_row_is_due_at_zero() {
        let notes = basic_notes();
        let root = rules(r#"{"start":0,"end":1440,"gap":1,"gap_seconds":5}"#);
        let built = assemble(&notes, &[], &root, &[], &[]).expect("session");
        let now = session::Unix::new(1_700_000_000);
        let picked = built.session.pick(now, minute(0)).expect("due");
        assert_eq!(picked.due_unix.get(), 0);
        assert_eq!(picked.ord.get(), 0);
        let face = &built.faces[0];
        match &face.body {
            FaceBody::Sides { prompt, answer } => {
                assert_eq!(prompt, "hablar");
                assert_eq!(answer, "to speak");
            }
            FaceBody::Choice { .. } => panic!("basic face"),
        }
        let mut session = built.session;
        let applied = session.answer(
            &picked.deck,
            &picked.guid,
            picked.ord,
            session::Review {
                grade: retention_popup::Grade::Again,
                now,
                due_unix: session::Unix::new(1_700_000_003),
            },
        );
        assert!(applied);
        assert_eq!(
            session.next_allowed(&picked.deck).map(session::Unix::get),
            Some(1_700_000_005)
        );
        assert!(
            session
                .pick(session::Unix::new(1_700_000_004), minute(1439))
                .is_none()
        );
    }

    #[test]
    fn override_gap_is_minutes_and_inherit_uses_root_seconds() {
        let notes = basic_notes();
        let root = rules(r#"{"start":0,"end":1440,"gap":1,"gap_seconds":5}"#);
        let inherited = assemble(&notes, &[], &root, &[], &[]).expect("inherit");
        let deck = session::DeckName::new("Spanish::Verbs").unwrap();
        let guid = session::Guid::new("abc123XYZ0").unwrap();
        let ord = session::CardOrd::new(0).unwrap();
        let now = session::Unix::new(50);
        let mut session = inherited.session;
        assert!(session.answer(
            &deck,
            &guid,
            ord,
            session::Review {
                grade: retention_popup::Grade::Good,
                now,
                due_unix: session::Unix::new(80),
            },
        ));
        assert_eq!(
            session.next_allowed(&deck).map(session::Unix::get),
            Some(55)
        );

        let decks = parse_decks(
            r#"[{"name":"Spanish::Verbs","delivery":{"start":100,"end":200,"gap":2}}]"#,
        )
        .expect("decks");
        let overridden = assemble(&notes, &[], &root, &decks, &[]).expect("override");
        assert!(overridden.session.pick(now, minute(0)).is_none());
        assert!(overridden.session.pick(now, minute(100)).is_some());
        let mut session = overridden.session;
        assert!(session.answer(
            &deck,
            &guid,
            ord,
            session::Review {
                grade: retention_popup::Grade::Good,
                now,
                due_unix: session::Unix::new(80),
            },
        ));
        assert_eq!(
            session.next_allowed(&deck).map(session::Unix::get),
            Some(170)
        );
    }

    #[test]
    fn reversed_cloze_and_choice_faces() {
        let text = "\
#separator:Tab
#html:false
#notetype:Basic (and reversed card)
#deck:Words
#columns:Front\tBack\tguid
#guid column:3
front\tback\trevGuid0001
";
        let notes = retention_popup::accept_text(text).unwrap().notes;
        let root = rules(r#"{"start":0,"end":1440,"gap":1}"#);
        assert_eq!(root.gap_secs, 60);
        let built = assemble(&notes, &[], &root, &[], &[]).unwrap();
        assert_eq!(built.faces.len(), 2);
        match &built.faces[0].body {
            FaceBody::Sides { prompt, answer } => {
                assert_eq!(prompt, "front");
                assert_eq!(answer, "back");
            }
            FaceBody::Choice { .. } => panic!("reversed"),
        }
        match &built.faces[1].body {
            FaceBody::Sides { prompt, answer } => {
                assert_eq!(prompt, "back");
                assert_eq!(answer, "front");
            }
            FaceBody::Choice { .. } => panic!("reversed"),
        }

        let cloze = "\
#separator:Tab
#html:false
#notetype:Cloze
#deck:Words
#columns:Text\tguid
#guid column:2
The {{c1::capital::city}}.\tclozeGuid01
";
        let notes = retention_popup::accept_text(cloze).unwrap().notes;
        let built = assemble(&notes, &[], &root, &[], &[]).unwrap();
        assert_eq!(built.faces.len(), 1);
        assert_eq!(built.faces[0].ord, 0);
        match &built.faces[0].body {
            FaceBody::Sides { prompt, answer } => {
                assert_eq!(prompt, "The [...].");
                assert_eq!(answer, "The capital.");
            }
            FaceBody::Choice { .. } => panic!("cloze"),
        }

        let notes = retention_popup::accept_text(include_str!("../fixtures/choice.txt"))
            .unwrap()
            .notes;
        let built = assemble(&notes, &[], &root, &[], &[]).unwrap();
        assert_eq!(built.faces.len(), 1);
        match &built.faces[0].body {
            FaceBody::Choice {
                prompt,
                choices,
                correct,
            } => {
                assert_eq!(prompt, "Which of these are FSRS grades?");
                assert_eq!(choices.len(), 4);
                assert_eq!(correct.as_slice(), ["again", "good"]);
            }
            FaceBody::Sides { .. } => panic!("choice"),
        }
    }

    #[test]
    fn shown_roundtrip_and_a_missing_deck_is_zero() {
        let notes = basic_notes();
        let root = rules(r#"{"start":0,"end":1440,"gap":1,"gap_seconds":5}"#);
        let shown = parse_shown(r#"[{"deck":"Other","next_allowed":9}]"#).unwrap();
        let built = assemble(&notes, &[], &root, &[], &shown).unwrap();
        assert_eq!(
            built
                .session
                .next_allowed(&session::DeckName::new("Spanish::Verbs").unwrap())
                .map(session::Unix::get),
            Some(0)
        );
        let dir = scratch();
        let parsed = parse_shown(&render_shown(&Study {
            root: retention_popup::open_root(&dir, "fixture").unwrap(),
            dir: dir.clone(),
            session: built.session,
            faces: Vec::new(),
            rows: Vec::new(),
            decks: built.decks,
        }))
        .unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(parsed, vec![("Spanish::Verbs".to_string(), 0)]);
    }

    #[test]
    fn grade_persists_memory_and_the_gap() {
        let dir = scratch();
        let root = retention_popup::open_root(&dir, "fixture").unwrap();
        retention_popup::import_text(&root, include_str!("../fixtures/basic.txt")).unwrap();
        std::fs::write(
            dir.join("roots/fixture/root.json"),
            r#"{"start":0,"end":1440,"gap":1,"gap_seconds":5}"#,
        )
        .unwrap();
        let mut study = load_study(&dir, "fixture").expect("study");
        let now = 1_700_000_000;
        let face = study.faces[0].clone();
        let scheduled =
            retention_popup::schedule(None, 0, retention_popup::Grade::Again, now).unwrap();
        assert!(matches!(
            study.save_grade(&face, retention_popup::Grade::Again, now),
            Save::Done
        ));
        let rows = retention_popup::memory_rows(&study.root).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].guid, "abc123XYZ0");
        assert_eq!(rows[0].ord, 0);
        assert_eq!(rows[0].due_unix, scheduled.due_unix);
        assert_eq!(rows[0].stability, scheduled.memory.stability);
        assert_eq!(rows[0].difficulty, scheduled.memory.difficulty);
        assert_eq!(rows[0].stability_fast, scheduled.memory.stability_fast);
        let shown =
            parse_shown(&std::fs::read_to_string(study.dir.join("shown.json")).unwrap()).unwrap();
        assert_eq!(shown, vec![("Spanish::Verbs".to_string(), now + 5)]);
        let deck = session::DeckName::new("Spanish::Verbs").unwrap();
        let guid = session::Guid::new("abc123XYZ0").unwrap();
        let ord = session::CardOrd::new(0).unwrap();
        assert!(study.session.due_unix(&deck, &guid, ord).unwrap().get() <= now + 4);
        assert!(
            study
                .session
                .pick(session::Unix::new(now + 4), minute(0))
                .is_none()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_root_opens_and_two_roots_do_not() {
        let dir = scratch();
        std::fs::create_dir_all(dir.join("roots/only")).unwrap();
        assert_eq!(choose_root(&dir, None).as_deref(), Some("only"));
        std::fs::create_dir_all(dir.join("roots/other")).unwrap();
        assert_eq!(choose_root(&dir, None), None);
        assert_eq!(choose_root(&dir, Some("kept")).as_deref(), Some("kept"));
        assert!(pid_busy(Some(4), 9, |_| true));
        assert!(!pid_busy(Some(9), 9, |_| true));
        assert!(!pid_busy(Some(4), 9, |_| false));
        assert_eq!(parse_pid("42\n"), Some(42));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn clock_reads_minutes_since_midnight() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(540), "09:00");
        assert_eq!(clock(1080), "18:00");
        assert_eq!(clock(1439), "23:59");
        assert_eq!(clock(1440), "24:00");
        assert_eq!(clock(-5), "00:00");
    }

    #[test]
    fn every_states_the_gap() {
        assert_eq!(every(45), "Every 45 min");
        assert_eq!(every(1), "Every 1 min");
    }

    fn scratch() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("retention-popup-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
