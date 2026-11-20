# retention-popup verification map

This directory is the maintained source for verifying the user-facing behavior of retention-popup. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- One Hyprland session. The harness refuses to start when `retention-controls` or `retention-card` is already mapped.
- Launch with `.cursor/skills/verify-retention-popup/bin/verify-popup launch` from the repo root.
- A fresh process starts with Top right selected. A due card may already be open. Do not press Show card. There is no countdown.
- Run `.cursor/skills/verify-retention-popup/bin/verify-popup doctor` and require `controls 1` and the recorded evidence directory. `card` may be 0 or 1.
- Never drive an instance that this verification run did not start.

## Driving conventions

- Start every recipe from the baseline state unless its preconditions say otherwise.
- Click the visible button text. Do not click coordinates chosen by eye.
- Treat every command as literal. Keep quoted labels and flags unchanged.
- Run every pointer and screenshot action through `verify-popup`.
- A due card opens on its own. Wait on the `retention-card` layer. Do not press Show card.
- Cleanup removes the process this run started. It does not remove the evidence directory.

## Proof and skip reporting

- Capture the user action and the resulting state, not only the final screen.
- UI proof includes the screenshot, the OCR text, and the layer list.
- A corner proof includes the layer's monitor and rectangle. The card center has to fall in the half of that monitor the corner names.
- Record the feature id and the entry point with every artifact.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with verify-popup` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Due card](./due-card.md) covers the card that opens when it is due, the answer, and the gap.
- [Show the card](./show-card.md) records that the countdown journey is gone.
- [Pick a corner](./pick-corner.md) covers the four corner chips and the sentence on the quest card.
- [Answer a path](./answer-path.md) covers the two quest choices and the locked-path line.
- [Close the card](./close-card.md) covers the × control leaving the process up.
- [Quit](./quit.md) covers the Quit button ending the process.
