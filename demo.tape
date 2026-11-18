# Recording for the README, rendered with https://github.com/charmbracelet/vhs
#
#   cargo install --path . --locked
#   vhs demo.tape
#
# Walks through the settings mode: each key opens a list, the screen follows
# the highlighted line, and Enter keeps it. A throwaway config file keeps the
# recording from writing over whatever is really set.

Output demo.gif

Set Shell bash
Set FontSize 16
Set Width 1100
Set Height 620
Set Padding 10
Set Framerate 30
Set PlaybackSpeed 1
Set TypingSpeed 40ms

Hide
Type "binkam --config /tmp/binkam-demo.toml --anim starfield --theme nord"
Enter
Sleep 1s
Show

# Starfield, with the key reminder along the bottom.
Sleep 3s

# t opens the theme list; the whole screen follows the highlighted line.
Type "t"
Sleep 1500ms
Down
Sleep 1200ms
Down
Sleep 1200ms
Down
Sleep 1500ms
Enter
Sleep 1500ms

# f changes the glyphs the clock is drawn with.
Type "f"
Sleep 1200ms
Down
Sleep 1500ms
Down
Sleep 1500ms
Enter
Sleep 1500ms

# b changes how big it is drawn.
Type "b"
Sleep 1000ms
Down
Sleep 2s
Enter
Sleep 1500ms

# a changes the animation, which also starts as soon as it is highlighted.
Type "a"
Sleep 1200ms
Down
Sleep 2500ms
Down
Sleep 2500ms
Down
Sleep 2500ms
Enter
Sleep 2s

# ? has every key, including the ones too wide for the bar.
Type "?"
Sleep 3500ms
Escape
Sleep 500ms

Type "q"
Sleep 500ms
