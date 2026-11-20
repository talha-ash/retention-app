# Start retention-popup with Hyprland

Add one line to `~/.config/hypr/hyprland.conf`. Hyprland starts the program when the session starts. This file does not change that config.

```
exec-once = retention-popup
```

`retention-popup` is the binary from this repo. Hyprland finds it on `PATH`. The release build writes `target/release/retention-popup`.

Dismissing the corner card does not exit the process. Quit on the control card does. A second start exits while the first process is still running.
