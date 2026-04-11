# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Raspberry Pi dashboard application built with Rust and [Slint](https://slint.dev/) UI framework. Displays switchable widgets on a Raspberry Pi 3B+ connected to a screen. Widgets: Clock, Snapcast now-playing, Home Assistant sensors (plain cards and gauges), Daily Verse (BibleGateway), Quotes (user-configured), and Weather (Home Assistant weather entity). Automatically switches to the now-playing widget when a Snapcast stream is playing. Unconfigured optional widgets are excluded from the TAB cycle. An optional auto-cycle timer advances widgets on a configurable interval.

## Build & Run

```bash
cargo build                        # Build the project
cargo run                          # Build and run (locally)
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
- **Background threads**: Each async client (Snapcast, Home Assistant, Daily Verse, Weather) runs in its own thread with a tokio runtime. They communicate UI updates to the main thread via `slint::invoke_from_event_loop()`. The Quotes and Clock widgets have no background threads and run entirely on the main thread.

### Widget trait and factory (`src/widget.rs`)

All widgets implement the `Widget` trait (`index`, `init`, `on_activate`, `is_active`). The `create_widgets(config)` factory inspects the config and returns a `WidgetController` that owns only the enabled widgets. `main.rs` operates on widgets generically via the controller — no widget-specific code.

The `WidgetController` centralises all widget-switching logic: `advance(dashboard, active_only)` cycles widgets (TAB uses `active_only=false`, auto-cycle uses `true`), and `deactivate_current(dashboard)` switches away from an inactive widget. The Slint `deactivate-widget` callback delegates to the controller.

To add a new widget: create a module with a struct implementing `Widget`, register it in `create_widgets`, add a new `.slint` component, and add a conditional block in `dashboard.slint`.

- `init(&mut self, &Dashboard)` — called once at startup for main-thread setup (timers, initial properties) and/or spawning background threads. Widgets that need a background thread call `dashboard.as_weak()` and spawn from here.
- `on_activate(&self, &Dashboard)` — called each time the widget becomes visible. Used by Quotes to pick a new random quote; no-op for others.
- `is_active(&self) -> bool` — whether the widget should be included in auto-cycle rotation. Default: `true`. Snapcast returns `false` when nothing is playing, causing the auto-cycle timer to skip it. Manual TAB switching still reaches inactive widgets.

Widget indices: 0 = HomeAssistant (optional), 1 = NowPlaying (Snapcast, optional), 2 = Clock (always), 3 = DailyVerse (optional), 4 = Quotes (optional), 5 = Weather (optional).

The 1-second `current_time` timer is a dashboard-level concern in `main.rs` (all widgets display the time via the shared Slint property).

### Snapcast integration (`src/snapcast.rs`)

Uses the `snapcast-control` crate (async/tokio) to connect to a Snapcast server via TCP. Key detail: `StreamMetadata` fields are private in the crate, so metadata is extracted via serialize-to-`serde_json::Value`-then-deserialize into a local `NowPlayingInfo` struct. Album art (SVG) is fetched via HTTP from `art_url` and decoded by Slint's built-in SVG support (`Image::load_from_svg_data`). When playback starts, Snapcast marks itself active and auto-switches the dashboard to its widget. When playback stops, it marks itself inactive and invokes the `deactivate-widget` Slint callback; `main.rs` then advances to the next active widget.

### Slint ↔ Rust boundary

`slint::include_modules!()` generates Rust types from `.slint` files at compile time. All Dashboard properties and callbacks declared in `dashboard.slint` become setter/getter methods on the generated `Dashboard` struct. `build.rs` compiles `ui/dashboard.slint` (which imports the other `.slint` files).

### Home Assistant authentication

The HA long-lived access token is read from the `HOMEASSISTANT_TOKEN` environment variable (not from config files). Both the HomeAssistant sensor widget and the Weather widget share this token. If the env var is missing, any configured HA-dependent widget is skipped with a warning.

### Weather widget (`src/weather.rs`)

Uses a Home Assistant weather entity. Fetches current conditions via `GET /api/states/<entity_id>` and forecasts via `POST /api/services/weather/get_forecasts`. HA condition strings are mapped to Unicode weather symbols. Forecast length is configurable (`forecast_days`, default 5). Forecast type can be `"daily"`, `"hourly"`, or `"twice_daily"` (default `"daily"`).

## Deployment

A systemd unit file (`raspberry-dashboard.service`) runs the binary as user `alarm` from `/home/alarm/raspberry-dashboard`.
