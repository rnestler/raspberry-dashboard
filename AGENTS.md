# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Raspberry Pi dashboard application built with Rust and [Slint](https://slint.dev/) UI framework. Displays switchable widgets on a Raspberry Pi 3B+ connected to a screen. Widgets: Clock, Snapcast now-playing, Home Assistant sensors (plain cards and gauges), and Daily Verse (BibleGateway). Automatically switches to the now-playing widget when a Snapcast stream is playing. Unconfigured optional widgets are excluded from the TAB cycle. An optional auto-cycle timer advances widgets on a configurable interval.

## Build & Run

```bash
cargo build                        # Build the project
cargo run                          # Build and run (locally)
SNAPCAST_HOST=host:1705 cargo run  # Run with custom Snapcast server address
cargo build --release              # Release build (recommended for Pi deployment)
```

Requires Rust edition 2024. No tests configured yet.

```bash
cargo fmt                          # Format code
cargo clippy                       # Lint
```

```bash
cross build --target aarch64-unknown-linux-gnu --no-default-features --features backend-linuxkms-noseat --release  # Cross-compile for Pi
```

The `backend-linuxkms-noseat` feature is used for Pi deployment (renders without a display server). Default feature uses the standard windowed backend for local development.

## Architecture

### Two-thread model

- **Main thread**: Runs the Slint event loop, owns all UI state. Slint timers handle clock updates (1s), screensaver repositioning (5s), and optional auto-cycle widget switching.
- **Background threads**: Each async client (Snapcast, Home Assistant, Daily Verse) runs in its own thread with a tokio runtime. They communicate UI updates to the main thread via `slint::invoke_from_event_loop()`.

### Widget system

`dashboard.slint` is the top-level window that conditionally renders widgets based on an integer `current-widget` property. TAB cycles widgets manually; the Snapcast module auto-switches based on playback state. Unconfigured optional widgets are excluded from the TAB cycle. To add a new widget: add it to the `enabled_widgets` list in `main.rs`, add a new `.slint` component, and add a conditional block in `dashboard.slint`.

### Snapcast integration (`src/snapcast.rs`)

Uses the `snapcast-control` crate (async/tokio) to connect to a Snapcast server via TCP. Key detail: `StreamMetadata` fields are private in the crate, so metadata is extracted via serialize-to-`serde_json::Value`-then-deserialize into a local `NowPlayingInfo` struct. Album art (SVG) is fetched via HTTP from `art_url` and decoded by Slint's built-in SVG support (`Image::load_from_svg_data`). When playback stops, the fallback widget is configurable (defaults to the first enabled widget rather than a hardcoded index).

### Slint ↔ Rust boundary

`slint::include_modules!()` generates Rust types from `.slint` files at compile time. All Dashboard properties and callbacks declared in `dashboard.slint` become setter/getter methods on the generated `Dashboard` struct. `build.rs` compiles `ui/dashboard.slint` (which imports the other `.slint` files).

## Environment Variables

- `SNAPCAST_HOST` — Snapcast server address (default: `127.0.0.1:1705`)

## Deployment

A systemd unit file (`raspberry-dashboard.service`) runs the binary as user `alarm` from `/home/alarm/raspberry-dashboard`.
