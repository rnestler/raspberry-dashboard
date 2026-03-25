# Raspberry Dashboard

A dashboard for my Raspberry Pi 3B+ connected to a screen.

Widgets:
- Clock (screensaver-style, repositions every 5s)
- Snapcast now-playing (auto-switches when a stream is playing)

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

## Deployment

Copy the binary to `/home/alarm/raspberry-dashboard` and install the systemd unit:
```bash
sudo cp raspberry-dashboard.service /etc/systemd/system/
sudo systemctl enable --now raspberry-dashboard
```

Set `SNAPCAST_HOST` in the `Environment=` line of the service file.
