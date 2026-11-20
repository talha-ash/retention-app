# Library

The control card creates a root, imports a note file into that root, exports the root, and saves a deck override that names a window and a gap. A new deck is inherit. The four corner chips stay. One imported deck still opens one due card.

## Sub-features

- `library-root` presses New root. The control card shows `r1`.
- `library-import` presses `basic.txt`. The control card shows `Spanish::Verbs inherit`.
- `library-export` presses Export. `$XDG_DATA_HOME/retention/exports/r1.txt` contains `abc123XYZ0`.
- `library-save` presses the deck row, Override, and Save. The row then reads override. At most one `retention-card` stays up.

## How to get to it (user POV)

- Empty the retention dir and copy `fixtures/basic.txt` into the inbox. Launch. Do not press Show card.
- Press `New root`, then `basic.txt`, then `Export`.
- Press the deck row, then `Override`, then `Save`.

## Driving it with verify-popup

Preconditions:

- `$XDG_DATA_HOME/retention` is empty except `inbox/basic.txt`, copied from `fixtures/basic.txt`.
- Doctor reports `controls 1`. Do not press Show card.

- **Launch.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup launch`. Do not press Show card.
- **Root.** Press `New root`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "New root"`. The control card shows `r1`.
- **Import.** Press `basic.txt`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "basic.txt"`. The control card shows `Spanish::Verbs inherit`.
- **Export.** Press `Export`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Export"`. The file `$XDG_DATA_HOME/retention/exports/r1.txt` contains `abc123XYZ0`.
- **Still inherit.** The deck row still says `Spanish::Verbs inherit`. `Override` has not been pressed.
- **Draft.** Press the deck row, then `Override`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Spanish::Verbs inherit"`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Override"`. The card shows `start 540`, `end 1080`, and `gap 45`.
- **Save.** Press `Save`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Save"`. The deck row reads `Spanish::Verbs override`.
- **One card.** Run `hyprctl layers`. One `retention-card` namespace at most is present. The imported note is due, so the corner card may already be up. It stays one card.

## Gotchas

- `Override` does not write. The row stays inherit until `Save`.
- There is no Show card button. Do not press Show card.
- A second process exits while the first is alive. Do not launch over a card that is already mapped.
