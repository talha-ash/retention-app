//! The desk window. The tray icon is shown only while the desk is hidden.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Desk<Id> {
    /// Process is up. The desk window has not been requested.
    Absent,
    /// XDG window id is reserved. The window is not shown yet.
    Opening(Id),
    Shown(Id),
    /// Window is gone. The tray icon is the only desk control.
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeskInput<Id> {
    /// The startup layer opened.
    ShellReady,
    WindowOpened(Id),
    Background,
    /// Tray Open or a left click.
    Foreground,
    /// CloseRequested.
    Close(Id),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeskEffect<Id> {
    OpenWindow,
    CloseWindow(Id),
    RemoveShell,
    Exit,
    /// The tray is down. Do not hide the window.
    Refuse,
}

pub fn step<Id: Copy + Eq>(
    desk: Desk<Id>,
    input: DeskInput<Id>,
    tray_ready: bool,
) -> (Desk<Id>, Vec<DeskEffect<Id>>) {
    match (desk, input) {
        (Desk::Absent, DeskInput::ShellReady | DeskInput::Foreground) => {
            (Desk::Absent, vec![DeskEffect::OpenWindow])
        }
        (Desk::Absent | Desk::Opening(_), DeskInput::WindowOpened(id)) => {
            (Desk::Shown(id), vec![DeskEffect::RemoveShell])
        }
        (Desk::Shown(id), DeskInput::WindowOpened(opened)) if id == opened => {
            (Desk::Shown(id), vec![DeskEffect::RemoveShell])
        }
        (Desk::Shown(id), DeskInput::Background) if tray_ready => {
            (Desk::Hidden, vec![DeskEffect::CloseWindow(id)])
        }
        (Desk::Shown(id), DeskInput::Background) => (Desk::Shown(id), vec![DeskEffect::Refuse]),
        (Desk::Hidden, DeskInput::Background) => (Desk::Hidden, Vec::new()),
        (Desk::Hidden, DeskInput::Foreground) => (Desk::Absent, vec![DeskEffect::OpenWindow]),
        (Desk::Shown(id), DeskInput::Foreground) => (Desk::Shown(id), Vec::new()),
        (Desk::Opening(id), DeskInput::Foreground) => (Desk::Opening(id), Vec::new()),
        (Desk::Shown(id), DeskInput::Close(closed)) if id == closed => {
            (Desk::Shown(id), vec![DeskEffect::Exit])
        }
        (Desk::Hidden, DeskInput::Close(_)) => (Desk::Hidden, Vec::new()),
        (Desk::Opening(id), DeskInput::Close(closed)) if id == closed => (Desk::Absent, Vec::new()),
        (desk, _) => (desk, Vec::new()),
    }
}

pub fn tray_visible<Id>(desk: &Desk<Id>) -> bool {
    matches!(desk, Desk::Hidden)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect(
        desk: Desk<u32>,
        input: DeskInput<u32>,
        tray_ready: bool,
        next: Desk<u32>,
        effects: &[DeskEffect<u32>],
    ) {
        let (got, got_effects) = step(desk, input, tray_ready);
        assert_eq!(got, next);
        assert_eq!(got_effects.as_slice(), effects);
    }

    #[test]
    fn absent_shell_ready_asks_for_a_window() {
        expect(
            Desk::Absent,
            DeskInput::ShellReady,
            true,
            Desk::Absent,
            &[DeskEffect::OpenWindow],
        );
    }

    #[test]
    fn window_opened_shows_the_desk_and_drops_the_shell() {
        expect(
            Desk::Absent,
            DeskInput::WindowOpened(4),
            true,
            Desk::Shown(4),
            &[DeskEffect::RemoveShell],
        );
        expect(
            Desk::Opening(1),
            DeskInput::WindowOpened(4),
            false,
            Desk::Shown(4),
            &[DeskEffect::RemoveShell],
        );
        expect(
            Desk::Opening(4),
            DeskInput::WindowOpened(4),
            true,
            Desk::Shown(4),
            &[DeskEffect::RemoveShell],
        );
    }

    #[test]
    fn shown_window_opened_again_still_drops_the_shell() {
        expect(
            Desk::Shown(4),
            DeskInput::WindowOpened(4),
            true,
            Desk::Shown(4),
            &[DeskEffect::RemoveShell],
        );
        expect(
            Desk::Shown(4),
            DeskInput::WindowOpened(4),
            true,
            Desk::Shown(4),
            &[DeskEffect::RemoveShell],
        );
    }

    #[test]
    fn background_hides_only_when_the_tray_is_ready() {
        expect(
            Desk::Shown(4),
            DeskInput::Background,
            true,
            Desk::Hidden,
            &[DeskEffect::CloseWindow(4)],
        );
        expect(
            Desk::Shown(4),
            DeskInput::Background,
            false,
            Desk::Shown(4),
            &[DeskEffect::Refuse],
        );
    }

    #[test]
    fn background_while_hidden_is_a_noop() {
        expect(Desk::Hidden, DeskInput::Background, true, Desk::Hidden, &[]);
        expect(Desk::Hidden, DeskInput::Background, true, Desk::Hidden, &[]);
    }

    #[test]
    fn foreground_from_hidden_opens_and_is_not_shown_yet() {
        let (next, effects) = step(Desk::<u32>::Hidden, DeskInput::Foreground, true);
        assert_eq!(next, Desk::Absent);
        assert_eq!(effects, vec![DeskEffect::OpenWindow]);
        assert!(!matches!(next, Desk::Shown(_)));
    }

    #[test]
    fn foreground_from_absent_opens_too() {
        expect(
            Desk::Absent,
            DeskInput::Foreground,
            true,
            Desk::Absent,
            &[DeskEffect::OpenWindow],
        );
    }

    #[test]
    fn foreground_while_shown_or_opening_is_a_noop() {
        expect(
            Desk::Shown(4),
            DeskInput::Foreground,
            true,
            Desk::Shown(4),
            &[],
        );
        expect(
            Desk::Shown(4),
            DeskInput::Foreground,
            true,
            Desk::Shown(4),
            &[],
        );
        expect(
            Desk::Opening(4),
            DeskInput::Foreground,
            false,
            Desk::Opening(4),
            &[],
        );
        expect(
            Desk::Opening(4),
            DeskInput::Foreground,
            false,
            Desk::Opening(4),
            &[],
        );
    }

    #[test]
    fn close_on_the_shown_desk_exits_and_stays_shown() {
        expect(
            Desk::Shown(4),
            DeskInput::Close(4),
            true,
            Desk::Shown(4),
            &[DeskEffect::Exit],
        );
    }

    #[test]
    fn close_while_hidden_is_the_close_we_started() {
        expect(Desk::Hidden, DeskInput::Close(4), true, Desk::Hidden, &[]);
        expect(Desk::Hidden, DeskInput::Close(9), false, Desk::Hidden, &[]);
    }

    #[test]
    fn close_while_opening_drops_the_window_without_exiting() {
        expect(
            Desk::Opening(4),
            DeskInput::Close(4),
            true,
            Desk::Absent,
            &[],
        );
    }

    #[test]
    fn other_pairs_do_nothing() {
        expect(Desk::Absent, DeskInput::Background, true, Desk::Absent, &[]);
        expect(Desk::Absent, DeskInput::Close(1), true, Desk::Absent, &[]);
        expect(
            Desk::Opening(4),
            DeskInput::ShellReady,
            true,
            Desk::Opening(4),
            &[],
        );
        expect(
            Desk::Opening(4),
            DeskInput::Background,
            true,
            Desk::Opening(4),
            &[],
        );
        expect(
            Desk::Opening(4),
            DeskInput::Close(9),
            true,
            Desk::Opening(4),
            &[],
        );
        expect(
            Desk::Shown(4),
            DeskInput::ShellReady,
            true,
            Desk::Shown(4),
            &[],
        );
        expect(
            Desk::Shown(4),
            DeskInput::Close(9),
            false,
            Desk::Shown(4),
            &[],
        );
        expect(
            Desk::Shown(4),
            DeskInput::WindowOpened(9),
            true,
            Desk::Shown(4),
            &[],
        );
        expect(Desk::Hidden, DeskInput::ShellReady, true, Desk::Hidden, &[]);
        expect(
            Desk::Hidden,
            DeskInput::WindowOpened(4),
            true,
            Desk::Hidden,
            &[],
        );
    }

    #[test]
    fn tray_is_visible_only_while_hidden() {
        assert!(!tray_visible(&Desk::<u32>::Absent));
        assert!(!tray_visible(&Desk::Opening(1)));
        assert!(!tray_visible(&Desk::Shown(1)));
        assert!(tray_visible(&Desk::<u32>::Hidden));
    }
}
