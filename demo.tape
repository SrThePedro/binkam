# Recording for the README, rendered with https://github.com/charmbracelet/vhs
#
#   cargo install --path .
#   vhs demo.tape
#
# Runs preview mode and steps through a few combinations by hand, so one
# recording shows several animations, themes and clock positions.

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
Type "kumsaati --preview --anim starfield --theme nord"
Enter
Sleep 1s
Show

# Starfield, the default look.
Sleep 4s

# n walks forward through the animations.
Type "n"
Sleep 3500ms
Type "n"
Sleep 3500ms
Type "n"
Sleep 3500ms

# t cycles the themes on whatever is on screen.
Type "t"
Sleep 2500ms
Type "t"
Sleep 2500ms

# c moves the clock: centre, bounce, corner, drift.
Type "c"
Sleep 4s
Type "c"
Sleep 2500ms

Type "q"
Sleep 500ms
