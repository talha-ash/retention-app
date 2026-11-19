---
name: prove-popup-countdown
description: Exhaustively check the retention-popup countdown and card state machine in src/main.rs App::update. Use when a change touches Arm, Tick, the delay chips, the corner, Close, Quit, or the quest-card answer buttons.
---

# Prove the popup countdown

A card that opens early, twice, or that ignores close shows up at the wrong time or sticks on screen. The control card's messages and the one-second tick are a small state machine. This skill checks every reachable state of that machine by running `App::update`.

The check is an exhaustive explorer in `src/popup_check.rs`, run with `cargo test`. It calls the real `update` function. A hand-written model would stay green when `src/main.rs` changes, so it would not satisfy this skill. Kani is not installed. TLA+ would check a separate model rather than this function.

## When to run it

Run it before finishing a change that edits any of these in `src/main.rs`:

- `App::update` arms for `Delay`, `Pick`, `Arm`, `Tick`, `Answer`, `ClosePopup`, `Glow`, or `Quit`
- `App::subscription`
- the `Corner` labels or anchors, when the change is about which corner is selected at the opening tick

Rendering, the smoke shader, and the layer-shell task that maps the surface are outside this check.

## What is true

Each sentence is one property. The harm is the sentence after it.

- A countdown and an open card never hold at the same time. The user would see a timer for a card that is already on screen, and a second open could stack another card.
- Show card opens the card on tick number N and not earlier, where N is the delay chip selected at that press (5, 10, or 30). An early card interrupts the user before the delay they chose.
- Pressing a delay chip while a countdown is running changes the chip and does not change the ticks still left. The next Show card uses the new chip. A chip that rewrote the running timer would skip or stretch the wait the status line is showing.
- An answer records `right corner` or `wrong place` and leaves the card up. Closing the card removes it and keeps the answer. Close that left the card up would stick the quest on screen. An answer that closed the card would hide the path the user just locked.
- Quit does not change the card state. The process ends because `iced::exit()` returns an exit task. Quit that also cleared the card would hide a failed exit: the card would vanish while the process kept running.

`Glow` only updates the smoke clock. `RemoveWindow` is the runtime's bookkeeping message and does not change this state. The checker sends both anyway.

## Scope

The explorer starts from `App::new` and applies every sequence of:

- `Delay(5)`, `Delay(10)`, `Delay(30)`
- `Pick` of each corner
- `Arm`, `Tick`, `ClosePopup`, `Glow`, `Quit`
- `Answer("right corner")`, `Answer("wrong place")`
- `RemoveWindow` for the current card id, or a fresh id when no card is open

On the passing run this produced 1152 reachable states. Delays other than the three chips are out of scope. The explorer does not run the exit task, does not execute the layer-shell open task, and does not ask the compositor where the surface landed. The anchor passed to that task is `self.corner.anchor()` at the opening tick (`App::update`, the `Tick` arm). This check sees the `corner` field, not the task payload. A change that passed a constant anchor would not be caught here.

The running app delivers `Tick` only while `remaining` is `Some`, and `Glow` only while a card is open (`App::subscription`). The explorer also sends those messages outside that schedule. `Tick` with no countdown leaves the state unchanged.

## Run

From the repo root:

```text
mise exec rust@stable -- cargo test --bin retention-popup explore_popup_machine -- --nocapture
```

Pass looks like `popup machine reachable states: 1152` and `test result: ok`. A failure prints the property name, the fields, and the shortest message trace. Read that trace as the sequence of `App::update` messages. Do not treat a compile error in `src/popup_check.rs` as a product bug until the test compiles.

Rust is not on `PATH`. `mise exec rust@stable --` is required. Bare `cargo` fails on this machine.

## Telling a bug from a modeling error

A real bug is a trace whose messages the control card or the quest card can send, and whose end state contradicts a property above. `Delay`, `Pick`, `Arm`, `Tick` while a countdown is running, `Answer` while the card is open, `ClosePopup`, and `Quit` are user-visible.

`Answer` while the card is closed, `Tick` while no countdown is running, and `RemoveWindow` are messages `update` accepts and the view does not send. If a failure uses only those, inspect the model before changing the product. If the product change was supposed to make one of those messages matter, update this skill and the explorer together.

Do not weaken a property to make the current code pass. If the current code fails, report the trace in the words above and leave the product code alone unless the user asks for a fix.

## Broken variants that must fail

Each of these was applied to `src/main.rs`, the check failed, and the edit was reverted. After all six reverts the check passed again at 1152 states.

- `Arm` no longer clears `popup_id`. Failure: `countdown_and_card_are_exclusive`, trace `Arm -> Tick -> Tick -> Tick -> Tick -> Tick -> Arm`, `remaining=Some(5)` and `popup=true`.
- The countdown tick jumps straight to 1 second left. Failure: `card_opens_on_the_nth_tick`, trace `Arm -> Tick`, `remaining=Some(1)` for a 5 second delay.
- `Delay` rewrites `remaining` while a countdown is running. Failure: `delay_chip_does_not_change_a_running_countdown`, trace `Arm -> Delay(30)`, `remaining=Some(30)`.
- `Answer` clears `popup_id`. Failure: `answer_records_only_the_pressed_path`, trace ends in `Answer(right corner)`, `popup=false`.
- `ClosePopup` leaves `popup_id` set. Failure: `close_removes_the_card`, trace ends in `ClosePopup`, `popup=true`.
- `Quit` clears `popup_id`. Failure: `quit_leaves_the_card_state_unchanged`, trace ends in `Quit`, `popup=false` after the card had opened.

## Coverage

| Code | What the check holds it to |
| --- | --- |
| `App::update` `Delay` | chip value only; running countdown unchanged |
| `App::update` `Pick` | selected corner only |
| `App::update` `Arm` | clears a card, clears the answer, starts `delay_secs` ticks |
| `App::update` `Tick` | opens on tick N, never a second card |
| `App::update` `Answer` | records one of the two path labels and leaves the card up |
| `App::update` `ClosePopup` | removes the card and keeps the answer |
| `App::update` `Quit` | state unchanged; exit task only |
| `App::update` `Glow` and the layer-shell fallback | no change to delay, countdown, card, or answer |
| `App::subscription` | schedule described above; not executed as a runtime subscription |

This repo has no git commits. Identity of the confirmed code:

- `src/main.rs` sha256 `d4fe04796f647406dd04370f75c9a9c2647f237d3088139fb8fcbe984092a1fb`
- `src/popup_check.rs` sha256 `3567c70e4e4465fe7d97918df1bf0d94b1693f5c67604fd48120346949fb9dd9`

Confirmed 2026-11-18. There is no sync commit. When the repo has commits, set the sync commit to the commit you re-ran this check against, and only after you have confirmed the explorer still describes `App::update` at that commit. If `src/main.rs` changes and the hash above no longer matches, the check is stale until you re-run it and read the `update` arms again. Move the recorded hash (and the sync commit, once one exists) forward only after that confirmation. A passing test against a stale reading of the code is not a confirmation.

Keep the product behavior in `src/main.rs`. The explorer lives in `src/popup_check.rs` and is compiled only for tests (`#[cfg(test)] mod popup_check`).

## After a change that did not re-run this check

Use `/maintain-formal-verification-skill` to bring this skill back in line with the code.
