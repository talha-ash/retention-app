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

A note's identity is its Anki `guid`. The agent copies guids when it updates a deck. The app does not invent identity from the prompt or from a file path.

One run of `/tmp/retention-design-2026-11-19/merge_and_delivery.py` on a three-card library. The learning steps in the delivery half are scripted 10 minute steps, not FSRS. The run is deterministic, so it was not repeated.

- Stable id, rephrase of the prompt. Schedule kept. `edited_kept` 1, `added` 0.
- Prompt hash, same rephrase. A second card appeared. Log line ends `layer-shell:kept, fsrs:kept, haze:new, layer-shell:new`.
- Source path, a new card whose path was `src/main.rs`. The new id `grades` disappeared and its prompt replaced the existing card. Log line `missing-ids ['grades'] text-landed-on ["layer-shell prompt='Which press maps to Good?'"]`.
- Stable id, patch that edits one card, adds one, and omits one. The omitted card stayed. `retired` 0.
- Stable id, the same file imported as a snapshot. `retired` 2.
- Stable id, the agent minted new ids for the same three prompts. Three extra cards. `added` 3.
- Second apply of the same patch, all three strategies. `unchanged` 2, `added` 0.

One throwaway run of `/tmp/retention-design-2026-11-19/read_apkg.py` built a legacy-2 `.apkg` and Anki's text note file for the same note. The package was a zip whose `collection.anki21` held the note. Fields were split on the `0x1f` byte. Both reads printed `abc123XYZ0`, `Basic`, `Spanish::Verbs`, and `hablar / to speak`. The script printed `same True`. The text file named the guid with `#guid column:3`. A text row with no guid is a new note. Anki's export help says a text reimport updates a note when the first field is unchanged. That match is the prompt-hash result above, and this plan does not use it. The sources for the package layout are the Anki schema notes and the legacy-2 writeup. This run did not open Anki, and it did not read a zstd `collection.anki21b`.

A text import and an `.apkg` import match on guid. Omission does not retire. The user retires a note from the library window. Applying the same file twice changes nothing.

An edit keeps memory. A `reset` column value of `yes` clears that note. A retired guid keeps its memory row. A later import of that guid brings the note back with the old memory, unless `reset` is `yes`. The agent is told not to reuse a guid for a different fact.

### Card types

`Basic` has `Front` and `Back`. The card shows Front, then Back, then Again, Hard, Good, and Easy.

`Choice` is the note type in Format. The user selects any number of choices. The exact set grades Good. Any other selection grades Again. The empty set does not parse. The grade script printed `two-exact Good`, `two-partial Again`, `two-extra Again`, and `empty rejected`.

`Cloze` stores the `Text` field. An unknown note type is skipped. The rest of the package still imports.

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
decks.json
collection.anki21
memory.sqlite
```

`root.json` and `decks.json` are app-owned delivery. `collection.anki21` is Anki's notes, cards, note types, and deck names. `memory.sqlite` is our FSRS state, keyed by guid and card ord. Losing `memory.sqlite` loses schedules and keeps the notes.

One imported `.apkg` is one root. Deck names inside it are decks. `Spanish::Verbs` is one deck name, with the parent kept in the name. A file that names a root is rejected. The open root is the import target.

Export for Anki writes a legacy-2 `.apkg` of the notes and deck names. That package does not contain our FSRS rows. Export for an agent writes Anki's text note file, including the guid column. There is no JSON exchange file.

The binary name stays `retention-popup`. The window title can say Retention.

### Agent loop

The app does not read a repository and does not call a model. The loop outside the app is four steps.

1. Export the open root as Anki's text note file.
2. Give an agent that file, the format section below, and whatever changed in the project.
3. The agent returns one text file. Same guids for facts that still exist, new guids for new facts. A `reset` column value `yes` clears memory for that note. Omitting a row does not retire it.
4. Import the text file into the same root.

An `.apkg` from Anki imports into the open root by the same guid rule. The user retires a note in the library window. The text file cannot retire by omission.

A `source` string on a card is stored and shown. It is not an identity and it is not fetched. GitHub is not in this plan.

### Format

The note model is Anki's. Two files carry it. A `.apkg` is what Anki exports. A text note file is what an agent writes, and Anki can import that file too.

A `.apkg` is a zip. The reader opens the first collection it finds, in this order. `collection.anki21b` is zstd-compressed sqlite. `collection.anki21` is sqlite. `collection.anki2` is the older sqlite. Notes are rows. Fields are split on the `0x1f` byte. Deck names and note types are the JSON in the `col` row. `media` may be a JSON map. When it is not JSON, the reader skips media and still reads notes. Image and audio files are not shown in v1.

Note types v1 reads:

- `Basic`. Fields `Front` and `Back`. One card.
- `Basic (and reversed card)`. One note, two cards.
- `Cloze`. The `Text` field is stored. The popup shows it in `pr-popup`.
- `Choice`. Fields `Prompt`, `Choices`, `Correct`, and `Explain`. `Choices` is `id|text` items separated by `;`. `Correct` is a non-empty set of those ids, separated by spaces. The exact set grades Good. Any other selection grades Again.

Any other note type is skipped. The rest of the package still imports. An empty `Correct` field fails that note.

The text file uses Anki's headers. A Basic note and a Choice note look like this.

```text
#separator:Tab
#html:false
#notetype:Basic
#deck:Spanish::Verbs
#columns:Front	Back	guid
#guid column:3
hablar	to speak	abc123XYZ0
```

```text
#separator:Tab
#html:false
#notetype:Choice
#deck:Popup
#columns:Prompt	Choices	Correct	Explain	guid
#guid column:5
Which of these are FSRS grades?	again|Again;hard|Hard;good|Good;easy|Easy	again good	The crate returns one next state per grade.	gradeGuid01
```

`cards.due`, `cards.ivl`, and `cards.data` may be present. `pr-format` counts those rows and does not write memory. Mapping an Anki schedule into FSRS is not in this plan. A notes-only import leaves existing memory in place.

## Perf rule

No timing baseline exists. Do not invent a millisecond cap. The first PR that runs a probe writes three runs, the median, and the range to `docs/evidence/<pr-id>.txt`. A later PR fails that probe when its median is greater than twice the recorded median and the ranges do not overlap. Overlapping ranges do not fail the PR. Twice is a coarse alarm, not a budget anyone has measured.

## pr-format

- [ ] Depends on trunk.
- [ ] Files. `Cargo.toml` (add `rusqlite` with `bundled`, and a zstd crate), `src/lib.rs`, `src/model.rs`, `src/format.rs`, `examples/accept.rs`, `fixtures/basic.txt`, `fixtures/choice.txt`, `fixtures/unknown.txt`. The `.apkg` fixture is built by the test, not committed as a blob. No other files. Do not edit `src/main.rs`.
- [ ] Change. Read a `.apkg` and Anki's text note file into the note model in Format. `Choice.Correct` is a non-empty set of ids that exist in `Choices`. An unknown note type is skipped. Scheduling columns are counted and not written to memory.
- [ ] Result. The Basic fixture yields guid `abc123XYZ0`, deck `Spanish::Verbs`, and fields `hablar` and `to speak`. The Choice fixture yields a correct set of two ids.
- [ ] Unit. `mise exec rust@stable -- cargo test` builds a legacy-2 zip, reads it, and reads the same note from `fixtures/basic.txt`. The two results are equal. A zstd `collection.anki21b` of that same sqlite reads back the same guid. `fixtures/unknown.txt` keeps the Basic note and skips the other. An empty `Correct` field fails that note.
- [ ] Live. `mise exec rust@stable -- cargo run --example accept -- fixtures/basic.txt` prints `guid abc123XYZ0`, `deck Spanish::Verbs`, and `notes 1`, then exits 0. The same command on the generated `.apkg` prints those same three lines. `scheduling-applied 0` is in both outputs.
- [ ] Perf. Generate a 1000-note `.apkg`, run the example three times, write median and range to `docs/evidence/pr-format.txt`. This box is the record. It does not fail on a cap.

## pr-merge

- [ ] Depends on `pr-format`.
- [ ] Files. `src/merge.rs`, `examples/apply.rs`, `fixtures/delta.txt`. No other files.
- [ ] Change. Apply a text file and an `.apkg` to an in-memory library that already has memory rows, matched on guid. Implement the identity rules in Decisions.
- [ ] Result. A rephrased Front keeps memory. A second apply prints no changes. An omitted row stays.
- [ ] Unit. Assert memory kept on a Front rephrase and on a Back edit. Assert `reset` `yes` clears one note only. Assert a text file that omits a note retires nothing. Assert a new guid adds a note and does not attach to an old one. Assert the second apply reports no change.
- [ ] Live. `mise exec rust@stable -- cargo run --example apply -- fixtures/delta.txt` prints `added 1` and `retired 0`. Run it again and it prints `changed 0`.
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
- [ ] Files. `src/disk.rs`, `examples/roundtrip.rs`. No other files. `rusqlite` is already added by `pr-format`.
- [ ] Change. Create the directory layout in Decisions under a caller-supplied data dir. Write `collection.anki21` and `memory.sqlite` by writing a temp file and renaming it into place. Export builds a legacy-2 `.apkg` and a text note file from the notes. The package has no FSRS rows.
- [ ] Result. A root round-trips on guid. The exported package has `scheduling-applied 0`. An unchanged note's fields stay the same on a second import.
- [ ] Unit. Import, export, and compare guids. Leave a temp file in the directory and run the import again. The finished files match a clean run. Second apply changes no schedule row.
- [ ] Live. `HOME` pointed at an empty temp dir. `mise exec rust@stable -- cargo run --example roundtrip -- fixtures/basic.txt` prints `exported 1` and `fsrs-rows-in-apkg 0`, and exits 0. `memory.sqlite` exists in the root. Unzipping the export shows `collection.anki21` and does not show `memory.sqlite`.
- [ ] Perf. Import 1000 cards, three runs, write `docs/evidence/pr-disk.txt`. Record only.

## pr-popup

Review-gated. Merge only after the operator has seen the live card.

- [ ] Depends on `pr-delivery` and `pr-disk`.
- [ ] Files. `src/main.rs`, delete `src/popup_check.rs`, `.cursor/skills/verify-retention-popup/SKILL.md`, `.cursor/skills/verify-retention-popup/features/show-card.md`, and a new feature file in that `features/` directory for the due-card journey. No other files. Do not remove `src/smoke.rs`.
- [ ] Change. Remove the delay slider as the scheduler and remove the hardcoded quest prompt. The running process asks delivery for the next card and opens the existing corner card. A Basic note reveals Back, then the four grades. A Choice note selects, then Continue. A Cloze note hides the deletion, then reveals it, then the four grades. A grade writes memory and restarts that deck's gap. The corner card's keyboard interactivity is off. A second launch exits while the first process still holds the layer namespace.
- [ ] Result. A fixture card whose prompt is not the demo sentence appears in the corner because it is due. After Again, it leaves, stays gone inside the gap, and returns after the gap.
- [ ] Unit. `mise exec rust@stable -- cargo test` passes with `src/popup_check.rs` gone. A pure test, with no window, records one Again and shows the deck's next-allowed time moved by the gap.
- [ ] Live. Fixture root, one Basic note due now, gap 5 seconds, window covering the whole day, corner top right. Launch with `.cursor/skills/verify-retention-popup/bin/verify-popup launch`. Do not press Show card. Pass when all of these hold. The `retention-card` layer's OCR contains the fixture prompt. After the Again control, `verify-popup wait --surface card --timeout 3 --absent` succeeds. Two seconds later the card layer is still absent. Eight seconds after the grade, the card layer is present again. Evidence is the skill's screenshots, OCR, and `hyprctl layers` JSON under `.cursor/skills/verify-retention-popup/evidence/<run-id>/`.
- [ ] Perf. Time from the grade click to the card layer disappearing, three runs, write `docs/evidence/pr-popup.txt`. Record only. This number is the compositor plus the app. Say so in the evidence file.

## pr-library

Review-gated.

- [ ] Depends on `pr-popup`.
- [ ] Files. `src/main.rs`, `src/library.rs`, `.cursor/skills/verify-retention-popup/features/` with one new feature file for this journey. No other files.
- [ ] Change. The control card can create a root, create a deck, import a file into the open root, export the open root, and edit delivery. A new deck is inherit. An override asks for both window and gap. The corner picker stays.
- [ ] Result. A root created from the control card, with one imported deck, shows a due card through the `pr-popup` path.
- [ ] Unit. Create, import, and export against a temp data dir. The exported guids match the file. The new deck's delivery is inherit until the override is saved.
- [ ] Live. Drive the control card with `verify-popup click`. Pass when the export file from the UI contains the imported guid, the deck row is inherit before the override control, and after the override the next card still uses one layer. Screenshots before and after each control, plus OCR, in the skill evidence directory.
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
- Image and audio files stored in an `.apkg`.
- Mapping Anki's `due`, `ivl`, or `cards.data` into FSRS.
- A JSON exchange file.
- Renaming the crate.

## Unproven

- The Rust parser and the Wayland journey. The `.apkg` read was a Python script on a package that script built. Anki itself was not opened.
- `collection.anki21b`. The script did not decompress one.
- FSRS intervals. The delivery timestamps are scripted steps.
- That an agent will keep ids. The format says to. No agent was run.
- Parse time, grade time, wake time, and idle resident set. No probe has been run.
- Whether Again, Hard, Good, and Easy fit the gold card. That is the `pr-popup` review.
- The solo two-clock case. It did not double-fire. Do not cite it as proof that two owners are safe.
