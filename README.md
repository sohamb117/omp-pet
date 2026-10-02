# OMP Pet

A native macOS desktop companion for oh-my-pi. Rust + AppKit, with a small TypeScript extension sending local session updates.

The first implementation covers a persistent draggable pet, hover task readout, context occupancy, activity indicators, and edge tucking. No webview or additional model calls.

## Development

Requires macOS and the Xcode Command Line Tools, plus Rust. Bun is used to verify the OMP extension.

```sh
cargo run
```

Work is committed in small increments. The companion owns one UI process; OMP sessions connect over a local Unix socket.
