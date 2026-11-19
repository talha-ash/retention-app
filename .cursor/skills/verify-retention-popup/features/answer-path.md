# Answer a path

Answer a path locks one of the two quest choices. The footer names the path. The quest card stays open.

## Sub-features

- `answer-yes` locks the footer to `right corner`.
- `answer-no` locks the footer to `wrong place`.
- `answer-switch` replaces the locked path when the other choice is pressed.
- `answer-stays` leaves the quest card mapped after either press.

## How to get to it (user POV)

- Open the quest card, then press `Yes. This corner.` or `No. Wrong spot.`

## Driving it with verify-popup

Preconditions:

- Doctor reports `controls 1`.
- A `retention-card` layer is up. Reach it with the Show card recipe first. The fresh process does not have a quest card.

- **Yes.** Press `Yes. This corner.` Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "Yes. This corner."`. The footer reads `Path locked  ·  right corner`. The `retention-card` layer is still present.
- **No.** Press `No. Wrong spot.` Run `.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "No. Wrong spot."`. The footer reads `Path locked  ·  wrong place`. The quest card stays open.
- **Proof.** Save a screenshot and OCR after each press. The second image must show `wrong place` and must not be the only artifact: the first image has to show `right corner` as well. A layer list taken after each press still contains `retention-card`.

## Gotchas

- The choice titles are `Yes. This corner.` and `No. Wrong spot.` The footer words are `right corner` and `wrong place`. Assert the footer, not only the button that was pressed.
- The buttons are not available on the control card. Clicking those phrases on `controls` fails the precondition.
- Closing the card does not clear the locked path inside the process, but the path is only visible on the quest card. The next Show card clears it before the new countdown ends.
