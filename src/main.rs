//! Control card, and a due card in the corner you picked.

mod desk;
mod library;
mod markdown;
mod review;
mod session;
mod tray;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::widget::{
    Column, Row, Space, button, column, container, mouse_area, row, scrollable, text, text_input,
};
use iced::{Background, Border, Color, Element, Length, Subscription, Task, Theme};
use iced_layershell::build_pattern::daemon;
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

const BG: Color = Color::from_rgb8(0x24, 0x25, 0x2b);
const SURFACE: Color = Color::from_rgb8(0x1c, 0x1d, 0x22);
const FG: Color = Color::from_rgb8(0xe7, 0xe8, 0xec);
const MUTED: Color = Color::from_rgb8(0x93, 0x96, 0xa3);
const ACCENT: Color = Color::from_rgb8(0x6e, 0x9e, 0xeb);
const LINE: Color = Color::from_rgb8(0x3c, 0x3f, 0x4a);
const ON_ACCENT: Color = Color::from_rgb8(0xf7, 0xf8, 0xfb);

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

const POPUP_SIZE: (u32, u32) = (460, 360);
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
    offset: i64,
    review: review::Review,
    data: Option<PathBuf>,
    catalog: library::Catalog,
    selected: Option<String>,
    draft_open: bool,
    draft_start: i64,
    draft_end: i64,
    draft_gap: i64,
    status: String,
    place: Place,
    name_draft: String,
    pending: Option<Pending>,
    shell_id: Option<iced::window::Id>,
    desk: desk::Desk<iced::window::Id>,
    popup_id: Option<iced::window::Id>,
    tray: tray::TrayHandle,
}

fn corner_anchor(corner: review::Corner) -> Anchor {
    match corner {
        review::Corner::TopLeft => Anchor::Top | Anchor::Left,
        review::Corner::TopRight => Anchor::Top | Anchor::Right,
        review::Corner::BottomLeft => Anchor::Bottom | Anchor::Left,
        review::Corner::BottomRight => Anchor::Bottom | Anchor::Right,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Place {
    Libraries,
    Inside(Focus),
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Focus {
    Library,
    Deck(String),
    Notes,
}

enum Pending {
    Root(String),
    Deck(String),
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Pick(review::Corner),
    Tick,
    ShowAnswer,
    Toggle(String),
    Continue,
    Grade(retention_popup::Grade),
    Quit,
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
    Libraries,
    Settings,
    Notes,
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
    Drag,
}

impl App {
    fn new() -> Self {
        let requested = std::env::var("RETENTION_ROOT").ok();
        let data = data_dir();
        let chosen = data.as_ref().and_then(|dir| {
            review::Review::choose_root(dir, requested.as_deref().filter(|id| !id.is_empty()))
        });
        let mut review = review::Review::new();
        if let (Some(dir), Some(id)) = (data.as_ref(), chosen.as_ref()) {
            let _ = review.load(dir, id);
        }
        let mut catalog = library::Catalog::load(data.as_deref().unwrap_or(Path::new("")));
        if let Some(id) = &chosen {
            let _ = catalog.open(id);
        }
        Self {
            offset: startup_offset(),
            review,
            data,
            catalog,
            selected: None,
            draft_open: false,
            draft_start: 540,
            draft_end: 1080,
            draft_gap: 45,
            status: String::new(),
            place: Place::Libraries,
            name_draft: String::new(),
            pending: None,
            shell_id: None,
            desk: desk::Desk::Absent,
            popup_id: None,
            tray: tray::TrayHandle::start(),
        }
    }

    fn subscription(_state: &Self) -> Subscription<Message> {
        let tick = iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick);
        Subscription::batch(
            std::iter::once(tick)
                .chain(std::iter::once(iced::event::listen_with(listen)))
                .chain(std::iter::once(Subscription::run(boot_stream)))
                .chain(std::iter::once(tray::subscription().map(Message::Tray))),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Pick(corner) => {
                self.review.set_corner(corner);
                Task::none()
            }
            Message::Tick => self.open_due(),
            Message::ShowAnswer => {
                let _ = self.review.reveal();
                Task::none()
            }
            Message::Toggle(id) => {
                let _ = self.review.toggle(&id);
                Task::none()
            }
            Message::Continue => self.commit_review(),
            Message::Grade(grade) => self.grade(grade),
            Message::Quit => iced::exit(),
            Message::NewRoot => match self.catalog.create_root() {
                Ok(id) => self.edit_root(id),
                Err(err) => {
                    let _ = self.finish(Err(err));
                    Task::none()
                }
            },
            Message::SelectDeck(name) => {
                self.selected = Some(name);
                self.draft_open = false;
                Task::none()
            }
            Message::NewDeck => match self.catalog.create_deck() {
                Ok(name) => {
                    let _ = self.finish(Ok(()));
                    self.edit_deck(name)
                }
                Err(err) => {
                    let _ = self.finish(Err(err));
                    Task::none()
                }
            },
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
            Message::Libraries => {
                self.place = Place::Libraries;
                self.pending = None;
                Task::none()
            }
            Message::Settings => {
                self.place = Place::Settings;
                self.pending = None;
                Task::none()
            }
            Message::Notes => {
                self.place = Place::Inside(Focus::Notes);
                self.pending = None;
                Task::none()
            }
            Message::NameChanged(value) => {
                self.name_draft = value;
                Task::none()
            }
            Message::SavePage => self.save_page(),
            Message::EditRoot(id) => self.edit_root(id),
            Message::EditDeck(name) => self.edit_deck(name),
            Message::AskDelete => {
                self.pending = match &self.place {
                    Place::Inside(Focus::Library) => self
                        .catalog
                        .open_id()
                        .map(|id| Pending::Root(id.to_string())),
                    Place::Inside(Focus::Deck(_)) => self.selected.clone().map(Pending::Deck),
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
            Message::Drag => match self.desk {
                desk::Desk::Shown(id) => iced::window::drag(id),
                _ => Task::none(),
            },
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
            let _ = self.review.load(&dir, "");
            return;
        };
        let _ = self.review.load(&dir, &id);
    }

    fn grade(&mut self, grade: retention_popup::Grade) -> Task<Message> {
        let saved = self.review.grade(grade, unix_now());
        self.after_save(saved)
    }

    fn commit_review(&mut self) -> Task<Message> {
        let saved = self.review.commit(unix_now());
        self.after_save(saved)
    }

    fn after_save(&mut self, saved: review::Save) -> Task<Message> {
        match saved {
            review::Save::Done => self.dismiss(),
            review::Save::MemoryOnly | review::Save::Held => Task::none(),
        }
    }

    fn dismiss(&mut self) -> Task<Message> {
        self.review.dismiss();
        self.close_popup()
    }

    fn open_due(&mut self) -> Task<Message> {
        match self.review.tick(unix_now(), self.offset) {
            review::Tick::Wait => self.close_popup(),
            review::Tick::Open(_) | review::Tick::Hold(_) => self.ensure_popup(),
        }
    }

    fn ensure_popup(&mut self) -> Task<Message> {
        if self.popup_id.is_some() {
            return Task::none();
        }
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: Some(POPUP_SIZE),
            layer: Layer::Top,
            anchor: corner_anchor(self.review.corner()),
            exclusive_zone: Some(0),
            margin: Some((MARGIN, MARGIN, MARGIN, MARGIN)),
            keyboard_interactivity: KeyboardInteractivity::None,
            output_option: OutputOption::Active,
            events_transparent: false,
            namespace: Some("retention-card".into()),
        });
        self.popup_id = Some(id);
        task
    }

    fn close_popup(&mut self) -> Task<Message> {
        match self.popup_id.take() {
            Some(id) => Task::done(Message::RemoveWindow(id)),
            None => Task::none(),
        }
    }

    fn view(&self, id: iced::window::Id) -> Element<'_, Message> {
        if self.popup_id == Some(id) {
            quest_card(self.study_card())
        } else {
            self.desktop()
        }
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
            self.place = Place::Inside(Focus::Library);
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
        self.name_draft = name.clone();
        self.place = Place::Inside(Focus::Deck(name));
        self.pending = None;
        self.status.clear();
        Task::none()
    }

    fn save_page(&mut self) -> Task<Message> {
        if matches!(&self.place, Place::Inside(Focus::Library)) {
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
            return Task::none();
        }
        if matches!(&self.place, Place::Inside(Focus::Deck(_))) {
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
            self.place = Place::Inside(Focus::Deck(current.clone()));
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
                    self.place = Place::Libraries;
                    self.selected = None;
                    return self.dismiss();
                }
            }
            Pending::Deck(name) => {
                let removed = self.catalog.delete_deck(&name);
                if self.finish(removed) {
                    self.selected = None;
                    self.place = Place::Inside(Focus::Library);
                    if let Some(id) = self.catalog.open_id().map(str::to_string) {
                        return self.edit_root(id);
                    }
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
        column![
            self.top_bar(),
            row![self.rail(), self.page_body()]
                .width(Length::Fill)
                .height(Length::Fill),
        ]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }

    fn top_bar(&self) -> Element<'_, Message> {
        let inside = matches!(&self.place, Place::Inside(_));
        let open = self.catalog.open_id().map(str::to_string);
        let mut bar = Row::new()
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .width(Length::Fill)
            .push(text("Retention").size(20).color(FG))
            .push(bar_item(
                "Libraries",
                matches!(&self.place, Place::Libraries),
                Message::Libraries,
            ));
        for id in self.catalog.root_ids() {
            let on = inside && open.as_deref() == Some(id.as_str());
            bar = bar.push(bar_item(id.clone(), on, Message::EditRoot(id.clone())));
        }
        bar = bar.push(bar_item(
            "Settings",
            matches!(&self.place, Place::Settings),
            Message::Settings,
        ));
        mouse_area(
            container(bar)
                .width(Length::Fill)
                .padding(18)
                .style(|_| container::Style {
                    background: Some(Background::Color(SURFACE)),
                    text_color: Some(FG),
                    ..Default::default()
                }),
        )
        .on_press(Message::Drag)
        .into()
    }

    fn rail(&self) -> Element<'_, Message> {
        let open_here = matches!(&self.place, Place::Inside(_));
        let notes = matches!(&self.place, Place::Inside(Focus::Notes));
        let deck_name = match &self.place {
            Place::Inside(Focus::Deck(name)) => Some(name.clone()),
            _ => None,
        };
        let mut items = Column::new()
            .spacing(10)
            .height(Length::Fill)
            .width(Length::Fill);
        if open_here {
            for deck in self.catalog.decks() {
                let on = deck_name.as_deref() == Some(deck.name.as_str());
                items = items.push(marked(
                    deck.name.clone(),
                    on,
                    Message::EditDeck(deck.name.clone()),
                ));
            }
            items = items.push(marked("Notes", notes, Message::Notes));
            items = items.push(wide("New deck", Message::NewDeck));
        } else {
            for id in self.catalog.root_ids() {
                items = items.push(marked(id.clone(), false, Message::EditRoot(id.clone())));
            }
            items = items.push(wide("New library", Message::NewRoot));
        }
        items = items.push(Space::new().height(Length::Fill));
        items = items.push(wide("Quit", Message::Quit));
        container(items)
            .width(Length::Fixed(196.0))
            .height(Length::Fill)
            .padding(18)
            .style(|_| container::Style {
                background: Some(Background::Color(SURFACE)),
                text_color: Some(FG),
                ..Default::default()
            })
            .into()
    }

    fn page_body(&self) -> Element<'_, Message> {
        let mut body = Column::new().spacing(12).width(Length::Fill);
        body = body.push(text(self.page_title()).size(26).color(FG));
        body = body.push(text(self.page_line()).size(14).color(MUTED));
        if !self.status.is_empty() {
            body = body.push(text(&self.status).size(14).color(ACCENT));
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
        match &self.place {
            Place::Libraries => "Libraries".to_string(),
            Place::Inside(Focus::Library) => "Library".to_string(),
            Place::Inside(Focus::Deck(name)) => name.clone(),
            Place::Inside(Focus::Notes) => "Notes".to_string(),
            Place::Settings => "Settings".to_string(),
        }
    }

    fn page_line(&self) -> &'static str {
        match &self.place {
            Place::Libraries => "Each library has its own cards and schedule.",
            Place::Inside(Focus::Library) => "Cards from this library appear between these times.",
            Place::Inside(Focus::Deck(_)) => {
                "Use the library schedule, or set times for this deck only."
            }
            Place::Inside(Focus::Notes) => {
                "Put a .txt, .tsv, or .apkg file in the inbox, then choose it here."
            }
            Place::Settings => "Due cards open in this corner.",
        }
    }

    fn page_content(&self) -> Element<'_, Message> {
        match self.place.clone() {
            Place::Libraries => text("Choose a library.").size(14).color(MUTED).into(),
            Place::Inside(Focus::Library) => self.root_page(),
            Place::Inside(Focus::Deck(_)) => self.deck_page(),
            Place::Inside(Focus::Notes) => self.import_page(),
            Place::Settings => self.settings_page(),
        }
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
            list = list.push(
                text("Nothing is waiting in the inbox.")
                    .size(14)
                    .color(MUTED),
            );
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
                    review::Corner::TopLeft.label(),
                    self.review.corner() == review::Corner::TopLeft,
                    Message::Pick(review::Corner::TopLeft),
                ),
                chip(
                    review::Corner::TopRight.label(),
                    self.review.corner() == review::Corner::TopRight,
                    Message::Pick(review::Corner::TopRight),
                ),
            ]
            .spacing(8),
            row![
                chip(
                    review::Corner::BottomLeft.label(),
                    self.review.corner() == review::Corner::BottomLeft,
                    Message::Pick(review::Corner::BottomLeft),
                ),
                chip(
                    review::Corner::BottomRight.label(),
                    self.review.corner() == review::Corner::BottomRight,
                    Message::Pick(review::Corner::BottomRight),
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
        let asking = matches!(
            (&self.pending, &self.place),
            (Some(Pending::Root(_)), Place::Inside(Focus::Library))
                | (Some(Pending::Deck(_)), Place::Inside(Focus::Deck(_)))
        );
        let (label, confirm) = if matches!(&self.place, Place::Inside(Focus::Deck(_))) {
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
        let Some(card) = self.review.card() else {
            return Column::new().into();
        };
        match card {
            review::Card::Prompt { text } => column![
                reading(&[text.as_str()]),
                card_button("Show answer", Message::ShowAnswer),
            ]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            review::Card::Answer { prompt, answer } => column![
                reading(&[prompt.as_str(), answer.as_str()]),
                row![
                    card_button("Again", Message::Grade(retention_popup::Grade::Again)),
                    card_button("Hard", Message::Grade(retention_popup::Grade::Hard)),
                    card_button("Good", Message::Grade(retention_popup::Grade::Good)),
                    card_button("Easy", Message::Grade(retention_popup::Grade::Easy)),
                ]
                .spacing(8),
            ]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            review::Card::Choice {
                prompt,
                choices,
                picked,
            } => {
                let mut body = Column::new().spacing(12).width(Length::Fill);
                for block in markdown::blocks(&prompt) {
                    body = body.push(block_line(&block));
                }
                for (id, label) in choices {
                    let on = picked.iter().any(|item| item == &id);
                    body = body.push(choice_row(label, on, Message::Toggle(id)));
                }
                column![
                    scrollable(body).width(Length::Fill).height(Length::Fill),
                    card_button("Continue", Message::Continue),
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

fn control_style(on: bool, status: button::Status, radius: f32) -> button::Style {
    button::Style {
        background: if on {
            Some(Background::Color(ACCENT))
        } else if status == button::Status::Hovered {
            Some(Background::Color(Color::from_rgb8(0x2e, 0x30, 0x38)))
        } else {
            None
        },
        text_color: if on { ON_ACCENT } else { FG },
        border: Border {
            radius: radius.into(),
            width: 1.0,
            color: if on { ACCENT } else { LINE },
        },
        ..button::Style::default()
    }
}

fn action(label: impl Into<String>, message: Message) -> Element<'static, Message> {
    button(text(label.into()).size(14).color(FG))
        .on_press(message)
        .padding([8, 12])
        .style(|_, status| control_style(false, status, 8.0))
        .into()
}

fn wide(label: impl Into<String>, message: Message) -> Element<'static, Message> {
    button(text(label.into()).size(14).color(FG))
        .on_press(message)
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_, status| control_style(false, status, 8.0))
        .into()
}

fn marked(label: impl Into<String>, on: bool, message: Message) -> Element<'static, Message> {
    nav_button(label, on, message, Length::Fill)
}

fn bar_item(label: impl Into<String>, on: bool, message: Message) -> Element<'static, Message> {
    nav_button(label, on, message, Length::Shrink)
}

fn nav_button(
    label: impl Into<String>,
    on: bool,
    message: Message,
    width: Length,
) -> Element<'static, Message> {
    button(
        text(label.into())
            .size(14)
            .color(if on { ON_ACCENT } else { FG }),
    )
    .on_press(message)
    .width(width)
    .padding([8, 12])
    .style(move |_, status| control_style(on, status, 8.0))
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
    button(text(label).size(14).color(if on { ON_ACCENT } else { FG }))
        .on_press(message)
        .style(move |_, status| control_style(on, status, 10.0))
        .into()
}

fn quest_card(inner: Element<'_, Message>) -> Element<'_, Message> {
    container(inner)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(22)
        .style(|_| container::Style {
            background: Some(Background::Color(BG)),
            text_color: Some(FG),
            border: Border {
                color: LINE,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn card_button<'a>(label: &'a str, message: Message) -> Element<'a, Message> {
    button(text(label).size(16).color(FG))
        .on_press(message)
        .width(Length::Fill)
        .padding([8, 12])
        .style(|_, status| control_style(false, status, 12.0))
        .into()
}

fn choice_row(label: String, on: bool, message: Message) -> Element<'static, Message> {
    button(text(label).size(16).color(if on { ON_ACCENT } else { FG }))
        .on_press(message)
        .width(Length::Fill)
        .padding(10)
        .style(move |_, status| control_style(on, status, 14.0))
        .into()
}

fn data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("retention"));
    }
    std::env::var_os("HOME")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(".local/share/retention"))
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

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| i64::try_from(dur.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_and_local_minute() {
        assert_eq!(parse_offset("+0500"), Some(5 * 3600));
        assert_eq!(parse_offset("-0530"), Some(-(5 * 3600 + 30 * 60)));
        assert_eq!(
            review::local_minute(0, 5 * 3600).map(session::Minute::get),
            Some(300)
        );
        assert_eq!(
            review::local_minute(0, -5 * 3600).map(session::Minute::get),
            Some(19 * 60)
        );
    }

    #[test]
    fn pid_file_ignores_self_and_a_dead_process() {
        assert!(pid_busy(Some(4), 9, |_| true));
        assert!(!pid_busy(Some(9), 9, |_| true));
        assert!(!pid_busy(Some(4), 9, |_| false));
        assert_eq!(parse_pid("42\n"), Some(42));
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
}
