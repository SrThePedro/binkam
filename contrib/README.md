# contrib

Optional extras. Nothing here is needed to run binkam.

## `binkam-fullscreen`

A shell script that opens binkam as a full-screen screensaver window: it finds
a terminal emulator, asks it for a full-screen window, and runs `binkam --lock`
inside it, so the first key press or mouse movement closes the lot. It is meant
for a key binding, but it works just as well from a launcher or an idle daemon.

```bash
install -Dm755 contrib/binkam-fullscreen ~/.local/bin/binkam-fullscreen
```

Pressing the key again while a window is open does nothing, so leaning on it
cannot pile up a dozen of them. Anything you pass goes on to binkam:

```bash
binkam-fullscreen --anim matrix --theme nord
```

Three environment variables change what it does:

| Variable | Default | What it is for |
| --- | --- | --- |
| `BINKAM` | the first binkam found | A binary that is not on `PATH` |
| `BINKAM_TERMINAL` | the first terminal found | A terminal the script does not know, or one you prefer |
| `BINKAM_APP_ID` | `binkam` | The window's app-id and title, which window rules match on |

It knows how to ask these for a full-screen window: foot, kitty, ghostty,
alacritty, konsole, gnome-terminal, xfce4-terminal and xterm. wezterm, and
anything named through `BINKAM_TERMINAL`, is started as an ordinary window and
needs a window rule to go full screen. Every window is given the app-id and
title `binkam`, which is what such a rule can match on.

## Binding it to a key

A key bound in a desktop environment rarely has the same `PATH` as a shell, so
spell the path out. (The script looks in `~/.cargo/bin` and `~/.local/bin` for
binkam itself for the same reason.)

### Hyprland

With the Lua configuration:

```lua
hl.bind(
    "SUPER + B",
    hl.dsp.exec_cmd(os.getenv("HOME") .. "/.local/bin/binkam-fullscreen")
)
```

With `hyprland.conf`:

```ini
bind = SUPER, B, exec, ~/.local/bin/binkam-fullscreen
```

### sway and i3

```
bindsym $mod+b exec ~/.local/bin/binkam-fullscreen
```

A terminal with no full-screen flag of its own can be handled by the compositor
instead — on sway:

```
for_window [app_id="binkam"] fullscreen enable
```

### KDE Plasma

System Settings → Keyboard → Shortcuts → Add New → Command or Script, with the
path written in full (`/home/you/.local/bin/binkam-fullscreen`), then click the
shortcut field and press the keys.

### GNOME

GNOME keeps custom shortcuts in dconf. This adds one; note that the first line
replaces the whole list, so read it first if you already have some:

```bash
path=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/binkam/
schema=org.gnome.settings-daemon.plugins.media-keys

gsettings set $schema custom-keybindings "['$path']"
gsettings set $schema.custom-keybinding:$path name 'binkam'
gsettings set $schema.custom-keybinding:$path command "$HOME/.local/bin/binkam-fullscreen"
gsettings set $schema.custom-keybinding:$path binding '<Super>b'
```

### Xfce

Settings → Keyboard → Application Shortcuts → Add, with the path written in
full.

### Windows

The script is for Unix shells, but Windows Terminal can do the same in one
line, and a shortcut can carry a hotkey:

1. Right-click the desktop → New → Shortcut, with the target
   `wt.exe --fullscreen binkam --lock`.
2. Move the shortcut into
   `%AppData%\Microsoft\Windows\Start Menu\Programs` — Windows only honours
   the hotkey of a shortcut on the desktop or in the Start Menu.
3. Right-click it → Properties → Shortcut key, and press the keys you want.
