# vhs tape — animated demonstration of the methodology walkthrough
# script. Renders to docs/screenshots/methodology-walkthrough.gif.
#
# Render locally:
#   vhs docs/recordings/methodology-walkthrough.tape
#
# Prerequisites: vhs + ttyd + ffmpeg on PATH, plus a running stack
# reachable on http://localhost:3000 (docker compose up does that).

Output docs/screenshots/methodology-walkthrough.gif

Set Shell bash
Set FontSize 13
Set Width 1200
Set Height 720
Set Padding 18
Set Theme "Catppuccin Mocha"
Set TypingSpeed 50ms
Set PlaybackSpeed 1.0

# Make the prompt minimal so the recording focuses on the script's
# coloured output and not the prompt cruft.
Hide
Type "export PS1='$ '" Enter
Type "clear" Enter
Show

# Title card — a brief comment so the viewer understands what they
# are about to see before the script starts scrolling.
Type "# AppControl — methodology walkthrough"
Sleep 700ms
Enter
Type "# Phases 1 → 5 driven via the API in ~20 seconds"
Sleep 700ms
Enter
Type ""
Enter

# Kick off the walkthrough. The script handles login, import,
# activation moves, annotations, knowledge promotion, pattern
# creation and propagation, knowledge summary, and optional Git
# push — each step prints a bold heading + ✓ checkmarks.
Type "./scripts/methodology-walkthrough.sh"
Sleep 600ms
Enter

# Let the script run. 35s is a comfortable upper bound on the
# wall-clock time of the walkthrough against a freshly-seeded stack.
Sleep 35s

# Final pause so the closing 'All phases exercised' message lingers
# on screen before the GIF loops.
Sleep 2500ms
