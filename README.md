# OMP Pet

A macOS desktop pet built in Rust with native AppKit. A small TypeScript oh-my-pi extension sends session events over a private Unix socket. The companion makes no model calls and uses no webview.

## Run

Requires macOS 12+, Xcode Command Line Tools, and Rust. Build and open the accessory app:

```sh
scripts/build-app.sh
open 'dist/OMP Pet.app'
```

Start oh-my-pi with the bridge (an absolute path works from any project):

```sh
omp -e /Users/soham/Documents/code/omp-pet/extension/index.ts
```

The TypeScript bridge uses only Node built-ins at runtime; the OMP import is a type import. Bun is needed only for development checks. The app must be running to show updates; the extension reconnects on later events or its five-second heartbeat when the app restarts.

Hover for task, active tools, context occupancy, and session state. Animated dots mean the agent lifecycle is active; they do not prove token generation or tool progress. The 2-point context bar uses the sprite pack's most common chromatic color, ignoring transparency and near-gray pixels. Context remains unknown when OMP cannot supply it.

Drag the pet near any screen edge to dock it. Leaving it hides it as a 3-point line; hold the cursor at that screen segment for 200 ms to reveal it. Reveal detection includes Dock and menu-bar insets. Placement and sprite choice persist across restarts. The pet and task card are nonactivating fixed-size panels with native tiling exclusion.

Use the menu-bar **π** or right-click the pet to pin the task readout, switch sessions, choose/reload sprites, restore bundled Zorua, reset placement, or quit.

## Custom sprites

Choose a folder containing `manifest.json` and PNGs using **Choose sprite pack…**. Version 1 requires a nonempty `idle` animation. Optional animations: `working`, `waiting`, `compacting`, `error`, `disconnected`, and `celebrate` (plays once after successful work). Missing states fall back to idle. Static sprites work as a single-frame animation.

```json
{
  "version": 1,
  "frame_ms": 200,
  "pixel_art": true,
  "idle": ["idle-1.png", "idle-2.png"],
  "working": [
    {"file": "walk.png", "x": 0, "y": 0, "width": 32, "height": 40, "duration_ms": 100},
    {"file": "walk.png", "x": 32, "y": 0, "width": 32, "height": 40, "duration_ms": 100}
  ]
}
```

Crop coordinates start at the PNG's top-left corner. Durations must be 80–2000 ms. Paths stay within the pack; limits are 128 frames, 16 MiB encoded PNGs, and four million decoded pixels. Sprites scale to fit without changing the panel size. Animation timers stop while tucked. **Reload sprites** updates edited files and recalculates the accent.

The bundled [Zorua pack](assets/zorua/CREDITS.md) uses attributed PMDCollab artwork with idle, walk, look-up, charge, hurt, sleep, and hop animations. Sprite rights are separate from the app's code license.

## Development

```sh
cargo test
bun install --ignore-scripts
bun test
bun run typecheck
python3 scripts/check-sprites.py
scripts/build-app.sh debug
```

Local controls return JSON acknowledgments:

```sh
python3 scripts/petctl.py status
python3 scripts/petctl.py readout
python3 scripts/petctl.py tuck
python3 scripts/petctl.py reveal
python3 scripts/petctl.py load_sprites --pack /path/to/my-pack
python3 scripts/petctl.py quit
```

`python3 scripts/demo.py` sends a temporary demonstration session. `OMP_PET_SOCKET` overrides the socket in both processes. `OMP_PET_SPRITES` overrides the sprite folder; `OMP_PET_STATE` overrides the preferences file. Defaults are `/tmp/omp-pet-<uid>/events.sock` inside an owned 0700 directory and `~/Library/Application Support/OMP Pet/preferences.json`.

Tests cover lifecycle/approval/error behavior through a real Unix socket, context handling, UTF-8 bounds, sequence resets on reconnect, stale socket events, independent sessions, screen geometry, palette filtering, and framing limits. Real OMP model-session use and third-party tiling utilities remain manual checks. The generated app is ad hoc signed for local use; distribution signing and launch-at-login are not configured.
