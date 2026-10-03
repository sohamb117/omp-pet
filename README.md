# OMP Pet

A macOS desktop pet built in Rust with native AppKit. A small TypeScript oh-my-pi extension sends session events over a private Unix socket. The companion makes no model calls and uses no webview.

## Install and run

On an Apple Silicon Mac with OMP installed:

```sh
omp plugin install 'github:sohamb117/omp-pet#v0.1.3'
```

Start OMP (or run `/reload-plugins` in an existing session), then:

```text
/pet show
```

The plugin automatically downloads its matching app release, checks its SHA-256 checksum, bundle identity/version, and code signature, and installs it under `~/Library/Application Support/OMP Pet/apps/<version>/`. No Rust toolchain, manual app download, admin rights, or environment variable is needed. Later launches reuse the cached app, including offline. `/pet install` performs installation without launching. Downloads only happen through these explicit commands; agent lifecycle hooks never download software.

An already running companion is reused. `OMP_PET_APP` overrides the managed app path; a local checkout's `dist/OMP Pet.app` is also preferred for development. Unsupported Intel/Rosetta installations receive a clear error and can use a manually built app via `OMP_PET_APP`. Failed downloads can be retried with `/pet show`; partial installs are cleaned up and prior versions are preserved. The app is ad-hoc signed, not Apple notarized.

The TypeScript bridge uses only Node built-ins at runtime; the OMP import is a type import. Bun is needed only for development checks. The extension reconnects on later events or its five-second heartbeat when the app restarts.

Hover for task, active tools, context occupancy, and session state. Animated dots mean the agent lifecycle is active; they do not prove token generation or tool progress. The 4-point context bar uses the sprite pack's most common chromatic color, ignoring transparency and near-gray pixels. Context remains unknown when OMP cannot supply it.

Drag the pet near any screen edge to dock it. Tucking happens only when selected with `/pet tuck` or the interaction menu. The tucked state is a 3-point line; hold the cursor at that screen segment (including nearby corners) for 200 ms to reveal it. The line sits on the physical display edge; reveal detection includes Dock and menu-bar insets. Hover reveal is temporary: leaving the pet, readout, and edge for 350 ms tucks it again. A lightweight cursor watcher runs while tucked or temporarily revealed to catch missed edge mouse events. Freely placed pets never tuck automatically. Explicit reveal or `/pet show` keeps the pet visible. Placement and sprite choice persist across restarts. The pet and task card are nonactivating fixed-size panels with native tiling exclusion.

Use the menu-bar **π** or right-click the pet for readout, session switching, and tuck/reveal. The bottom-right resize grip appears only while hovering over the pet. Drag the grip or Option-drag anywhere on the pet to resize it. Size is saved across restarts, and the readout scales with the widget. The readout sits above the pet, stays within the screen, and has tight margins with a palette-matched background at 70% opacity.

**Pin readout** keeps the hover card open while the pet is visible; its three fields are project, current step/status, and used tokens plus percent. The card uses the pet palette. Configuration and lifecycle controls are OMP slash commands:

```text
/pet sprites                 # native folder picker
/pet sprites /path/to/pack   # select a pack directly
/pet reload                  # reload edited sprites
/pet cat                     # restore the default cat
/pet reset                   # reset desktop placement
/pet quit                    # quit the companion
/pet show                    # install/launch the app if needed and reveal the pet
/pet install                 # install the app without launching it
/pet size 160                # resize the complete widget (64–256)
/pet status                  # show native status
```

The pet sleeps immediately when no OMP session is connected, including at app startup. While connected, it sleeps after one continuous minute without work or anything needing attention. Reconnecting starts a fresh idle minute. Idle heartbeats do not reset the countdown; working, compacting, approval requests, and errors in any connected session keep it awake. Work wakes it immediately. The countdown uses one one-shot timer, not polling. Sleep frames are capped at one per second and never faster than the pack's slowest idle frame; packs with static idle images use a still sleep pose. The native cat closes its eyes without an animation timer, and tucked pets never animate. `/pet status` includes `sleeping` and `sleep_timer_pending` for inspection.

## Adopt from Morisoba

OMP Pet 0.1.2 registers the `omppet://` URL scheme. After `/pet show` has opened the app once, an **Adopt in OMP Pet** link on Morisoba can download and activate a sprite pack directly. For upgrades, quit an older running companion before `/pet show`.

Accepted sources are HTTPS endpoints on `morisoba.moe`, `www.morisoba.moe`, and `pets.morisoba.moe`, and `ompoke.morisoba.moe` (0.1.3+), plus explicit loopback HTTP ports for development. The URL identifies a catalog entry and direction and includes a SHA-256 digest. Downloading and archive extraction run off the main thread, with time and size limits. The importer accepts only flat PNG/manifest/credit files, rejects traversal, duplicate paths and symlinks, verifies the digest, and uses the normal sprite validator before switching. Prior sprite preferences survive failures. Downloaded packs are stored under `~/Library/Application Support/OMP Pet/packs/<sha256>/`; no Pokémon art is added to the app bundle.

`omp-pet --check-pack /path/to/folder` validates a generated pack using the native image loader without launching the desktop pet. The site lives in a separate project; the app does not depend on it for normal operation or local sprite loading.

## Custom sprites

Choose a folder containing `manifest.json` and PNGs with `/pet sprites`. Version 1 requires a nonempty `idle` animation. Optional animations: `working`, `waiting`, `compacting`, `error`, `disconnected`, `sleep`, and `celebrate` (plays once after successful work). Missing states fall back to idle; missing sleep frames use a still idle pose. Static sprites work as a single-frame animation.

```json
{
  "version": 1,
  "frame_ms": 200,
  "pixel_art": true,
  "idle": ["idle-1.png", "idle-2.png"],
  "sleep": ["sleep-1.png", "sleep-2.png"],
  "working": [
    {"file": "walk.png", "x": 0, "y": 0, "width": 32, "height": 40, "duration_ms": 100},
    {"file": "walk.png", "x": 32, "y": 0, "width": 32, "height": 40, "duration_ms": 100}
  ]
}
```

Crop coordinates start at the PNG's top-left corner. Durations must be 80–2000 ms. Paths stay within the pack; limits are 128 frames, 16 MiB encoded PNGs, and four million decoded pixels. Transparent padding is excluded and every frame fits the same full pet viewport (112×112 by default), preserving its proportions. Animation timers stop while tucked. `/pet reload` updates edited files and recalculates the accent.

The default is the native cat. No Pokémon artwork is bundled or tracked in the current source tree. The downloaded Zorua pack remains locally under `work/sprite-packs/zorua`, with source attribution and upstream credits, and can be selected with `/pet sprites /Users/soham/Documents/code/omp-pet/work/sprite-packs/zorua`. Sprite rights are separate from the app's code license.

## Development

Building from source requires Xcode Command Line Tools and Rust on macOS:

```sh
scripts/build-app.sh
omp plugin install "$PWD"
```


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

## CI and releases

GitHub Actions runs on pushes to `main`, pull requests, and manual dispatches. It checks Rust formatting, Clippy, Rust tests, TypeScript types, and plugin tests, then builds and verifies the Apple Silicon app. Every successful run provides an `OMP-Pet-macos-arm64` ZIP artifact plus SHA-256 checksum, retained for 14 days. The app defaults to the native cat; no custom sprite art is packaged.

To prepare a release, update both Cargo.toml and package.json to the same version and push a matching `vX.Y.Z` tag. After all checks pass, the same workflow creates a **draft** GitHub release with the app ZIP and checksum. Review and publish the draft in GitHub. These builds are ad-hoc signed, not Apple notarized; distribution with Developer ID signing/notarization requires an Apple Developer certificate and credentials, which are not configured.

## Resource benchmark

With the release app running, execute `python3 scripts/benchmark.py --seconds 20`. It measures visible idle animation, working animation, visible readout, and tucked operation without invoking a model, and writes `work/benchmark.json`. Avoid interacting with the pet during sampling. It temporarily displays a synthetic task and restores visibility afterward. Restarting clears the completed benchmark session.

CPU is delta process user + system time divided by elapsed wall time (100% = one CPU core). Memory is sampled resident set size, including shared pages; it is not private footprint or total macOS compositor usage. The 1 Hz status polling and synthetic task updates are included. The current sprite pack and widget size are recorded, so compare like-for-like. Counters are queried only by `/pet status`; there is no background telemetry collection.

Measured example: [macOS ARM64, Zorua, October 2 2026](benchmarks/macos-arm64-2026-10-02.json): approximately 1.0% of one CPU core idle, 2.2% working, 1.1% tucked, and 74–75 MiB RSS at the recorded widget size. These are single 15-second samples of the native companion, including measurement overhead; they exclude OMP, GPU, and WindowServer.

A separate quiet idle check after restart (no synthetic updates or polling between endpoints) measured 1.9% CPU visible and 0.17% tucked, with about 43 MiB RSS. This shows the effect of workload and measurement conditions; do not treat either run as an exact battery-life prediction.
