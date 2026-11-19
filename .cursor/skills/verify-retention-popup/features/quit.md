# Quit

Quit ends the retention-popup process. Both surfaces go away. This is the only control that exits.

## Sub-features

- `quit-button` ends the process from the control card.
- `quit-surfaces` removes `retention-controls` and `retention-card` if one was open.
- `quit-doctor` makes a later doctor fail because the recorded process is gone.

## How to get to it (user POV)

- Press `Quit` on the control card.

## Driving it with verify-popup

Preconditions:

- Doctor reports `controls 1` for this run.
- Run this recipe last. The process cannot be driven after it exits.

- **Quit.** Press `Quit`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Quit"`.
- **Gone.** Within 3 seconds neither `retention-controls` nor `retention-card` remains for the recorded pid, and that pid is no longer alive.
- **Proof.** Save `hyprctl layers` JSON after the click and the launch log's ending. Doctor then fails because the process group is gone. That failure is the expected end of this recipe, not a broken launch.
- **Cleanup.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup cleanup` so the state file does not point at a dead group. Evidence stays.

## Gotchas

- × on the quest card is not Quit. The process keeps running.
- Killing the process by name is not this recipe. The proof is the Quit button.
- After Quit, do not launch again in the same evidence directory and describe it as one continuous session.
