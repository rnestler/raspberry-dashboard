# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Raspberry Pi dashboard application built with Rust and [Slint](https://slint.dev/) UI framework. Currently displays a clock that repositions randomly every 5 seconds (screensaver-style). Targets a Raspberry Pi 3B+ connected to a screen.

## Build & Run

```bash
cargo build          # Build the project
cargo run            # Build and run
cargo build --release  # Release build (recommended for Pi deployment)
```

Requires Rust edition 2024. No tests or linter configured yet.

## Architecture

- **`build.rs`** — Build script that compiles Slint UI files via `slint_build::compile()`
- **`ui/dashboard.slint`** — Slint UI definition for the `Dashboard` window component. Properties set from Rust: `current-time`, `time-x`, `time-y`
- **`src/main.rs`** — Application entry point. Creates the Dashboard window and runs two timers:
  - Clock timer (1s): updates displayed time
  - Position timer (5s): randomizes text position within window bounds
- Slint components are imported into Rust via `slint::include_modules!()` macro, which generates Rust types from the `.slint` files at compile time
