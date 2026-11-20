---
name: verify-retention-popup
description: Drive the retention-popup Wayland layer-shell app the way a user does. Use when a change touches the control card, the corner, or the due card, and the proof has to be the real Hyprland surfaces.
---

# Verify retention-popup

`retention-popup` paints layer-shell surfaces on the Hyprland session. The centered control card is `retention-controls`. A due card opens in the selected corner as `retention-card`. The buttons are mouse buttons. Nothing in the app sends a keyboard shortcut.

The control card reads `Retention`. It shows the four corner chips and Quit. There is no Show card button and no countdown. Launch is ready when a `grim` capture of `retention-controls` OCRs as `Top right`. The corner card may already be open by then. Nobody presses Show card.

This machine has two monitors, DP-1 and DP-3. The app anchors the due card to the active output. The harness aims the pointer at the monitor `hyprctl` reports for the surface being driven.

Do not start a second copy. If either namespace is already present, stop and leave that instance alone. A second process also exits 0 while the first is alive.

Notes and due times come from the retention data directory, `$XDG_DATA_HOME/retention` or `$HOME/.local/share/retention`. `RETENTION_ROOT` selects the root id. With one child of `roots/`, that root is used.

## Launch

From the repo root:

```text
.cursor/skills/verify-retention-popup/bin/verify-popup launch
```

That runs `mise exec rust@stable -- cargo run --release` with `CARGO_TARGET_DIR` unset, so the binary is `target/release/retention-popup`. Rust is not on `PATH`. Bare `cargo` fails here. The launch is ready when a `grim` capture of the `retention-controls` layer OCRs as containing `Top right`, not when the process prints a line and not when the namespace first appears. Hyprland fades the layer in, and an early capture is transparent. The script waits up to 120 seconds. The fresh process starts with the top-right corner selected. A due card may already be open. Do not press Show card.

The script records the process group and the evidence directory in `/tmp/verify-retention-popup.json`. Evidence for that run is `.cursor/skills/verify-retention-popup/evidence/<run-id>/`. `launch.log` in that directory is the process output.

## Doctor

```text
.cursor/skills/verify-retention-popup/bin/verify-popup doctor
```

Run this first whenever a later command looks wrong. Require this shape, with the recorded pid and evidence path filled in:

```text
ok
pid <layer pid>
pgid <process group>
controls 1
card 0
evidence <evidence directory>
```

`card` may be `1` because the process may already have opened the corner card. Doctor fails when no instance was launched by this skill, when the control surface's process group is not the one recorded, or when the control surface count is not 1.

## Drive

The harness is `.cursor/skills/verify-retention-popup/bin/verify-popup`. It captures the layer rectangle from `hyprctl layers` with `grim`. Light text on the dark card is read with `tesseract` (`--psm 6`). The blue corner chips use dark text, which that pass misses, so Pillow isolates each blue band, turns it into black text on white, and reads it again. The click is the center of the matched words, sent through `bin/wayland-click`. The pointer is a `zwlr_virtual_pointer_v1` aimed at the named output. A point can land about 10 pixels from the request, so clicks use the center of a text button, not a border. Pillow is required (`python3 -c 'import PIL'`).

```text
.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "Show answer"
.cursor/skills/verify-retention-popup/bin/verify-popup click --surface card --text "Again"
.cursor/skills/verify-retention-popup/bin/verify-popup shot --surface controls --output <png>
.cursor/skills/verify-retention-popup/bin/verify-popup shot --surface card --output <png>
.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 12
.cursor/skills/verify-retention-popup/bin/verify-popup wait --surface card --timeout 3 --absent
```

`--surface` is `controls` or `card`. `--text` is the visible label, case as drawn. The feature files list every label.

The countdown journey is gone. `prove-show-card` exits with that message and cannot pass. The due-card journey is `features/due-card.md`. Extend a new journey from the feature files. Do not replace a listed entry point with a different one and call it the same proof.

## Evidence

Write proof under `.cursor/skills/verify-retention-popup/evidence/<run-id>/`. Cleanup must not delete that directory.

For a UI action, keep the screenshot from before the action and the screenshot after it, plus the OCR text of the result and the `hyprctl layers` JSON. The due card has to show up as a `retention-card` layer. The card's center has to sit in the top-right half of the monitor that owns it when the chosen corner is top right. The card surface keeps 96 pixels of transparent padding around the gold card, so card OCR is the inner crop. The prompt is the note front.

Drive the real buttons. Do not call `App::update` from a test and present that as this proof. There is no test-only endpoint and no dry-run. The process really does open a surface on the logged-in session.

## Cleanup

```text
.cursor/skills/verify-retention-popup/bin/verify-popup cleanup
```

This signals the recorded process group and deletes `/tmp/verify-retention-popup.json`. It does not delete the evidence directory. Do not kill `retention-popup` by name. If the state file is missing, there is nothing to clean up. Run cleanup after a failed attempt too, so a broken run does not leave a card on screen.

## Helpers

`bin/verify-popup` is the only command the feature files use. It rebuilds `bin/wayland-click` from `bin/wayland-click.c` with `gcc -O2 -Wall -Wextra -lwayland-client` when the source is newer. The click tool's own invocation, which the script uses, is:

```text
bin/wayland-click click <local-x> <local-y> <monitor-width> <monitor-height> <output-name>
```

Coordinates are local to that output. `move` is the same shape and does not press a button.
