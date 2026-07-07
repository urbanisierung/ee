# Demo recording for `rr` — per-directory command shortcuts.
#
# Requires `vhs` (https://github.com/charmbracelet/vhs) plus ffmpeg and ttyd.
# `rr` must be on PATH. This recording assumes a directory with a couple of
# named commands configured and no default, e.g.:
#
#   rr set build -- pnpm run build
#   rr set test -- pnpm test --watch
#
# so that bare `rr` opens the picker shown here.
#
# Run from the repo root:
#   vhs site/demos/rr.tape

Output "site/demos/rr.gif"

Set Shell "bash"
Set FontSize 16
Set Width 900
Set Height 420
Set Theme "Dracula"

Sleep 500ms
Type "cd $HOME/github.com/acme/frontend"
Enter
Sleep 500ms
Type "rr"
Enter
Sleep 1s
Screenshot "site/demos/rr.png"
Sleep 500ms
Escape
