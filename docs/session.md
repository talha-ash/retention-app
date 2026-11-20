# Start retention-popup with Hyprland

On this machine the start line is in `~/.config/hypr/autostart.lua`.

```lua
o.launch_on_start("retention-popup")
```

Hyprland runs that line when the session starts. A reload does not run it again.

`retention-popup` is the binary from this repo. `~/.local/bin/retention-popup` points at `target/release/retention-popup`.

A config that still uses `hyprland.conf` uses this line instead.

```
exec-once = retention-popup
```

Dismissing the corner card does not exit the process. Quit on the control card does. A second start exits while the first process is still running.
