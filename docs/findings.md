# Findings

Date: 2026-11-18. This machine: Arch, Omarchy, Wayland, Hyprland 0.56.2.

Rust can do this app. The popup is the part that looks hard, and on this desktop it is a solved protocol, already in use by Omarchy itself. A corner-card prototype now exists and has been opened on this session, including a moving blue haze around the card. The product (topics, FSRS, files, autostart) is not started. Rust is fixed. Everything else below is a finding or a recommendation, and the open list at the end is still open.

## Verdict

One Rust process can sit in the background, keep the settings window closed, and at a scheduled time show a small card in a corner. The card is drawn by us. It can have a close button and answer buttons. It is not an OS notification, and it is not a second launch of the app.

The card has to be a [layer-shell surface](https://wayland.app/protocols/wlr-layer-shell-unstable-v1). A normal window cannot place itself.

## Why a normal window cannot be the popup

Wayland does not give a client a global coordinate space. `xdg-shell` lets a client describe its own window geometry. It does not let a client say "put me 16px from the top-right of the monitor." [`winit`](https://rust-windowing.github.io/winit/winit/window/trait.Window.html) documents `set_outer_position` as unsupported on Wayland. Iced, egui, Slint's default backend, and Tauri all sit on that window model, so a "borderless always-on-top window at the corner" works on X11, Windows, and macOS, and does nothing useful here.

Layer shell is the Wayland protocol for surfaces that are layers of the desktop: bars, docks, wallpapers, notifications. The client sets:

- **anchor** — bitflags `top`, `bottom`, `left`, `right`. Top and right together is the top-right corner. The same four bits cover the other three corners and the edges.
- **size** — both width and height set by us, so the card is a card, not a bar.
- **margin** — inset from the anchored edges.
- **exclusive zone** — `0` means "slide me out of the way of bars that have reserved their edge." A corner card cannot reserve an edge anyway. This is the notification pattern in the protocol text.
- **layer** — `top` sits above normal windows. `overlay` sits above fullscreen. Default for us should be `top`, so the card appears over the editor and a fullscreen app can cover it.
- **keyboard** — off unless we ask. Pointer, touch, and tablet events still arrive. A click on a choice or the close button does not have to steal the keyboard from whatever is being typed.

Hyprland treats these as layers, not windows. Its wiki says layer surfaces are wallpapers, notification overlays, and bars, and it animates them with `layersIn` / `layersOut` (`slide`, `popin`, `fade`). The open and close motion comes from the compositor. The pixels inside the card are ours, including a transparent rounded rectangle.

This session already runs that protocol. `hyprctl layers` on 2026-11-18 shows, per monitor:

| Namespace | Layer | What it is |
| --- | --- | --- |
| `omarchy-background` | 0 background | wallpaper |
| `omarchy-bar` | 2 top | top bar |
| `omarchy-dock` | 2 top | dock |
| `omarchy-notifications` | 3 overlay | notification strip on the right |

The protocol registry lists Hyprland 0.52.1 as layer shell version 5. This machine is Hyprland 0.56.2 and the surfaces above are live, so the mechanism is present on the compositor we will actually run.

A settings window is a normal window. The popup is a second surface in the same process. Showing it does not map the settings window and does not take a tile.

## What does not satisfy the popup

- **libnotify / the Freedesktop notification daemon.** The daemon owns the drawing. We would get Omarchy's notification strip, not our card, and answer buttons would be whatever that daemon allows.
- **A positioned `winit` window.** Unsupported on Wayland, which is this session (`XDG_SESSION_TYPE=wayland`).
- **Tauri, or Slint on its default winit backend, for the card.** Both can draw a good-looking view. Neither can anchor it to a corner here.
- **`spell_framework` as the base.** It does put Slint on layer shell, and its own docs say breaking changes are expected. Fine as a reference, poor as the thing the whole app sits on.
- **Hand-writing the Wayland client** with `smithay-client-toolkit` alone. The toolkit does expose layer shell (`LayerShell::bind`, `set_anchor`, `set_size`, a `simple_layer` example). We would then also own the renderer, input, and text layout. That is the maximum-control path and the largest amount of code for no product gain in v1.

## GUI crate

Rust stays. The crate is not locked. One option already matches the popup, the background process, and a real UI toolkit:

**[iced](https://docs.rs/iced_layershell/latest/iced_layershell/) + [`iced_layershell`](https://crates.io/crates/iced_layershell) (waycrate).** Crate `iced_layershell` 0.19.1, updated 2026-07-12. It draws with iced and anchors with layer shell. `application()` is one surface. `daemon()` is the background case: no surface until we open one, and it can target the active screen. Anchors are the protocol anchors, so top-right is `Anchor::Top | Anchor::Right` plus a margin. Input regions can be limited to the card, so clicks outside it fall through to the window underneath.

That crate is community-maintained, not part of iced itself. It has to track iced's versions. That is the main technical risk of this choice.

Slint is the stronger drawing toolkit if a later prototype looks too plain. Choosing it means writing or adopting a layer-shell backend, because Slint's Wayland support is winit. GTK4 via `gtk4-rs` plus `gtk4-layer-shell` also anchors correctly, and it is how Waybar does this, but it is a C toolkit with Rust bindings, which is the opposite of "a Rust GUI library we fully control."

The main window and the card should share one UI crate. Platform-specific code stays in the "where does this surface sit" module. On a later Windows or macOS port, that module is a borderless window with real coordinates. On GNOME it is a different problem, described next.

## Bindings in the raylib style

These are C or C++ libraries with a Rust binding. The app code is Rust. The window and the drawing library are not.

Draw-it-yourself, same idea as raylib:

| Binding | C library | What you get | Corner popup on Hyprland |
| --- | --- | --- | --- |
| [`raylib`](https://github.com/raylib-rs/raylib-rs) crate, raylib 6.0 | raylib, windowing through GLFW | Rectangles, text, images, and optional `raygui` buttons. A `wayland` feature exists. | No. `set_window_position` is exposed, and GLFW reports that call unavailable on Wayland. The compositor places the window. |
| [`sdl3`](https://docs.rs/sdl3) | SDL3 | A window and a renderer. We draw every pixel. | No. Same window model. |
| [`imgui`](https://github.com/imgui-rs/imgui-rs) | Dear ImGui | Immediate-mode buttons, text, and panels. Official backends are winit and SDL2. | No. Those backends cannot set a screen position here. |

Widget toolkits, Rust bindings, different idea from raylib:

| Binding | C/C++ library | What you get | Corner popup on Hyprland |
| --- | --- | --- | --- |
| [`fltk`](https://github.com/fltk-rs/fltk-rs) 1.5 | FLTK 1.4 | About 80 widgets, small static binary, theme schemes. | No layer-shell support. A normal window. |
| [`gtk4`](https://github.com/gtk-rs/gtk4-rs) + [`gtk4-layer-shell`](https://docs.rs/gtk4-layer-shell) 0.8.1 | GTK 4 and gtk4-layer-shell | Widgets plus CSS. `init_layer_shell()`, then anchor and margin. Needs both C libraries installed. | Yes. This is the binding that can pin the card. |
| cxx-qt | Qt | Qt Quick or widgets from Rust. Heavier. Omarchy's own apps are Qt written in C++. | Only with extra layer-shell integration. Not the small path. |

raylib is the right shape if the goal is "Rust calls a C drawing library and we paint the card." On this session that card opens as a normal window, and Hyprland decides where it sits. `gtk4` plus `gtk4-layer-shell` is the binding that can put our UI at top-right. iced plus `iced_layershell`, and GPUI below, are the Rust-native crates that can both paint a custom card and anchor it. raylib, SDL, and Dear ImGui meet the drawing style and miss the corner.

## GPUI

[GPUI](https://gpui.rs/) is Zed's UI framework, written in Rust. It is not a port of raylib or GTK. Layout is CSS-style: `div()`, flex, gap, padding, color, shadow, text. The homepage sample is a normal window (`WindowBounds::Windowed`). That sample cannot pin itself to a corner on Wayland.

The current Zed tree can. [`WindowKind`](https://github.com/zed-industries/zed/blob/main/crates/gpui/src/platform.rs) has `LayerShell(LayerShellOptions)` on Linux with the `wayland` feature. The Wayland backend creates a `zwlr_layer_surface_v1` and sets size, anchor, margin, keyboard interactivity, and exclusive zone. A shipped example is a clock pinned to the bottom edge, with a transparent background and `KeyboardInteractivity::None`. Top-right is the same call with `Anchor::TOP | Anchor::RIGHT` and a margin. The settings window stays `WindowKind::Normal`. Opening the card does not open that window.

This is the first Rust UI crate we looked at whose own code speaks layer shell. `iced_layershell` is a second project glued to iced. GTK does it through a C library. GPUI does it next to the `div()` that draws the card. Zed also ships on Windows and macOS, so the later ports have a window backend already.

The cost is that GPUI is Zed's internal framework, still pre-1.0. The [README](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md) says breaking changes are expected. crates.io `gpui` 0.2.2 was published on 2025-10-22. Layer shell [landed on 2025-10-29](https://github.com/zed-industries/zed/pull/35610). `cargo add gpui` does not get the popup. As of a Zed discussion in 2026, the current tree still could not be published, because `gpui_macros`, `gpui_shared_string`, and `gpui_util` are marked unpublishable, and Zed said automated releases were not ready yet. Using the feature means a git dependency on a Zed commit, or a third-party republish (`gpui-ce`, `gpui-unofficial`, `gpui-pre`). A git dependency cannot itself be published to crates.io.

Two smaller limits, both from Zed's own issues and the original patch:

- Picking a specific monitor at the moment the first window opens is unreliable. `displays()` can still be empty, and the layer surface is created with output `NULL`, so the compositor chooses. The protocol says that choice is generally the output the user just used. That matches a popup. It is a poor fit for "always monitor 2" during startup.
- Anchor and margin are set when the surface is created. Changing the corner means opening a new surface.

GPUI is the best match for the card if we accept pinning a Zed commit and riding API breaks. `gtk4` plus `gtk4-layer-shell` is the match that stays still. `iced` plus `iced_layershell` sits between them.

## Where the same popup is not available

The protocol registry (same page as the protocol) marks support as of its current table:

| Compositor | Layer shell |
| --- | --- |
| Hyprland 0.52.1 | version 5 |
| KWin 6.7 (KDE Plasma) | version 5 |
| Sway 1.11 | version 4 |
| niri 26.04 | version 5 |
| COSMIC 1.11 | version 5 |
| Mutter 51 (GNOME) | not supported |
| Weston 15 | not supported |

[`gtk-layer-shell`](https://github.com/wmww/gtk-layer-shell/) states the same split: supported on wlroots compositors, KDE Plasma Wayland, and some Mir setups; not supported on GNOME Wayland or any X11 desktop.

So "Arch first" is the easy case, because Omarchy is Hyprland. A later KDE, Sway, or niri build keeps the same popup. A later GNOME build does not. X11 can position a normal window, but this machine is not on X11. Windows and macOS can position a normal window. GNOME Wayland is the hole to remember, and it is not v1.

## Two clocks

These are different settings. Putting both into one "show this card at 15:00" field fights the scheduler.

1. **Memory clock.** The algorithm decides when a card is due. A card answered well today is not due again at the next popup time.
2. **Delivery clock.** A topic has the times of day we are willing to be interrupted. Example: weekdays at 10:00 and 16:00, top-right.

When a delivery time fires, look at cards in that topic whose due time has passed. Show one, the one with the lowest retrievability (most likely forgotten right now). If nothing is due, show nothing. Further due cards wait for the next delivery time, or for the user to dismiss the current card. Do not stack a pile of popups.

"Each question pops at its own clock time" is the setting to avoid. The folder can still hold many cards. The topic owns the interruption schedule.

## Which algorithm

Use [FSRS](https://github.com/open-spaced-repetition/fsrs-rs) through the official Rust crate [`fsrs`](https://docs.rs/fsrs/latest/fsrs/) (7.0.0 on docs.rs). Anki ships this project. Anki 23.10 added it, and Anki's manual still describes the older SM-2 variant as the default you get until FSRS is turned on. FSRS is the one Anki documents as the replacement, and it is the one we can call instead of reimplementing.

`FSRS::default()` uses FSRS-7 weights. Scheduling a card is `next_states(previous_memory, desired_retention, elapsed_days)`. The result has a next state per grade. Anki's default desired retention is 0.90. We store the returned memory state and the due instant. We do not train weights in v1. The same crate can optimize weights later from a review log (`compute_parameters`), and Anki's manual says that optimizer can run on any number of reviews from 24.06.3 onward. v1 just appends the log.

Grades are Again, Hard, Good, Easy.

- **Reveal card.** After the answer is shown, those four buttons map straight onto the four states.
- **Choice card.** Wrong maps to Again, right maps to Good. A self-grade after the explanation can wait.

Duolingo is the wrong model to copy. Their published scheduler is half-life regression (Settles and Meeder, ACL 2016, [A Trainable Spaced Repetition Model for Language Learning](https://aclanthology.org/P16-1174/)). It is fit to language-learning traces. It is not a formula we can drop onto "how does this feature work" cards, and their current production system is not a public spec.

SM-2 is the formula most older flashcard apps copied. It is short enough to implement by hand. Doing that means maintaining a scheduler the `fsrs` crate already supersedes, and throwing away the review log shape Anki's optimizer expects. The crate is the smaller decision.

## What the app reads

An agent (Claude or anything else) writes card files. The app never asks the model to schedule. Scheduling state lives somewhere the model is not invited to edit, or a regenerated card wipes its due date.

Proposed contract, not locked:

```text
topics/<topic-id>/topic.toml     # name, delivery times, corner
topics/<topic-id>/cards/<id>.json
```

Reveal:

```json
{
  "id": "session-cookie",
  "type": "reveal",
  "prompt": "What does the auth middleware check before the handler runs?",
  "answer": "The session cookie signature, then the expiry."
}
```

Choice:

```json
{
  "id": "where-sessions-live",
  "type": "choice",
  "prompt": "Where does this app store sessions?",
  "choices": [
    { "id": "a", "text": "A signed cookie, no server row" },
    { "id": "b", "text": "Redis, keyed by the cookie value" },
    { "id": "c", "text": "A sessions table in Postgres" }
  ],
  "answer": "c",
  "explain": "Login inserts a row. The cookie only holds the session id."
}
```

`type` is the extension point. Later types are new strings. The app ignores a type it does not know and continues with the ones it does. Review state (memory state, due time, history) is app-owned and is not a field in these files.

A topic is a folder. The folder can be a project, a stack, or anything else. The format does not care. v1 needs no cloud account and no model inside the app. Dropping a file in the folder is the whole authoring path.

## Background process

The process has to stay running. A popup cannot be raised by a dead process. How it is started is still open: a systemd user service, or a Hyprland `exec-once` line. Either way it is one process, with the settings window created only when asked.

Closing the card unmaps that surface. It does not quit the process.

Omarchy's theme file is `~/.local/state/omarchy/current/theme/colors.toml`. On this machine it has `mode`, `accent`, `background`, `foreground`, and the usual palette. The card can read `accent` when we care about matching the desktop. That is independent of the GUI crate. Omarchy's own apps are C++ and Qt. This app does not need to be, because the theme is a toml file and the popup is a Wayland surface any client can create.

## Prototype

Paused here on 2026-11-18. The crate is `retention-popup` in this repo. It is a test of the corner and of a game-like card, not the retention app.

Run it from this directory with `mise exec rust@stable -- cargo run --release`. Rust on this machine is rustc 1.100.0, installed through mise and not activated on `PATH`, so a bare `cargo` fails until that changes. A centered control card opens on start (namespace `retention-controls`). Pick a delay (5, 10, or 30 seconds) and a corner, then press Show card. After the delay a second surface opens (namespace `retention-card`, layer top, 16px margin, exclusive zone 0). Close is the × on the card. Quit is on the control card. Both launches on this session exited cleanly when the window was closed.

The control card is a plain Tokyo-night panel. The corner card is a gold-bordered quest card (violet gradient, two choices). Around it is a bluish haze that keeps moving. That haze is a wgpu fragment shader drawn with iced's `Shader` widget (`iced::widget::shader`), in `src/smoke.rs`. iced 0.14 already renders with wgpu, so no particle crate was added. Nothing we found can inject particles into iced's own render pass. The shader is the library.

What the haze taught us, because the first versions looked wrong:

- The layer surface is a rectangle. Anything we paint, including smoke, is clipped to it. The extra area has to be larger than the card and cleared transparent, or the halo is cut off. Current surface is 652×552, with 96px of padding around the card (`smoke::PAD`).
- Value noise (a hash on a grid) reads as soft blocks. Gradient noise plus two domain warps curls into wisps. That is the version that looked like smoke.
- If the haze still has alpha at the surface edge, the clip reads as a square around the smoke. Alpha has to reach 0 before that edge. The shader fades across the outer 48px and also dies before the padding runs out.
- iced's default antialiasing uses a multisampled pass. The smoke pipeline is one sample. The prototype turns antialiasing off so the shader can draw into the pass that already exists. Turning it back on without matching the sample count will hide the haze or panic when the popup opens.
- `iced` needs the `smol` feature or `iced::time::every` does not exist. That feature also selects the executor `iced_layershell` runs on.
- `Cargo.lock` pins `winit-common` and `winit-core` to `0.31.0-beta.2`. `0.31.0-beta.4` adds a key code that `iced_layershell` 0.19 cannot match, and the build breaks. Do not unpinned-update those two.

Hyprland still only places the surface and plays `layersIn` / `layersOut`. The card, the type, and the smoke are pixels we paint. A later effect stays inside this surface, or the surface grows and the new margin stays transparent.

## Open decisions

1. UI crate. Suggestion, and what the prototype uses: `iced` + `iced_layershell`. It is a normal crates.io dependency, `daemon()` plus corner anchors match a background card, and a wgpu shader on the card already looks like a game hover. GPUI remains the alternative if a later card needs Zed's layout more than this shader path. raylib cannot place the card on this desktop.
2. Shape of a topic's delivery schedule: a list of clock times, an interval, quiet hours, per weekday or not.
3. Where topic folders live on disk, and where the review log lives.
4. Autostart: systemd user service, or Hyprland `exec-once`.
5. v1 reveal card: four grades, or only "knew it" / "missed it" mapped to Good and Again.
6. Whether the card may ever use the `overlay` layer. Default in this note is `top`.

## Sources

- Layer shell protocol, including anchor, margin, exclusive zone, keyboard interactivity, and the compositor support table: https://wayland.app/protocols/wlr-layer-shell-unstable-v1
- `xdg-shell` window geometry is surface-local, not a screen position: https://wayland.app/protocols/xdg-shell
- `winit` `set_outer_position`: unsupported on Wayland. https://rust-windowing.github.io/winit/winit/window/trait.Window.html
- smithay-client-toolkit layer shell: https://docs.rs/smithay-client-toolkit/latest/smithay_client_toolkit/shell/ and the `simple_layer` example
- `iced_layershell` 0.19.1: https://crates.io/crates/iced_layershell and https://docs.rs/iced_layershell/latest/iced_layershell/
- Slint backends (winit on Wayland): https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_winit/
- `spell_framework` (Slint on layer shell, breaking changes expected): https://docs.rs/spell-framework/latest/spell_framework/
- gtk-layer-shell supported desktops: https://github.com/wmww/gtk-layer-shell/
- Hyprland layer surfaces and layer rules: https://wiki.hypr.land/0.46.0/Configuring/Keywords/
- Hyprland layer animations: https://wiki.hypr.land/0.48.0/Configuring/Animations/
- This machine, 2026-11-18: `hyprctl version` reports Hyprland 0.56.2. `hyprctl layers` shows `omarchy-background`, `omarchy-bar`, `omarchy-dock`, `omarchy-notifications`.
- Anki algorithms: https://docs.ankiweb.net/faqs/what-spaced-repetition-algorithm and https://docs.ankiweb.net/manual/deck-options
- Anki 23.10 FSRS integration, via `fsrs-rs`: https://docs.ankiweb.net/releases/changes/changes/23.10
- `fsrs` crate 7.0.0: https://docs.rs/fsrs/latest/fsrs/ and https://github.com/open-spaced-repetition/fsrs-rs/
- GPUI: https://gpui.rs/ and https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md
- GPUI layer shell, merged 2025-10-29: https://github.com/zed-industries/zed/pull/35610 and `WindowKind::LayerShell` in https://github.com/zed-industries/zed/blob/main/crates/gpui/src/platform.rs
- crates.io `gpui` 0.2.2, published 2025-10-22, before that merge: https://crates.io/crates/gpui
- Publishing status: https://github.com/zed-industries/zed/discussions/55271
- Monitor selection limit: https://github.com/zed-industries/zed/issues/46378
- Duolingo's published model: https://aclanthology.org/P16-1174/
