# Show the card

Show card starts a countdown on the control card and then opens a quest card in the selected corner. Pressing Show card again removes that quest card and starts a new countdown. The control card stays where it is.

## Sub-features

- `show-arm` starts the countdown from the idle control card.
- `show-status` names the delay and the corner while the timer is running.
- `show-open` maps the quest card in that corner after the delay.
- `show-again` removes an open quest card and starts a new countdown. The button still reads Show card.

## How to get to it (user POV)

- Press `Show card` on the control card. There is no keyboard shortcut.

## Driving it with verify-popup

Preconditions:

- Doctor reports `controls 1` and `card 0`.
- The control card shows `5 seconds` and `Top right` already selected, which is the fresh-process state.
- No quest card is visible.

- **Arm.** Press `Show card`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Show card"`. The status line reads `Card appears in 5s, top right`.
- **Capture the countdown.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup shot --surface controls --output .cursor/skills/verify-retention-popup/evidence/<run-id>/controls-armed.png`. OCR of that image contains `Card appears in` and `top right`.
- **Wait for the card.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 12`. A `retention-card` layer exists, and `retention-controls` is still present.
- **Confirm the corner.** The quest card center is in the top-right half of its monitor. OCR of the inner card, inset 96 pixels from the surface edge, contains `slipped`. The corner name is on the control card's status line. The muted sentence on the quest card is too faint for the reader. The screenshot is `card.png` in the run's evidence directory.
- **Show again.** Press `Show card` once more. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Show card"`. Within 3 seconds the `retention-card` layer is gone and the status line reads `Card appears in` again.
- **Proof.** `layers-card.json` shows both namespaces before the second press. `controls-restarted.txt` shows the new countdown. `result.txt` reads `pass show-card` for the bundled run `.cursor/skills/verify-retention-popup/bin/verify-popup prove-show-card`.

## Gotchas

- The button reads `Restart timer` only while the countdown is still running. Once the quest card is open the same button reads `Show card` again.
- Changing the delay chip during a countdown does not change the number already counting down. Only Show card applies a delay.
- A 10 second or 30 second chip makes the wait that long. The fresh process is 5 seconds. Do not raise the timeout and call a shorter wait a pass.
- The quest surface is larger than the gold card because of the haze around it. Click the words.
- Two copies share this session. Doctor must be for the process this run launched.
