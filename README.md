# Raspberry Dashboard

A dashboard for my Raspberry Pi 3B+ connected to a screen.

Widgets:
- Clock (screensaver-style, repositions every 5s)
- Snapcast now-playing (auto-switches when a stream is playing)
- Home Assistant sensors (displays sensor readings from a Home Assistant instance)

TAB to switch widgets, q to quit.

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

- `SNAPCAST_HOST` — Snapcast server address (default: `127.0.0.1:1705`)
- `DASHBOARD_CONFIG` — Path to config file (default: `config.toml`)

### Config file

Optional TOML config file for Home Assistant integration. See [config.toml.example](config.toml.example).

## Deployment

Copy the binary to `/home/alarm/raspberry-dashboard` and install the systemd unit:
```bash
sudo cp raspberry-dashboard.service /etc/systemd/system/
sudo systemctl enable --now raspberry-dashboard
```

Set `SNAPCAST_HOST` in the `Environment=` line of the service file.

## Screenshots

![Clock widget](./images/clock-widget.png)

![Snapcast widget](./images/snapcast-widget.png)

![Home Assistant widget](./images/homeassistant-widget.png)
