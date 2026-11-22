//! Which window system draws the card. Review, Editor, and Desk do not read this.

use crate::review::Corner;

pub const LIBRARY: (i32, i32) = (960, 680);
pub const CARD: (i32, i32) = (460, 360);
pub const MARGIN: i32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    #[cfg(target_os = "linux")]
    Layer,
    Window(Seat),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seat {
    LibraryCorner,
    ScreenPanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Force {
    #[cfg(target_os = "linux")]
    Layer,
    Window,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub forced: Option<Force>,
    pub wayland: bool,
    pub layer_shell: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Start,
    End,
}

pub fn force_from(value: Option<&str>) -> Option<Force> {
    match value {
        #[cfg(target_os = "linux")]
        Some("layer") => Some(Force::Layer),
        Some("window") => Some(Force::Window),
        _ => None,
    }
}

pub fn choose(facts: Facts) -> Host {
    if let Some(Force::Window) = facts.forced {
        return window_seat(facts.wayland);
    }
    #[cfg(target_os = "linux")]
    if facts.forced == Some(Force::Layer) || (facts.wayland && facts.layer_shell) {
        return Host::Layer;
    }
    window_seat(facts.wayland)
}

fn window_seat(wayland: bool) -> Host {
    if wayland {
        Host::Window(Seat::LibraryCorner)
    } else {
        Host::Window(Seat::ScreenPanel)
    }
}

pub fn corner_origin(corner: Corner, monitor: (i32, i32)) -> (i32, i32) {
    let (width, height) = monitor;
    let (card_w, card_h) = CARD;
    match corner {
        Corner::TopLeft => (MARGIN, MARGIN),
        Corner::TopRight => (width - card_w - MARGIN, MARGIN),
        Corner::BottomLeft => (MARGIN, height - card_h - MARGIN),
        Corner::BottomRight => (width - card_w - MARGIN, height - card_h - MARGIN),
    }
}

pub fn corner_align(corner: Corner) -> (Side, Side) {
    match corner {
        Corner::TopLeft => (Side::Start, Side::Start),
        Corner::TopRight => (Side::End, Side::Start),
        Corner::BottomLeft => (Side::Start, Side::End),
        Corner::BottomRight => (Side::End, Side::End),
    }
}

pub fn detect() -> Facts {
    let forced = std::env::var("RETENTION_HOST")
        .ok()
        .as_deref()
        .and_then(|value| force_from(Some(value)));
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty());
    Facts {
        forced,
        wayland,
        layer_shell: wayland && layer_shell_probe().unwrap_or(false),
    }
}

/// `None` when this process cannot open a Wayland socket. `Some` is whether
/// the compositor listed `zwlr_layer_shell_v1`. The probe does not bind it.
pub fn layer_shell_probe() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        probe_linux()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn probe_linux() -> Option<bool> {
    use wayland_client::protocol::wl_registry;
    use wayland_client::{Connection, Dispatch, QueueHandle};

    struct Seen(bool);

    impl Dispatch<wl_registry::WlRegistry, ()> for Seen {
        fn event(
            state: &mut Self,
            _: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { interface, .. } = event {
                if interface == "zwlr_layer_shell_v1" {
                    state.0 = true;
                }
            }
        }
    }

    let conn = Connection::connect_to_env().ok()?;
    let display = conn.display();
    let mut queue = conn.new_event_queue();
    let handle = queue.handle();
    let _registry = display.get_registry(&handle, ());
    let mut seen = Seen(false);
    queue.roundtrip(&mut seen).ok()?;
    Some(seen.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(forced: Option<Force>, wayland: bool, layer_shell: bool) -> Facts {
        Facts {
            forced,
            wayland,
            layer_shell,
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn an_empty_force_on_wayland_with_the_global_is_the_layer() {
        assert_eq!(choose(facts(None, true, true)), Host::Layer);
    }

    #[test]
    fn wayland_without_the_global_draws_inside_the_library() {
        assert_eq!(
            choose(facts(None, true, false)),
            Host::Window(Seat::LibraryCorner)
        );
    }

    #[test]
    fn no_wayland_is_a_screen_panel() {
        assert_eq!(
            choose(facts(None, false, false)),
            Host::Window(Seat::ScreenPanel)
        );
    }

    #[test]
    fn forcing_a_window_on_wayland_stays_inside_the_library() {
        assert_eq!(
            choose(facts(Some(Force::Window), true, true)),
            Host::Window(Seat::LibraryCorner)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn forcing_the_layer_selects_the_layer() {
        assert_eq!(force_from(Some("layer")), Some(Force::Layer));
        assert_eq!(choose(facts(Some(Force::Layer), false, false)), Host::Layer);
    }

    #[test]
    fn other_force_text_is_ignored() {
        assert_eq!(force_from(Some("layer ")), None);
        assert_eq!(force_from(Some("")), None);
        assert_eq!(force_from(None), None);
        assert_eq!(force_from(Some("window")), Some(Force::Window));
    }

    #[test]
    fn corners_on_a_1080p_monitor() {
        let monitor = (1920, 1080);
        assert_eq!(corner_origin(Corner::TopLeft, monitor), (16, 16));
        assert_eq!(corner_origin(Corner::TopRight, monitor), (1444, 16));
        assert_eq!(corner_origin(Corner::BottomLeft, monitor), (16, 704));
        assert_eq!(corner_origin(Corner::BottomRight, monitor), (1444, 704));
    }

    #[test]
    fn alignment_follows_the_corner() {
        assert_eq!(
            corner_align(Corner::TopLeft),
            (Side::Start, Side::Start)
        );
        assert_eq!(
            corner_align(Corner::TopRight),
            (Side::End, Side::Start)
        );
        assert_eq!(
            corner_align(Corner::BottomLeft),
            (Side::Start, Side::End)
        );
        assert_eq!(
            corner_align(Corner::BottomRight),
            (Side::End, Side::End)
        );
    }

    #[test]
    fn the_card_and_the_library_keep_their_sizes() {
        assert_eq!(LIBRARY, (960, 680));
        assert_eq!(CARD, (460, 360));
        assert_eq!(MARGIN, 16);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn this_process_follows_the_compositor() {
        if std::env::var_os("RETENTION_HOST").is_some() {
            return;
        }
        let found = detect();
        if found.wayland && found.layer_shell {
            assert_eq!(choose(found), Host::Layer);
        }
    }

    #[test]
    fn this_session_lists_layer_shell_when_the_socket_answers() {
        match layer_shell_probe() {
            None => {}
            Some(found) => assert!(found),
        }
    }
}
