# OMP Pet

A macOS desktop pet built in Rust with native AppKit. A small TypeScript oh-my-pi extension sends session events over a private Unix socket. The companion makes no model calls and uses no webview.

## Run

Requires macOS 12+, Xcode Command Line Tools, and Rust. Build and open the accessory app:

```sh
scripts/build-app.sh
open 'dist/OMP Pet.app'
```

Install the local plugin once, then start OMP from any project:

```sh
omp install /Users/soham/Documents/code/omp-pet
omp
```

In an existing OMP session, run `/reload-plugins` after installation or extension edits. For one-off loading, use `omp -e /Users/soham/Documents/code/omp-pet/extension/index.ts`.

The TypeScript bridge uses only Node built-ins at runtime; the OMP import is a type import. Bun is needed only for development checks. `/pet show` opens the app automatically and waits for its socket; `OMP_PET_APP` can override the app bundle path. The app must be running to show updates; the extension reconnects on later events or its five-second heartbeat when the app restarts.

Hover for task, active tools, context occupancy, and session state. Animated dots mean the agent lifecycle is active; they do not prove token generation or tool progress. The 4-point context bar uses the sprite pack's most common chromatic color, ignoring transparency and near-gray pixels. Context remains unknown when OMP cannot supply it.

Drag the pet near any screen edge to dock it. Tucking happens only when selected with `/pet tuck` or the interaction menu. The tucked state is a 3-point line; hold the cursor at that screen segment (including nearby corners) for 200 ms to reveal it. The line sits on the physical display edge; reveal detection includes Dock and menu-bar insets. A lightweight cursor watcher runs only while tucked to catch missed edge mouse events. There is no automatic tucking. Placement and sprite choice persist across restarts. The pet and task card are nonactivating fixed-size panels with native tiling exclusion.

Use the menu-bar **π** or right-click the pet for readout, session switching, and tuck/reveal. Drag the bottom-right grip or Option-drag anywhere on the pet to resize it. Size is saved across restarts, and the readout scales with the widget. The readout has tight margins and an opaque palette-matched background.

**Pin readout** keeps the hover card open while the pet is visible; its three fields are project, current step/status, and used tokens plus percent. The card uses the pet palette. Configuration and lifecycle controls are OMP slash commands:

```text
/pet sprites                 # native folder picker
/pet sprites /path/to/pack   # select a pack directly
/pet reload                  # reload edited sprites
/pet cat                     # restore the default cat
/pet reset                   # reset desktop placement
/pet quit                    # quit the companion
/pet show                    # launch the app if needed and reveal the pet
/pet size 160                # resize the complete widget (64–256)
/pet status                  # show native status
```

## Custom sprites

Choose a folder containing `manifest.json` and PNGs with `/pet sprites`. Version 1 requires a nonempty `idle` animation. Optional animations: `working`, `waiting`, `compacting`, `error`, `disconnected`, and `celebrate` (plays once after successful work). Missing states fall back to idle. Static sprites work as a single-frame animation.

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

Crop coordinates start at the PNG's top-left corner. Durations must be 80–2000 ms. Paths stay within the pack; limits are 128 frames, 16 MiB encoded PNGs, and four million decoded pixels. Transparent padding is excluded and every frame fits the same full pet viewport (112×112 by default), preserving its proportions. Animation timers stop while tucked. `/pet reload` updates edited files and recalculates the accent.

The default is the native cat. No Pokémon artwork is bundled or tracked in the current source tree. The downloaded Zorua pack remains locally under `work/sprite-packs/zorua`, with source attribution and upstream credits, and can be selected with `/pet sprites /Users/soham/Documents/code/omp-pet/work/sprite-packs/zorua`. Sprite rights are separate from the app's code license.

## Development

```sh
cargo test
bun install --ignore-scripts
bun test
bun run typecheck
python3 scripts/check-sprites.py /path/to/pack
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

Tests cover lifecycle/approval/error behavior through a real Unix socket, context handling, UTF-8 bounds, sequence resets on reconnect, stale socket events, independent sessions, screen geometry, palette filtering, and framing limits. `python3 scripts/verify-omp.py` verifies plugin discovery and `/pet status` in the installed OMP runtime using an isolated profile and a local placeholder endpoint. It invokes no agent. Model-session use and third-party tiling utilities remain manual checks. The generated app is ad hoc signed for local use; distribution signing and launch-at-login are not configured.
