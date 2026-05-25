# Animated terminal recordings (vhs)

The Hands-on guide on docs.appcontrol.io embeds an animated GIF of
`scripts/methodology-walkthrough.sh` driving every methodology phase.
The recording is **scripted** with [vhs](https://github.com/charmbracelet/vhs),
not captured live, so it stays deterministic across re-renders.

## Files

| Path | What it is |
|---|---|
| `methodology-walkthrough.tape` | vhs tape that drives the walkthrough script |
| `../screenshots/methodology-walkthrough.gif` | rendered output (auto-generated) |

## Re-render locally

```bash
# Prereqs (Ubuntu / Debian)
sudo apt-get install -y ffmpeg
curl -L https://github.com/tsl0922/ttyd/releases/latest/download/ttyd.x86_64 \
  -o /usr/local/bin/ttyd && sudo chmod +x /usr/local/bin/ttyd
curl -L https://github.com/charmbracelet/vhs/releases/latest/download/vhs_Linux_x86_64.tar.gz \
  | sudo tar -xz -C /usr/local/bin/ vhs

# Then bring up a stack so the recording has a backend to call
docker compose -f docker/docker-compose.yaml up -d
until curl -fsS http://localhost:3000/health > /dev/null; do sleep 2; done

# Render
vhs docs/recordings/methodology-walkthrough.tape
```

The GIF lands in `docs/screenshots/methodology-walkthrough.gif` and
is committed alongside the regular Playwright captures. CI runs the
same render on every push to `main` that touches the tape, the
script, or the example payloads, then commits the refreshed GIF.

## Why vhs and not Playwright?

The walkthrough is a CLI demo, not a UI demo. Playwright records
browser windows; recording the terminal would require shelling out
to ttyd anyway, which is exactly what vhs does already — with the
added benefit of a declarative `.tape` script that's reviewable in
PRs.
