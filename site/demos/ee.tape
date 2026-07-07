# Demo recording for `ee` — the env file picker.
#
# Requires `vhs` (https://github.com/charmbracelet/vhs) plus ffmpeg and ttyd.
# `ee` must be on PATH, and $HOME/.config/envvars/ should contain a few
# sample files (this repo's own recording used prod.env/staging.env/local.env).
#
# Run from the repo root:
#   vhs site/demos/ee.tape

Output "site/demos/ee.gif"

Set Shell "bash"
Set FontSize 16
Set Width 900
Set Height 420
Set Theme "Dracula"

Sleep 500ms
Type "ee"
Enter
Sleep 1s
Type "prod"
Sleep 500ms
Screenshot "site/demos/ee.png"
Sleep 500ms
Escape
