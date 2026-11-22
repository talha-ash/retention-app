//! The desk window. The tray icon stays on the bar while this process is running.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Desk<Id> {
    phase: Phase<Id>,
    shell: Option<Id>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase<Id> {
    /// Process is up. The desk window has not been requested.
    Absent,
    /// XDG window id is reserved. The window is not shown yet.
    Opening(Id),
    Shown(Id),
    /// Window is gone. The tray icon is the only desk control.
    Hidden,
}

impl<Id: Copy> Desk<Id> {
    pub fn absent() -> Self {
        Self {
            phase: Phase::Absent,
            shell: None,
        }
    }

    pub fn phase(self) -> Phase<Id> {
        self.phase
    }

    pub fn shell(self) -> Option<Id> {
        self.shell
    }

    pub fn drag_id(self) -> Option<Id> {
        match self.phase {
            Phase::Shown(id) => Some(id),
            _ => None,
        }
    }

    pub fn take_shell(self) -> (Self, Option<Id>) {
        (
            Self {
                phase: self.phase,
                shell: None,
            },
            self.shell,
        )
    }
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
    /// The startup layer was seen. Store its id once.
    ShellSeen(Id),
    /// The XDG window id is reserved. Only Absent and Hidden become Opening.
    Reserved(Id),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeskEffect<Id> {
    OpenWindow,
    CloseWindow(Id),
    RemoveShell(Id),
    Exit,
    /// The tray is down. Do not hide the window.
    Refuse,
}

pub fn step<Id: Copy + Eq>(
    mut desk: Desk<Id>,
    input: DeskInput<Id>,
    tray_ready: bool,
) -> (Desk<Id>, Vec<DeskEffect<Id>>) {
    let mut effects = Vec::new();
    desk.phase = match (desk.phase, input) {
        (Phase::Absent, DeskInput::ShellReady | DeskInput::Foreground) => {
            effects.push(DeskEffect::OpenWindow);
            Phase::Absent
        }
        (Phase::Absent | Phase::Opening(_), DeskInput::WindowOpened(id)) => {
            if let Some(shell) = desk.shell.take() {
                effects.push(DeskEffect::RemoveShell(shell));
            }
            Phase::Shown(id)
        }
        (Phase::Shown(id), DeskInput::WindowOpened(opened)) if id == opened => {
            if let Some(shell) = desk.shell.take() {
                effects.push(DeskEffect::RemoveShell(shell));
            }
            Phase::Shown(id)
        }
        (Phase::Shown(id), DeskInput::Background) if tray_ready => {
            effects.push(DeskEffect::CloseWindow(id));
            Phase::Hidden
        }
        (Phase::Shown(id), DeskInput::Background) => {
            effects.push(DeskEffect::Refuse);
            Phase::Shown(id)
        }
        (Phase::Hidden, DeskInput::Background) => Phase::Hidden,
        (Phase::Hidden, DeskInput::Foreground) => {
            effects.push(DeskEffect::OpenWindow);
            Phase::Absent
        }
        (Phase::Shown(id), DeskInput::Foreground) => Phase::Shown(id),
        (Phase::Opening(id), DeskInput::Foreground) => Phase::Opening(id),
        (Phase::Shown(id), DeskInput::Close(closed)) if id == closed => {
            effects.push(DeskEffect::Exit);
            Phase::Shown(id)
        }
        (Phase::Hidden, DeskInput::Close(_)) => Phase::Hidden,
        (Phase::Opening(open), DeskInput::Close(closed)) if open == closed => Phase::Absent,
        (phase, DeskInput::ShellSeen(id)) => {
            if desk.shell.is_none() {
                desk.shell = Some(id);
            }
            phase
        }
        (Phase::Absent | Phase::Hidden, DeskInput::Reserved(id)) => Phase::Opening(id),
        (phase, _) => phase,
    };
    (desk, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(phase: Phase<u32>) -> Desk<u32> {
        Desk { phase, shell: None }
    }

    fn expect(
        desk: Desk<u32>,
        input: DeskInput<u32>,
        tray_ready: bool,
        next: Phase<u32>,
        effects: &[DeskEffect<u32>],
    ) {
        let (got, got_effects) = step(desk, input, tray_ready);
        assert_eq!(got.phase(), next);
        assert_eq!(got.shell(), desk.shell());
        assert_eq!(got_effects.as_slice(), effects);
    }

    #[test]
    fn absent_shell_ready_asks_for_a_window() {
        expect(
            Desk::absent(),
            DeskInput::ShellReady,
            true,
            Phase::Absent,
            &[DeskEffect::OpenWindow],
        );
    }

    #[test]
    fn window_opened_shows_the_desk() {
        expect(
            Desk::absent(),
            DeskInput::WindowOpened(4),
            true,
            Phase::Shown(4),
            &[],
        );
        expect(
            at(Phase::Opening(1)),
            DeskInput::WindowOpened(4),
            false,
            Phase::Shown(4),
            &[],
        );
    }

    #[test]
    fn a_remembered_shell_is_removed_when_the_desk_opens() {
        let (seen, effects) = step(Desk::absent(), DeskInput::ShellSeen(7), true);
        assert!(effects.is_empty());
        assert_eq!(seen.shell(), Some(7));
        let (opened, effects) = step(seen, DeskInput::WindowOpened(4), true);
        assert_eq!(opened.phase(), Phase::Shown(4));
        assert_eq!(opened.shell(), None);
        assert_eq!(effects, vec![DeskEffect::RemoveShell(7)]);
        let (again, _) = step(opened, DeskInput::ShellSeen(8), true);
        let (again, effects) = step(again, DeskInput::WindowOpened(4), true);
        assert_eq!(again.phase(), Phase::Shown(4));
        assert_eq!(effects, vec![DeskEffect::RemoveShell(8)]);
    }

    #[test]
    fn reserved_opens_only_from_absent_or_hidden() {
        let (reserved, effects) = step(Desk::absent(), DeskInput::Reserved(3), true);
        assert_eq!(reserved.phase(), Phase::Opening(3));
        assert!(effects.is_empty());
        let (stays, _) = step(at(Phase::Shown(4)), DeskInput::Reserved(9), true);
        assert_eq!(stays.phase(), Phase::Shown(4));
        let (hidden, _) = step(at(Phase::Hidden), DeskInput::Reserved(2), true);
        assert_eq!(hidden.phase(), Phase::Opening(2));
    }

    #[test]
    fn drag_id_is_the_shown_window() {
        assert_eq!(at(Phase::Shown(4)).drag_id(), Some(4));
        assert_eq!(Desk::<u32>::absent().drag_id(), None);
        assert_eq!(at(Phase::Opening(4)).drag_id(), None);
        assert_eq!(at(Phase::Hidden).drag_id(), None);
    }

    #[test]
    fn background_hides_only_when_the_tray_is_ready() {
        expect(
            at(Phase::Shown(4)),
            DeskInput::Background,
            true,
            Phase::Hidden,
            &[DeskEffect::CloseWindow(4)],
        );
        expect(
            at(Phase::Shown(4)),
            DeskInput::Background,
            false,
            Phase::Shown(4),
            &[DeskEffect::Refuse],
        );
    }

    #[test]
    fn background_while_hidden_is_a_noop() {
        expect(at(Phase::Hidden), DeskInput::Background, true, Phase::Hidden, &[]);
    }

    #[test]
    fn foreground_from_hidden_opens_and_is_not_shown_yet() {
        let (next, effects) = step(at(Phase::Hidden), DeskInput::Foreground, true);
        assert_eq!(next.phase(), Phase::Absent);
        assert_eq!(effects, vec![DeskEffect::OpenWindow]);
        assert_eq!(next.drag_id(), None);
    }

    #[test]
    fn foreground_from_absent_opens_too() {
        expect(
            Desk::absent(),
            DeskInput::Foreground,
            true,
            Phase::Absent,
            &[DeskEffect::OpenWindow],
        );
    }

    #[test]
    fn foreground_while_shown_or_opening_is_a_noop() {
        expect(
            at(Phase::Shown(4)),
            DeskInput::Foreground,
            true,
            Phase::Shown(4),
            &[],
        );
        expect(
            at(Phase::Opening(4)),
            DeskInput::Foreground,
            false,
            Phase::Opening(4),
            &[],
        );
    }

    #[test]
    fn close_on_the_shown_desk_exits_and_stays_shown() {
        expect(
            at(Phase::Shown(4)),
            DeskInput::Close(4),
            true,
            Phase::Shown(4),
            &[DeskEffect::Exit],
        );
    }

    #[test]
    fn close_while_hidden_is_the_close_we_started() {
        expect(at(Phase::Hidden), DeskInput::Close(4), true, Phase::Hidden, &[]);
        expect(at(Phase::Hidden), DeskInput::Close(9), false, Phase::Hidden, &[]);
    }

    #[test]
    fn close_while_opening_drops_the_window_without_exiting() {
        expect(
            at(Phase::Opening(4)),
            DeskInput::Close(4),
            true,
            Phase::Absent,
            &[],
        );
    }

    #[test]
    fn other_pairs_do_nothing() {
        expect(Desk::absent(), DeskInput::Background, true, Phase::Absent, &[]);
        expect(Desk::absent(), DeskInput::Close(1), true, Phase::Absent, &[]);
        expect(
            at(Phase::Opening(4)),
            DeskInput::ShellReady,
            true,
            Phase::Opening(4),
            &[],
        );
        expect(
            at(Phase::Opening(4)),
            DeskInput::Background,
            true,
            Phase::Opening(4),
            &[],
        );
        expect(
            at(Phase::Opening(4)),
            DeskInput::Close(9),
            true,
            Phase::Opening(4),
            &[],
        );
        expect(
            at(Phase::Shown(4)),
            DeskInput::ShellReady,
            true,
            Phase::Shown(4),
            &[],
        );
        expect(
            at(Phase::Shown(4)),
            DeskInput::Close(9),
            false,
            Phase::Shown(4),
            &[],
        );
        expect(
            at(Phase::Shown(4)),
            DeskInput::WindowOpened(9),
            true,
            Phase::Shown(4),
            &[],
        );
        expect(at(Phase::Hidden), DeskInput::ShellReady, true, Phase::Hidden, &[]);
        expect(
            at(Phase::Hidden),
            DeskInput::WindowOpened(4),
            true,
            Phase::Hidden,
            &[],
        );
    }
}
