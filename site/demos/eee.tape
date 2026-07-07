# Demo recording for `eee` — the repo picker.
#
# Requires `vhs` (https://github.com/charmbracelet/vhs) plus ffmpeg and ttyd.
# `eee` must be on PATH, and $HOME/github.com/<org>/<repo> should contain a
# few sample repo directories (this repo's own recording used
# acme/frontend, acme/backend, acme/infra).
#
# Run from the repo root:
#   vhs site/demos/eee.tape

Output "site/demos/eee.gif"

Set Shell "bash"
Set FontSize 16
Set Width 900
Set Height 420
Set Theme "Dracula"

Sleep 500ms
Type "eee"
Enter
Sleep 1s
Screenshot "site/demos/eee.png"
Sleep 500ms
Escape
