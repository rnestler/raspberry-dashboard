# Raspberry Dashboard

A dashboard for my Raspberry Pi 3B+ connected to a screen.

Widgets:
- Clock (screensaver-style, repositions every 5s)
- Snapcast now-playing (auto-switches when a stream is playing)
- Home Assistant sensors (displays sensor readings with plain cards and gauges)
- Daily Verse (Bible verse of the day from BibleGateway, fetched once per day)
- Quotes (random user-configured quotes, picked fresh each time the widget is shown)
- Weather (current conditions and forecast from a Home Assistant weather entity)

TAB cycles through enabled widgets, q quits. Widgets that require configuration (Snapcast, Home Assistant, Daily Verse, Quotes, Weather) are excluded from the cycle when not configured. An optional auto-cycle timer advances to the next widget every N seconds.

## Build

```bash
cargo build          # Local dev build
cargo run            # Run locally
cargo fmt            # Format
cargo clippy         # Lint
```

Cross-compile for Pi:
```bash
cross build --target aarch64-unknown-linux-gnu --no-default-features --features backend-linuxkms-noseat --release
```

## Configuration

- `DASHBOARD_CONFIG` — Path to config file (default: `config.toml`)
- `HOMEASSISTANT_TOKEN` — Home Assistant long-lived access token (required for the Home Assistant and Weather widgets)

### Config file

Optional TOML config file. See [config.toml.example](config.toml.example) for all options.

Key sections:
- `widget_cycle_secs` — auto-cycle interval in seconds (optional; TAB resets the timer)
- `[snapcast]` — Snapcast server `host` address (e.g. `"127.0.0.1:1705"`); enables the now-playing widget
- `[homeassistant]` — Home Assistant URL, poll interval, and sensor list (supports plain cards and gauges); requires `HOMEASSISTANT_TOKEN`
- `[daily_verse]` — enables the Daily Verse widget; optionally set `version` for a BibleGateway translation (default: `NGU-DE`)
- `[[quotes.items]]` — list of quotes for the Quotes widget; each entry has a `text` and an optional `source`
- `[weather]` — Home Assistant weather entity URL, entity ID, poll interval, forecast days (default: 5), and forecast type (`"daily"`, `"hourly"`, or `"twice_daily"`); requires `HOMEASSISTANT_TOKEN`

## Deployment

Copy the binary to `/home/alarm/raspberry-dashboard` and install the systemd unit:
```bash
sudo cp raspberry-dashboard.service /etc/systemd/system/
sudo systemctl enable --now raspberry-dashboard
```

Configure the Snapcast host and other settings in the config file pointed to by `DASHBOARD_CONFIG`.

## Screenshots

![Clock widget](./images/clock-widget.png)

![Snapcast widget](./images/snapcast-widget.png)

![Home Assistant widget](./images/homeassistant-widget.png)

![Weather widget](./images/weather-widget.png)
