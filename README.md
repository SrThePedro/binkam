# kumsaati

An animated, clock-centered terminal screensaver. A big block-font clock sits on
top of a changeable background effect; any key press puts the terminal back the
way it was.

Built with [ratatui](https://ratatui.rs). Works on Linux and Windows.

## Install

```bash
cargo install --path .
```

Or run it straight from the repository:

```bash
cargo run --release
```

## Usage

```bash
kumsaati                                  # random animation, changing every few minutes
kumsaati --anim fire                      # pin one animation
kumsaati --anim fire --anim snow          # cycle between just these two
kumsaati --theme nord --clock-position drift
kumsaati --preview                        # browse the combinations
kumsaati --list                           # list the animations
kumsaati --list-themes                    # list the themes
kumsaati --print-config                   # print the settings this run would use
```

Any key quits, except in `--preview`, where `n` and `p` change animation, `t`
changes theme, `c` moves the clock and `q` or `Esc` quits.

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

### Clock positions

`center` and `corner` stay put. `bounce` travels and reflects off the edges,
taking a new colour on every hit. `drift` crawls along a slow path, which keeps
a static image off one set of pixels.

## Configuration

Settings come from three layers, each beating the one before it: the built-in
defaults, then the config file, then the command line. No config file is needed;
without one the defaults apply.

| Platform | Path |
| --- | --- |
| Linux | `~/.config/kumsaati/config.toml` |
| Windows | `%APPDATA%\kumsaati\config\config.toml` |

`kumsaati --config path/to/file.toml` reads somewhere else instead, and
`kumsaati --print-config` prints the settings a run would end up with, which is
the quickest way to check a file is being picked up.

Every key is optional. A broken file is reported with the line and column that
went wrong, rather than being ignored or crashing.

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

# What the clock shows.
show_seconds = true
show_date = true

# 24 for 00-23, or 12 for 01-12 with a ÖÖ/ÖS marker on the date line.
hour_format = 24

# Darken the animation right behind the clock, so the digits stay legible.
clock_backdrop = true

# Fix the randomness for a reproducible picture. Left out, every run differs.
# seed = 42
```

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --dump-frame                      # one frame as plain text
cargo run -- --dump-frame --anim matrix --size 80x24
```

`--dump-frame` draws a single frame to an off-screen backend and prints it, so
nothing touches the real terminal.

## License

Not yet chosen.
