//! StatusNotifierItem for the Omarchy top bar. The tray thread never sees [`crate::desk::Desk`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use iced::Subscription;
use iced::futures::channel::mpsc::{self, UnboundedReceiver, UnboundedSender};
use iced::futures::{SinkExt, Stream};
use ksni::blocking::TrayMethods;
use ksni::menu::StandardItem;
use ksni::{Category, MenuItem, Status, ToolTip, Tray};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Open,
    Quit,
}

#[derive(Clone)]
pub struct TrayHandle {
    shared: Arc<Shared>,
}

struct Shared {
    ready: AtomicBool,
    visible: Mutex<bool>,
    commands: std::sync::mpsc::Sender<bool>,
    events: Mutex<Option<UnboundedReceiver<TrayEvent>>>,
}

static SHARED: OnceLock<Arc<Shared>> = OnceLock::new();

impl TrayHandle {
    pub fn start() -> Self {
        let shared = SHARED.get_or_init(spawn_shared).clone();
        Self { shared }
    }

    pub fn ready(&self) -> bool {
        self.shared.ready.load(Ordering::Acquire)
    }

    pub fn set_visible(&self, visible: bool) {
        {
            let mut current = self.shared.visible.lock().expect("tray visibility");
            if *current == visible {
                return;
            }
            *current = visible;
        }
        if self.ready() {
            let _ = self.shared.commands.send(visible);
        }
    }
}

pub fn subscription() -> Subscription<TrayEvent> {
    Subscription::run(event_stream)
}

fn event_stream() -> impl Stream<Item = TrayEvent> + Send + 'static {
    iced::stream::channel(8, async |mut sender| {
        let events = SHARED
            .get()
            .and_then(|shared| shared.events.lock().expect("tray events").take());
        let Some(mut events) = events else {
            std::future::pending::<()>().await;
            return;
        };
        while let Some(event) = iced::futures::StreamExt::next(&mut events).await {
            if sender.send(event).await.is_err() {
                break;
            }
        }
        std::future::pending::<()>().await;
    })
}

fn spawn_shared() -> Arc<Shared> {
    let (commands, command_rx) = std::sync::mpsc::channel();
    let (events_tx, events_rx) = mpsc::unbounded();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let shared = Arc::new(Shared {
        ready: AtomicBool::new(false),
        visible: Mutex::new(false),
        commands,
        events: Mutex::new(Some(events_rx)),
    });
    let thread_shared = Arc::clone(&shared);
    let spawned = std::thread::Builder::new()
        .name("retention-tray".into())
        .spawn(move || serve(thread_shared, events_tx, command_rx, started_tx));
    if spawned.is_ok() {
        let _ = started_rx.recv().unwrap_or(false);
    }
    shared
}

fn serve(
    shared: Arc<Shared>,
    events: UnboundedSender<TrayEvent>,
    commands: std::sync::mpsc::Receiver<bool>,
    started: std::sync::mpsc::Sender<bool>,
) {
    let visible = *shared.visible.lock().expect("tray visibility");
    let handle = match (RetentionTray { events, visible }).spawn() {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("tray spawn failed: {err}");
            let _ = started.send(false);
            return;
        }
    };
    shared.ready.store(true, Ordering::Release);
    let _ = started.send(true);
    while let Ok(visible) = commands.recv() {
        let _ = handle.update(|tray| tray.visible = visible);
    }
}

struct RetentionTray {
    events: UnboundedSender<TrayEvent>,
    visible: bool,
}

impl RetentionTray {
    fn emit(&mut self, event: TrayEvent) {
        let _ = self.events.unbounded_send(event);
    }
}

impl Tray for RetentionTray {
    fn id(&self) -> String {
        "retention-popup".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.emit(TrayEvent::Open);
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn title(&self) -> String {
        "Retention".into()
    }

    fn status(&self) -> Status {
        if self.visible {
            Status::Active
        } else {
            Status::Passive
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![disk()]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "Retention is in the background".into(),
            ..ToolTip::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Open".into(),
                activate: Box::new(|tray: &mut Self| tray.emit(TrayEvent::Open)),
                ..StandardItem::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| tray.emit(TrayEvent::Quit)),
                ..StandardItem::default()
            }
            .into(),
        ]
    }
}

fn disk() -> ksni::Icon {
    const SIZE: i32 = 22;
    let mut data = vec![0; (SIZE * SIZE * 4) as usize];
    let radius = SIZE as f32 / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 + 0.5 - radius;
            let dy = y as f32 + 0.5 - radius;
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let at = ((y * SIZE + x) * 4) as usize;
            // ARGB32, network byte order. ksni's Icon example rotates RGBA to this order.
            data[at] = 255;
            data[at + 1] = 110;
            data[at + 2] = 158;
            data[at + 3] = 235;
        }
    }
    ksni::Icon {
        width: SIZE,
        height: SIZE,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &ksni::Icon, x: i32, y: i32) -> [u8; 4] {
        let at = ((y * icon.width + x) * 4) as usize;
        icon.data[at..at + 4].try_into().expect("pixel")
    }

    #[test]
    fn disk_is_opaque_inside_and_clear_outside() {
        let icon = disk();
        assert_eq!((icon.width, icon.height), (22, 22));
        assert_eq!(icon.data.len(), 22 * 22 * 4);
        assert_eq!(pixel(&icon, 11, 11), [255, 110, 158, 235]);
        assert_eq!(pixel(&icon, 10, 10), [255, 110, 158, 235]);
        assert_eq!(pixel(&icon, 0, 11), [255, 110, 158, 235]);
        assert_eq!(pixel(&icon, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&icon, 21, 21), [0, 0, 0, 0]);
    }

    #[test]
    fn left_click_opens_and_the_menu_quits() {
        let (tx, mut rx) = mpsc::unbounded();
        let mut tray = RetentionTray {
            events: tx,
            visible: false,
        };
        assert_eq!(tray.id(), "retention-popup");
        assert_eq!(tray.title(), "Retention");
        assert_eq!(tray.category(), Category::ApplicationStatus);
        assert_eq!(tray.status(), Status::Passive);
        assert_eq!(tray.tool_tip().title, "Retention is in the background");
        assert_eq!(tray.icon_pixmap().len(), 1);

        tray.activate(0, 0);
        assert_eq!(rx.try_recv(), Ok(TrayEvent::Open));

        let mut menu = tray.menu();
        let MenuItem::Standard(open) = menu.remove(0) else {
            panic!("open item");
        };
        let MenuItem::Standard(quit) = menu.remove(0) else {
            panic!("quit item");
        };
        assert_eq!(open.label, "Open");
        assert_eq!(quit.label, "Quit");
        assert!(menu.is_empty());

        (open.activate)(&mut tray);
        (quit.activate)(&mut tray);
        assert_eq!(rx.try_recv(), Ok(TrayEvent::Open));
        assert_eq!(rx.try_recv(), Ok(TrayEvent::Quit));
        assert!(rx.try_recv().is_err());

        tray.visible = true;
        assert_eq!(tray.status(), Status::Active);
    }
}
