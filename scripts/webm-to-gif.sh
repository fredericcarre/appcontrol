#!/usr/bin/env bash
# scripts/webm-to-gif.sh
#
# Convert every .webm under frontend/e2e-screenshots/gif-videos/
# (saved by capture-gifs.spec.ts via page.video().saveAs()) into an
# optimized .gif in docs/screenshots/.
#
# Two-pass palette method:
#   pass 1: generate a 256-colour palette from the source frames
#   pass 2: encode the GIF using that palette via paletteuse
# This yields ~5–10× smaller files at the same perceived quality
# compared to the single-pass `fps=15,scale,split,palettegen,
# paletteuse` filter chain.

set -euo pipefail

ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
VIDEO_DIR="${VIDEO_DIR:-$ROOT/frontend/e2e-screenshots/gif-videos}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/docs/screenshots}"
FPS="${FPS:-15}"
WIDTH="${WIDTH:-1024}"
# Playwright starts recording the moment the browser context is
# created, so every video opens with 2-3 seconds of "about:blank"
# while realLogin() / openFirstAppMap() set up cookies and navigate.
# GitHub renders the first frame of a GIF as the preview thumbnail,
# so a blank first frame meant the README showed empty rectangles
# until the reader clicked play. Skipping the dead lead lets the
# GIF (and its thumbnail) open on rendered content.
TRIM_START="${TRIM_START:-3}"

log() { echo "[webm-to-gif] $*"; }
fail() { echo "[webm-to-gif] ERROR: $*" >&2; exit 1; }

command -v ffmpeg >/dev/null 2>&1 || fail "ffmpeg not installed"

if [ ! -d "$VIDEO_DIR" ]; then
  log "No video directory at $VIDEO_DIR — nothing to convert"
  exit 0
fi

mkdir -p "$OUTPUT_DIR"

shopt -s nullglob
videos=("$VIDEO_DIR"/*.webm)
shopt -u nullglob

if [ "${#videos[@]}" = "0" ]; then
  log "No .webm files in $VIDEO_DIR — nothing to convert"
  exit 0
fi

log "Found ${#videos[@]} video(s)"

for video in "${videos[@]}"; do
  name=$(basename "$video" .webm)
  gif="$OUTPUT_DIR/$name.gif"
  palette=$(mktemp --suffix=.png)
  log "→ $name.webm → $name.gif"

  ffmpeg -hide_banner -loglevel error -y -ss "$TRIM_START" -i "$video" \
    -vf "fps=$FPS,scale=$WIDTH:-1:flags=lanczos,palettegen=max_colors=256" \
    "$palette"

  ffmpeg -hide_banner -loglevel error -y -ss "$TRIM_START" -i "$video" -i "$palette" \
    -filter_complex "fps=$FPS,scale=$WIDTH:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=5" \
    -loop 0 \
    "$gif"

  rm -f "$palette"
  size_kb=$(($(stat -c%s "$gif") / 1024))
  log "   wrote $gif (${size_kb} KB)"
done

log "Done. Wrote $(ls "$OUTPUT_DIR"/*.gif 2>/dev/null | wc -l) GIF(s)."
