# Close the card

Close the card removes the quest card and leaves the control card and the process running.

## Sub-features

- `close-button` unmaps the quest card from the × control.
- `close-process` leaves `retention-controls` in place and the recorded process alive.
- `close-idle` returns the status line to the idle sentence.

## How to get to it (user POV)

- Open the quest card, then press `×` on that card.

## Driving it with verify-popup

Preconditions:

- Doctor reports `controls 1` and the recorded process group.
- A `retention-card` layer is up.

- **Close.** Press `×`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "×"`. If OCR reads that glyph as `x`, the recipe has not found the control: stop and report the OCR text. Do not click a nearby coordinate instead.
- **Gone.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 3 --absent`.
- **Still running.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup doctor`. It still prints `ok` and `controls 1`. The status line reads `Pick a delay and a corner, then show the card.`
- **Proof.** A screenshot of the control card after the wait shows that idle sentence, and `hyprctl` no longer lists `retention-card` for this process.

## Gotchas

- Quit is a different control on the other card. Quit ends the process. × does not.
- The × sits on the gold header, inside the haze padding. Click the glyph's OCR box.
- Show card also removes an open quest card, and it starts a new countdown. That is the Show card recipe, not this one.
