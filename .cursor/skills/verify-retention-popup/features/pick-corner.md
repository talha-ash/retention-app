# Pick a corner

Pick a corner chooses where the next quest card opens. The control card has four chips. The quest card's sentence names the corner that was selected when the card opened.

## Sub-features

- `corner-chip` selects one of the four corners before the countdown starts.
- `corner-sentence` shows that corner on the quest card.
- `corner-place` puts the quest card center in the matching half of its monitor.

## How to get to it (user POV)

- Press `Top left`, `Top right`, `Bottom left`, or `Bottom right` on the control card, then press `Show card`.

## Driving it with verify-popup

Preconditions:

- Doctor reports `controls 1` and `card 0`.
- The fresh process has `Top right` selected. A recipe that wants a different corner presses that chip first.

- **Choose bottom left.** Press `Bottom left`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Bottom left"`. The chip is drawn selected. The status line has not started a countdown.
- **Arm.** Press `Show card`. Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface controls --text "Show card"`. The status line reads `Card appears in 5s, bottom left`.
- **Open.** Run `.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 12`. OCR of the quest card contains `bottom left`.
- **Place.** The quest card center is in the left half and the bottom half of its monitor. Save the layer rectangle next to the screenshot.
- **Proof.** The armed screenshot shows `bottom left`, and the card screenshot shows the same words. Repeat the same commands with `Top left`, `Top right`, and `Bottom right`, one run at a time from a fresh process. Do not mark an unpressed chip as verified.

## Gotchas

- Pressing a corner chip while a countdown is running changes where the card will open. It does not restart the timer.
- Pressing a corner chip while the quest card is already open does not move that card. The next Show card uses the new corner.
- The four names are two words. The click text includes the space.
- `Top right` is the fresh-process corner. A pass that never presses another chip has not verified the other three.
