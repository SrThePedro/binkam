# binkam

[![CI](https://github.com/SrThePedro/binkam/actions/workflows/ci.yml/badge.svg)](https://github.com/SrThePedro/binkam/actions/workflows/ci.yml)

An animated, clock-centered terminal screensaver. A big block-font clock sits on
top of a changeable background effect, and everything about it can be set from
inside it: a key reminder sits along the bottom, each key opens a list, and the
screen follows the highlighted line as you move through it. Choices are written
back to the config file, so the next run starts where the last one left off.

Built with [ratatui](https://ratatui.rs). Tested on Linux, macOS and Windows.

## Install

### A prebuilt binary

Every [release](../../releases) carries binaries for Linux, macOS and Windows,
with a `SHA256SUMS` file to check them against. No toolchain needed:

```bash
# Linux, x86_64. The release page lists the other targets.
curl -fsSL https://github.com/SrThePedro/binkam/releases/latest/download/binkam-x86_64-unknown-linux-musl.tar.gz | tar -xz
install -Dm755 binkam ~/.local/bin/binkam
```

The `musl` build is statically linked, so it runs on any distribution whatever
its glibc. The `gnu` one is the ordinary dynamically linked build. On Windows,
unzip it and put `binkam.exe` anywhere on `PATH`.

### With cargo, straight from the repository

```bash
cargo install --git https://github.com/SrThePedro/binkam --locked
```

That puts `binkam` in `~/.cargo/bin`, which has to be on `PATH`; `cargo` says
so if it is not. `cargo uninstall binkam` removes it again.

`--locked` builds against the versions in `Cargo.lock`, the ones the tests ran
against. Without it `cargo install` takes the newest compatible dependencies
instead, which is usually fine and occasionally not.

### From a clone

```bash
cargo install --path . --locked   # install it
cargo run --release               # or just run it
```

Building needs a Rust toolchain of 1.88 or newer, because of let-chains.

## Usage

```bash
binkam                                  # random animation, changing every few minutes
binkam --anim fire                      # pin one animation
binkam --anim fire --anim snow          # cycle between just these two
binkam --theme nord --clock-position drift
binkam --clock-font tall --clock-scale 2
binkam --lock                           # plain screensaver: any key quits
binkam --list                           # list the animations
binkam --list-themes                    # list the themes
binkam --list-fonts                     # list the clock fonts
binkam --print-config                   # print the settings this run would use
```

Anything given on the command line only applies to that run; it is the config
file, and the settings mode below, that stick.

## Settings mode

This is what plain `binkam` does. The keys along the bottom each open a list in
the top left corner:

| Key | Opens |
| --- | --- |
| `a` | Animation, or all of them in turn |
| `t` | Theme |
| `k` | Where the clock sits |
| `f` | Clock font |
| `b` | Clock size |
| `s` | Seconds |
| `d` | Date line |
| `h` | 12 or 24 hour |
| `r` | How much colour to use |
| `g` | The darkening behind the clock |
| `c` | How often the animation changes |
| `z` | Frame rate |

Inside a list, <kbd>↑</kbd> and <kbd>↓</kbd> (or <kbd>k</kbd> and <kbd>j</kbd>)
move, and the screen follows at once — picking a theme by eye is the point.
<kbd>Enter</kbd> keeps the highlighted line and writes it to the config file;
<kbd>Esc</kbd> puts back whatever the list opened on.

Outside a list, `n` and `p` change animation without opening one, `?` shows the
whole list of keys, and `q` or <kbd>Esc</kbd> quits. A key that means nothing
does nothing: a mistyped letter should not throw away what you were setting up.

The bar shows as much as the terminal is wide enough for, down to `? tuşlar`
alone, and `?` has the rest.

### `--lock`

`--lock` turns all of that off: no bar, no lists, and any key press or mouse
movement quits. That is what a lock or idle command wants, and it is what the
examples below use. It is also the only mode that captures the mouse, so in the
settings mode the terminal's own selection and scrolling keep working.

A frame of `starfield` on an 80 column terminal, as `--dump-frame` prints it:

```
                               +                     +                   +
                                                                    +
                                                                .
                               +          .
                                                        +
                  .
                   ██ ██  ██    ██████ ██████    ██████ ██████
                   ██ ██  ██ ██ ██    .  * ██ ██   . ██     ██
             .     ██ ██████    ██████ ██████    ██████     ██       .
                   ██     ██ ██     ██ ██     ██ ██         ██
                   ██     ██    ██████ ██████    ██████    .██      .
                                                                +
                          16 Kasım 2026, Pazartesi.               *

.               .                               *            +
       +                       .                         .  .       .
                                            .                             +
                                                     +        .
```

In colour and in motion it looks rather better. To record it:

```bash
cargo install --path .
vhs demo.tape          # writes demo.gif; see https://github.com/charmbracelet/vhs
```

### Animations

| Name | What it looks like |
| --- | --- |
| `starfield` | Stars streaming out from the centre of the screen |
| `matrix` | Falling character rain with fading trails |
| `fire` | DOOM style flames, drawn with half-block pixels |
| `plasma` | Smooth truecolor plasma from a sum of sine waves |
| `life` | Conway's Game of Life, with trails behind dying cells |
| `snow` | Drifting snowflakes that sway as they fall |
| `rain` | Slanted rain with splashes where it lands |
| `waves` | Overlapping sine waves as thin lines |
| `pipes` | Pipes growing across the screen, wiped once it fills up |

### Themes

`default` keeps each animation's own palette. `mono`, `amber`, `nord` and
`gruvbox` re-colour everything: each pixel keeps the brightness the animation
chose and takes its hue from the theme, so fire still rises and matrix still
fades its trails, just in the theme's colours.

### Colour

Everything is drawn in truecolor and reduced once per frame, so no animation
has to know what the terminal supports.

| `--color` | What happens |
| --- | --- |
| `auto` (default) | Read `COLORTERM`, `TERM` and `NO_COLOR` and pick one of the below |
| `truecolor` | 24 bit colour, exactly what was drawn |
| `256` | Snapped to the 256 colour palette |
| `none` | No colour at all; half-block pixels turn back into block characters, so fire and plasma keep their shapes |

`--no-color` is shorthand for `--color none`, and `NO_COLOR=1` in the
environment does the same.

### Clock positions

`center` and `corner` stay put. `bounce` travels and reflects off the edges,
taking a new colour on every hit. `drift` crawls along a slow path, which keeps
a static image off one set of pixels.

### Clock fonts and size

| Font | |
| --- | --- |
| `block` | The default: 5 rows, 47 columns for `HH:MM:SS` |
| `shadow` | `block` with a lighter block offset by one cell, for an edge |
| `slim` | One-column strokes; 33 columns, for a narrow terminal |
| `tall` | 7 rows, for a screen with height to spare |

`clock_scale` goes from 1 to 4 and turns every glyph pixel into a square that
many cells on a side, so a font grows without changing shape.

Asking for more than the terminal has is not an error. The clock walks back
down until something fits — that size, that size without the seconds, then one
smaller, and so on, falling back to plain text on a terminal too small for any
of it. The size asked for is remembered, so widening the terminal brings it
back.

## A key that starts it

A screensaver worth having is one you can also summon.
`contrib/binkam-fullscreen` opens a full-screen terminal running `binkam
--lock`, so the first key press or mouse movement gives the screen back. It
finds a terminal itself, hands anything you give it on to binkam, and does
nothing at all while a window is already open.

```bash
install -Dm755 contrib/binkam-fullscreen ~/.local/bin/binkam-fullscreen
```

Then bind it. Hyprland, with the Lua configuration:

```lua
hl.bind(
    "SUPER + B",
    hl.dsp.exec_cmd(os.getenv("HOME") .. "/.local/bin/binkam-fullscreen")
)
```

or with `hyprland.conf`:

```ini
bind = SUPER, B, exec, ~/.local/bin/binkam-fullscreen
```

A key bound in a desktop environment rarely has the same `PATH` as a shell,
which is why the path is spelled out; the script looks in `~/.cargo/bin` for
binkam for that same reason. [`contrib/README.md`](contrib/README.md) has the
same for sway, KDE, GNOME, Xfce and Windows, the terminals the script knows,
and the environment variables that override its choices.

## Running it when the machine goes idle

`binkam` is a screensaver, not a screen locker. Any key press or mouse
movement ends it, and it never asks for a password. If you want the screen
actually locked, run a locker and let this decorate the wait, or use it only
inside a terminal multiplexer.

### tmux

tmux can run it as its lock command, which is the least fuss of any of this:

```tmux
# ~/.tmux.conf
set -g lock-after-time 300
set -g lock-command "binkam --lock"
```

Five minutes without input in a session and the clock takes over the attached
client; the first key gives the shell back. `tmux lock-session` runs it
straight away, which is the quickest way to check the setting took.

The timer counts activity per session, so a session running something chatty
never goes idle.

### Hyprland, with hypridle

```ini
# ~/.config/hypr/hypridle.conf
listener {
    timeout = 300
    # Only start one, however many times the listener fires.
    on-timeout = pidof binkam > /dev/null || foot --fullscreen -- binkam --lock
}
```

Any terminal that can start fullscreen will do: `alacritty -o
'window.startup_mode="Fullscreen"' -e binkam --lock`, `kitty
--start-as=fullscreen binkam --lock`, and so on. (Alacritty reads `-o` as
TOML, and quietly ignores the option if the value is not quoted.)
`contrib/binkam-fullscreen` knows the spelling for a dozen terminals and
picks one for you.

To have the screen really locked as well, keep `hyprlock` on a longer timeout
and let `binkam` fill the time before it.

### sway, or anything else with swayidle

```bash
swayidle -w timeout 300 'pidof binkam > /dev/null || foot --fullscreen -- binkam --lock'
```

## Configuration

Settings come from three layers, each beating the one before it: the built-in
defaults, then the config file, then the command line. No config file is needed;
without one the defaults apply.

| Platform | Path |
| --- | --- |
| Linux | `~/.config/binkam/config.toml` |
| Windows | `%APPDATA%\binkam\config\config.toml` |

`binkam --config path/to/file.toml` uses somewhere else instead, for both
reading and saving; a file that is not there yet is fine, and the first save
creates it. `binkam --print-config` prints the settings a run would end up
with, which is the quickest way to check a file is being picked up.

Every key is optional. A broken file is reported with the line and column that
went wrong, rather than being ignored or crashing.

Pressing <kbd>Enter</kbd> on a list writes this file, so there is no need to
edit it by hand. A saved file holds every setting, with one exception: `seed`
is never written, because it pins the randomness for debugging and a saved one
would hand every later run the same picture.

### Example `config.toml`

```toml
# Frames per second. Higher is smoother and costs more CPU.
fps = 30

# Animations to draw from. Leave it out for all of them, give one name to pin
# it, or a list to cycle between just those.
#   anim = "fire"
#   anim = ["fire", "plasma", "waves"]
anim = ["starfield", "matrix", "plasma", "waves"]

# Minutes between automatic animation changes. 0 turns it off, and so does
# naming a single animation above.
cycle_minutes = 3

# default, mono, amber, nord, gruvbox
theme = "nord"

# center, bounce, corner, drift
clock_position = "drift"

# block, shadow, slim or tall, and how many cells wide and tall one glyph
# pixel becomes. Too big for the terminal simply draws smaller.
clock_font = "block"
clock_scale = 1

# What the clock shows.
show_seconds = true
show_date = true

# 24 for 00-23, or 12 for 01-12 with a ÖÖ/ÖS marker on the date line.
hour_format = 24

# Darken the animation right behind the clock, so the digits stay legible.
clock_backdrop = true

# auto, truecolor, 256 or none. auto reads COLORTERM, TERM and NO_COLOR.
color = "auto"

# Fix the randomness for a reproducible picture. Left out, every run differs.
# seed = 42
```

## Performance

Measured with `--bench`, which runs a given number of frames against an
off-screen backend and reports what they cost. The time covers updating,
drawing, theming, colour reduction and ratatui's own diffing; it does not
cover writing to a real terminal.

Release build, 200x50 (a large terminal), 30 fps, on one core:

| Animation | Per frame | Share of a core |
| --- | --- | --- |
| `snow` | 102 µs | 0.30% |
| `starfield` | 102 µs | 0.30% |
| `rain` | 115 µs | 0.35% |
| `pipes` | 126 µs | 0.38% |
| `matrix` | 129 µs | 0.39% |
| `life` | 174 µs | 0.52% |
| `waves` | 176 µs | 0.53% |
| `fire` | 292 µs | 0.87% |
| `plasma` | 429 µs | 1.29% |

At a more usual 80x24 the heaviest, `plasma`, costs 93 µs per frame, or 0.28%
of a core.

Two settings move the numbers: a theme other than `default` adds a pass over
the buffer (`plasma` 1.29% to 1.43%), and `--color 256` adds another
(1.29% to 1.59%).

To produce the table on another machine:

```bash
cargo build --release
for a in $(./target/release/binkam --list); do
  ./target/release/binkam --bench 4000 --size 200x50 --anim "$a" --seed 1
done
```

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --dump-frame                      # one frame as plain text
cargo run -- --dump-frame --anim matrix --size 80x24
cargo run -- --dump-frame --clock-font tall --clock-scale 2
cargo run --release -- --bench 4000            # what a frame costs
```

`--dump-frame` draws a single frame to an off-screen backend and prints it, so
nothing touches the real terminal. `--bench` does the same and times it.

`clippy` is clean at the default level and that is what the build expects.
`clippy::pedantic` is not clean on purpose: most of what is left is numeric
cast warnings in drawing code, where the value is already clamped and a
float-to-integer `as` cast saturates rather than wrapping.

## License

Either of

- MIT ([LICENSE-MIT](LICENSE-MIT), or <https://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE), or
  <https://www.apache.org/licenses/LICENSE-2.0>)

whichever suits you. This is the pair the Rust project itself uses: MIT is
short and permissive, Apache-2.0 adds an explicit patent grant.

Unless you say otherwise, any contribution you deliberately submit for
inclusion in this work is licensed the same way, with no further conditions.
