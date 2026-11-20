# Due card

A due card opens in the selected corner without a button press. Show answer keeps the front on the card and adds the back under it. Again grades it, the card leaves, and that deck stays hidden for its gap.

## Sub-features

- `due-open` maps `retention-card` without pressing Show card.
- `due-front` shows the fixture front on the first face.
- `due-reveal` presses Show answer. The front and the back are both on the card.
- `due-again` presses Again and the card leaves.
- `due-gap` keeps the card absent through the gap, then shows it again.

## How to get to it (user POV)

- Launch the process. Do not press Show card. The corner card opens when a card is due.

## Driving it with verify-popup

Preconditions:

- The fixture root is the only root, or `RETENTION_ROOT` names it.
- `root.json` is `{"start":0,"end":1440,"gap":1,"gap_seconds":5}`.
- The note front is not the demo sentence.
- Doctor reports `controls 1`. The corner card may already be open.

- **Launch.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup launch`. Do not press Show card.
- **Front.** The `retention-card` OCR contains the fixture front. The inner crop is 96 pixels in from the surface edge.
- **Reveal.** Press `Show answer`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "Show answer"`.
- **Grade.** Press `Again`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "Again"`.
- **Gone.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 3 --absent`. It succeeds.
- **Still gone.** Two seconds later the card is still absent.
- **Back.** Eight seconds after the grade the card is present again.

## Gotchas

- There is no Show card button. `prove-show-card` exits because the countdown journey is gone.
- The hold is `gap_seconds` from `root.json`. The `gap` field is minutes and does not set this hold when `gap_seconds` is present.
- A second process exits while the first is alive. Do not launch over a card that is already mapped.
