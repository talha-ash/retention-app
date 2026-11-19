# Retention build plan

The goal is a desktop app that shows one flashcard on a timer, so work done with an agent stays in memory. This file is the plan an owner runs box by box. It does not implement the app.

Landing order is `pr-format`, then `pr-merge` and `pr-memory` together, then `pr-delivery`, then `pr-disk`, then `pr-popup`, then `pr-library` and `pr-autostart` together.

```text
trunk
  pr-format
    pr-merge
    pr-memory
      pr-delivery
      pr-disk          waits for pr-merge and pr-memory
        pr-popup       waits for pr-delivery and pr-disk
          pr-library
          pr-autostart
```

`pr-merge` and `pr-memory` both branch from `pr-format` and do not edit the same files. `pr-library` and `pr-autostart` both wait for `pr-popup` and do not edit the same files.

`pr-popup`, `pr-library`, and `pr-autostart` change an interaction. Each waits for the operator's review before merge. The earlier PRs do not.

`src/main.rs` stays the countdown prototype until `pr-popup`. `cargo test` for the countdown in `src/popup_check.rs` stays green through `pr-disk`.

Bare `cargo` is not on `PATH` on this machine. Every command below is prefixed with `mise exec rust@stable --`.

## Decisions

A root is one independent library. A deck is a folder inside that root. A card belongs to one deck. Findings called a root a topic. This plan says root.

Three stores stay apart.

- The agent file holds card text only.
- The app holds memory (FSRS state, due time, review log).
- The app holds delivery (the window and the gap).

Import rejects a card that contains `due`, `stability`, `difficulty`, `reps`, `lapses`, or `interval`.

### Identity

A card's identity is its `id` string. The agent copies ids when it updates a deck. The app does not invent identity from the prompt or from a file path.

One run of `/tmp/retention-design-2026-11-19/merge_and_delivery.py` on a three-card library. The learning steps in the delivery half are scripted 10 minute steps, not FSRS. The run is deterministic, so it was not repeated.

- Stable id, rephrase of the prompt. Schedule kept. `edited_kept` 1, `added` 0.
- Prompt hash, same rephrase. A second card appeared. Log line ends `layer-shell:kept, fsrs:kept, haze:new, layer-shell:new`.
- Source path, a new card whose path was `src/main.rs`. The new id `grades` disappeared and its prompt replaced the existing card. Log line `missing-ids ['grades'] text-landed-on ["layer-shell prompt='Which press maps to Good?'"]`.
- Stable id, patch that edits one card, adds one, and omits one. The omitted card stayed. `retired` 0.
- Stable id, the same file imported as a snapshot. `retired` 2.
- Stable id, the agent minted new ids for the same three prompts. Three extra cards. `added` 3.
- Second apply of the same patch, all three strategies. `unchanged` 2, `added` 0.

Patch is the default. Omission does not retire. A deck may list ids under `retire`, and that is the only way a patch removes a card. Snapshot retires every card of that deck that the file does not name. Applying the same patch twice changes nothing.

An edit keeps memory. `reset: true` on that card clears it. A retired id keeps its memory row and loses its card file. A later upsert of that id brings the card back with the old memory, unless `reset` is set. The agent is told not to reuse an id for a different fact.

### Card types

`qa` has a prompt and an answer. The card shows the prompt, then the answer, then Again, Hard, Good, and Easy.

`choice` has a prompt, choices, a non-empty `correct` set of choice ids, and an explain string. The user selects any number of choices. The exact set grades Good. Any other selection grades Again. The empty set does not parse. The same run printed `two-exact Good`, `two-partial Again`, `two-extra Again`, and `empty rejected`.

An unknown `type` is skipped. The rest of the file still imports.

### Memory

Memory is the `fsrs` crate. `FSRS::default()` and desired retention `0.90`. A grade calls `next_states`. The app stores the returned state and the due instant, and appends a review log line. v1 does not train weights.

This plan does not re-measure FSRS. The crate is the scheduler findings already selected.

### Delivery

Delivery is a window and a gap. The window is a same-day start and end, local time, `start` before `end`. The gap is an integer number of minutes, at least 1. There is no per-weekday schedule in v1.

A deck is `inherit` or a full override. An override names both the window and the gap. A deck cannot keep the root gap and add its own. One deck has one delivery. One card has one deck. The process shows one card.

The next wake is the earliest instant when some deck is inside its window, its gap has elapsed since its last popup, and it has a due card. Answering restarts that deck's gap only. A due card that is inside a gap stays due and waits.

Same script, same fixture. Window 09:00 to 18:00. Vocab gap 45 minutes. Hot gap 10 minutes. Three vocab cards due at or before 09:00, one of them at 08:30. The hot card due at 09:10 with two scripted 10 minute steps.

- Clock list at 09:00 and 16:00. `layer=0`. The 10 minute card never appeared.
- One shared gap of 45 minutes. The hot card first appeared at 11:15, then 12:00, then 12:45.
- One shared gap of 10 minutes. The hot card was due at 09:10 and first appeared at 09:30, after the vocab cards.
- Per-deck gaps, one card on screen. Hot at 09:10, 09:20, and 09:30. Next vocab card at 09:45, then 10:30.
- Root gap and deck gap both armed. `09:00 root->vocab/early AND deck->vocab/hola`. `minutes with two popups=1`.
- The same two clocks with only the hot card. `minutes with two popups=0`. The first answer moved the due time before the second clock looked. The failure that did show up is the shared-library minute, not a double review of one card.

v1 uses the per-deck result. The corner card uses layer `top`, the prototype's layer. It does not use `overlay`.

### Disk

Roots live in `$XDG_DATA_HOME/retention/roots/<root-id>/`, or `~/.local/share/retention/roots/<root-id>/` when `XDG_DATA_HOME` is unset.

```text
root.json
decks/<deck-id>/deck.json
decks/<deck-id>/cards/<card-id>.json
memory.sqlite
```

`root.json` and `deck.json` are app-owned. Card files are the text the agent can round-trip. `memory.sqlite` is not exported. Losing that file loses schedules and keeps the card text.

The exchange file is one JSON document. Import applies it to the open root. A file that names a root is rejected. Export writes card text and ids, never memory.

JSON only. No second parser.

The binary name stays `retention-popup`. The window title can say Retention.

### Agent loop

The app does not read a repository and does not call a model. The loop outside the app is four steps.

1. Export the open root.
2. Give an agent that file, the format section below, and whatever changed in the project.
3. The agent returns one patch. Same ids for facts that still exist, new ids for new facts, `retire` for facts that are gone, `reset` when a fact changed enough that the old interval is wrong.
4. Import the patch into the same root.

A `source` string on a card is stored and shown. It is not an identity and it is not fetched. GitHub is not in this plan.

### Format

`format` is `1`. Missing `mode` means `patch`.

```json
{
  "format": 1,
  "mode": "patch",
  "decks": [
    {
      "id": "popup",
      "name": "Popup",
      "retire": ["old-card"],
      "cards": [
        {
          "id": "layer-shell",
          "type": "qa",
          "prompt": "What protocol pins the card?",
          "answer": "wlr-layer-shell",
          "source": "https://wayland.app/protocols/wlr-layer-shell-unstable-v1"
        },
        {
          "id": "grades",
          "type": "choice",
          "prompt": "Which of these are FSRS grades?",
          "choices": [
            {"id": "again", "text": "Again"},
            {"id": "hard", "text": "Hard"},
            {"id": "good", "text": "Good"},
            {"id": "easy", "text": "Easy"}
          ],
          "correct": ["again", "good"],
          "explain": "The crate returns one next state per grade.",
          "reset": false
        }
      ]
    }
  ]
}
```

## Perf rule

No timing baseline exists. Do not invent a millisecond cap. The first PR that runs a probe writes three runs, the median, and the range to `docs/evidence/<pr-id>.txt`. A later PR fails that probe when its median is greater than twice the recorded median and the ranges do not overlap. Overlapping ranges do not fail the PR. Twice is a coarse alarm, not a budget anyone has measured.

## pr-format

- [ ] Depends on trunk.
- [ ] Files. `Cargo.toml`, `src/lib.rs`, `src/model.rs`, `src/format.rs`, `examples/accept.rs`, `fixtures/patch.json`, `fixtures/choice.json`, `fixtures/bad-memory.json`, `fixtures/unknown-type.json`. No other files. Do not edit `src/main.rs`.
- [ ] Change. Parse the format above into typed cards. `correct` is a non-empty set of choice ids that exist on that card. Memory fields fail the file. An unknown `type` is skipped.
- [ ] Result. `fixtures/choice.json` yields one choice card whose correct set has two ids.
- [ ] Unit. `mise exec rust@stable -- cargo test` covers the four fixtures. The bad memory fixture fails on the field name. The unknown type fixture keeps the known card.
- [ ] Live. `mise exec rust@stable -- cargo run --example accept -- fixtures/patch.json` prints `cards 2` and exits 0. The same command on `fixtures/bad-memory.json` exits non-zero and prints the rejected field name.
- [ ] Perf. Generate 1000 qa cards, run the example three times, write median and range to `docs/evidence/pr-format.txt`. This box is the record. It does not fail on a cap.

## pr-merge

- [ ] Depends on `pr-format`.
- [ ] Files. `src/merge.rs`, `examples/apply.rs`, `fixtures/delta.json`, `fixtures/snapshot.json`. No other files.
- [ ] Change. Apply patch and snapshot to an in-memory library that already has memory rows. Implement the identity rules in Decisions.
- [ ] Result. The stable-id rows of the prototype hold. A second apply prints no changes.
- [ ] Unit. Assert schedule kept on rephrase, on path change, and on an answer edit. Assert `reset` clears one card only. Assert a patch that omits a card retires nothing. Assert the snapshot fixture retires the missing cards. Assert `retire` removes that card file's logical card and a second apply does not report a change. Assert new ids add cards and do not attach to old ones.
- [ ] Live. `mise exec rust@stable -- cargo run --example apply -- fixtures/delta.json` prints `added 1` and `retired 0`. Run it again and it prints `changed 0`.
- [ ] Perf. Same 1000-card probe as `pr-format`, three runs. Fail only under the perf rule against `docs/evidence/pr-format.txt`.

## pr-memory

- [ ] Depends on `pr-format`. Branch from `pr-format`, not from `pr-merge`.
- [ ] Files. `Cargo.toml` (add `fsrs`), `src/memory.rs`, `examples/grade.rs`. No other files.
- [ ] Change. Map Again, Hard, Good, and Easy to `next_states`. Map an exact choice set to Good and any other selection to Again. Append a review log value the caller can store.
- [ ] Result. One fixed previous state and one elapsed-day count produce four due instants, and the choice mapping matches the grade lines in Decisions.
- [ ] Unit. The four due instants equal the crate's return values for that input. Do not reimplement the formula. Choice tests cover exact, partial, and extra.
- [ ] Live. `mise exec rust@stable -- cargo run --example grade` prints four lines, `Again`, `Hard`, `Good`, and `Easy`, each with a due instant, and exits 0.
- [ ] Perf. 1000 `next_states` calls, three runs, write `docs/evidence/pr-memory.txt`. Record only.

## pr-delivery

- [ ] Depends on `pr-memory`.
- [ ] Files. `src/delivery.rs`, `examples/wake.rs`. No other files.
- [ ] Change. A deck delivery is inherit or a full override. Compute the next wake and the one card it shows, using the per-deck rules in Decisions.
- [ ] Result. The fixture in Decisions prints the per-deck timestamps, and a type that would arm two deliveries for one deck does not exist.
- [ ] Unit. Assert hot at 09:10, 09:20, 09:30 and vocab at 09:00, 09:45, 10:30 for that fixture. Assert the clock-list fixture shows `layer` zero times. Assert two decks ready in the same minute produce one card, and the other card is the next wake.
- [ ] Live. `mise exec rust@stable -- cargo run --example wake` prints `09:10 hot/layer` as the first hot line and exits 0.
- [ ] Perf. 10000 cards, three runs, write `docs/evidence/pr-delivery.txt`. Record only.

## pr-disk

- [ ] Depends on `pr-merge` and `pr-memory`.
- [ ] Files. `Cargo.toml` (add `rusqlite` with `bundled`), `src/disk.rs`, `examples/roundtrip.rs`. No other files.
- [ ] Change. Create the directory layout in Decisions under a caller-supplied data dir. Write card files and sqlite by writing a temp file and renaming it into place. Export builds the exchange document from card files only.
- [ ] Result. A root round-trips. The export document has no memory fields. An unchanged card's bytes stay the same on a second patch apply.
- [ ] Unit. Import, export, and compare ids. Leave a temp file in the directory and run the import again. The finished files match a clean run. Second apply changes no schedule row.
- [ ] Live. `HOME` pointed at an empty temp dir. `mise exec rust@stable -- cargo run --example roundtrip -- fixtures/patch.json` prints `exported 2` and `memory-fields 0`, and exits 0. `memory.sqlite` exists in the root and its bytes are not in the printed export.
- [ ] Perf. Import 1000 cards, three runs, write `docs/evidence/pr-disk.txt`. Record only.

## pr-popup

Review-gated. Merge only after the operator has seen the live card.

- [ ] Depends on `pr-delivery` and `pr-disk`.
- [ ] Files. `src/main.rs`, delete `src/popup_check.rs`, `.cursor/skills/verify-retention-popup/SKILL.md`, `.cursor/skills/verify-retention-popup/features/show-card.md`, and a new feature file in that `features/` directory for the due-card journey. No other files. Do not remove `src/smoke.rs`.
- [ ] Change. Remove the delay slider as the scheduler and remove the hardcoded quest prompt. The running process asks delivery for the next card and opens the existing corner card. `qa` reveals, then the four grades. `choice` selects, then Continue. A grade writes memory and restarts that deck's gap. The corner card's keyboard interactivity is off. A second launch exits while the first process still holds the layer namespace.
- [ ] Result. A fixture card whose prompt is not the demo sentence appears in the corner because it is due. After Again, it leaves, stays gone inside the gap, and returns after the gap.
- [ ] Unit. `mise exec rust@stable -- cargo test` passes with `src/popup_check.rs` gone. A pure test, with no window, records one Again and shows the deck's next-allowed time moved by the gap.
- [ ] Live. Fixture root, one qa card due now, gap 5 seconds, window covering the whole day, corner top right. Launch with `.cursor/skills/verify-retention-popup/bin/verify-popup launch`. Do not press Show card. Pass when all of these hold. The `retention-card` layer's OCR contains the fixture prompt. After the Again control, `verify-popup wait --surface card --timeout 3 --absent` succeeds. Two seconds later the card layer is still absent. Eight seconds after the grade, the card layer is present again. Evidence is the skill's screenshots, OCR, and `hyprctl layers` JSON under `.cursor/skills/verify-retention-popup/evidence/<run-id>/`.
- [ ] Perf. Time from the grade click to the card layer disappearing, three runs, write `docs/evidence/pr-popup.txt`. Record only. This number is the compositor plus the app. Say so in the evidence file.

## pr-library

Review-gated.

- [ ] Depends on `pr-popup`.
- [ ] Files. `src/main.rs`, `src/library.rs`, `.cursor/skills/verify-retention-popup/features/` with one new feature file for this journey. No other files.
- [ ] Change. The control card can create a root, create a deck, import a file into the open root, export the open root, and edit delivery. A new deck is inherit. An override asks for both window and gap. The corner picker stays.
- [ ] Result. A root created from the control card, with one imported deck, shows a due card through the `pr-popup` path.
- [ ] Unit. Create, import, and export against a temp data dir. The exported ids match the file. The new deck's delivery is inherit until the override is saved.
- [ ] Live. Drive the control card with `verify-popup click`. Pass when the export file from the UI contains the imported card id, the deck row is inherit before the override control, and after the override the next card still uses one layer. Screenshots before and after each control, plus OCR, in the skill evidence directory.
- [ ] Perf. No new probe. Repeat the `pr-disk` import probe, three runs, and apply the perf rule against `docs/evidence/pr-disk.txt`.

## pr-autostart

Review-gated. The PR adds a snippet. It does not edit the operator's Hyprland config.

- [ ] Depends on `pr-popup`.
- [ ] Files. `docs/session.md`. No other files.
- [ ] Change. Document one `exec-once` line that starts `retention-popup` when Hyprland starts. State that closing the card does not exit the process, and that a second start exits.
- [ ] Result. The snippet is one line and names this binary.
- [ ] Unit. None. The file is the change. Read it back and check the binary name.
- [ ] Live. The operator installs the line and logs in once. Pass when one `retention-popup` process is running and no card is up until a card is due. The operator writes that process list into `docs/evidence/pr-autostart.txt`.
- [ ] Perf. Resident set of that idle process, three samples about 30 seconds apart, in the same evidence file. Record only.

## Out of this plan

- Any GitHub client, webhook, or repo sync.
- FSRS weight training.
- GNOME, X11, Windows, and macOS.
- Per-weekday windows, overnight windows, and more than one window per day.
- A second card type beyond `qa` and `choice`.
- Renaming the crate.

## Unproven

- The parser, the sqlite layout, and the Wayland journey. This plan only ran the Python script.
- FSRS intervals. The delivery timestamps are scripted steps.
- That an agent will keep ids. The format says to. No agent was run.
- Parse time, grade time, wake time, and idle resident set. No probe has been run.
- Whether Again, Hard, Good, and Easy fit the gold card. That is the `pr-popup` review.
- The solo two-clock case. It did not double-fire. Do not cite it as proof that two owners are safe.
