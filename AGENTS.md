# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project

`openrgb-tui` is a Rust terminal UI (ratatui + crossterm) for controlling RGB
lighting through an OpenRGB SDK server over TCP. The crate has a library part
(`src/lib.rs`: `protocol`, `client`, `worker`) and a binary (`src/main.rs` + `src/ui/`).

## Commands

```sh
cargo build                                  # debug build
cargo run                                    # run the TUI (needs a server, see below)
cargo run -- host:port                       # connect to a specific server
cargo run --example mock_server [-- PORT]    # fake server with virtual devices, default 6742
cargo run --example probe -- HOST PORT       # read-only device listing
cargo test                                   # unit tests (src/protocol.rs, src/worker.rs)
cargo fmt --all && cargo clippy --all-targets
scripts/build.sh [targets]                   # release build + archives in dist/
```

The TUI takes over the terminal and needs an interactive TTY, so you can't drive it from a
non-interactive shell. To check protocol or client changes, run `mock_server` in the
background and use the `probe` example, or write unit tests.

## Architecture

- `src/protocol.rs`: SDK packet header (`ORGB` magic, 16-byte header), packet IDs
  (`pkt`), mode flags, and (de)serialization of `Controller`, `Mode` and `Zone`.
  Parsing depends on the negotiated protocol version. Keep it version-aware.
- `src/client.rs`: blocking `TcpStream` client. `connect` negotiates
  `proto = min(server, CLIENT_PROTOCOL_VERSION)` and tolerates servers that never
  answer the version request. `recv` skips and records `DEVICE_LIST_UPDATED` notifications.
- `src/worker.rs`: owns the `Client` on a background thread. The UI sends `Command`s
  and receives `Event`s over `std::sync::mpsc` channels. **Never do network I/O from
  the UI thread.**
- `src/ui/mod.rs`: `App` state, the event loop, key and mouse handling, popups, help text
  (`help_lines`) and status bar hints.
- `src/ui/pages.rs`: connection, devices overview, profiles and settings pages.
- `src/ui/device.rs`: device tabs (Lighting, LEDs, Zones, Info).
- `src/ui/color.rs`: the `Hsv` model and color picker rows. `src/ui/widgets.rs`: the
  `Col` line-layout helper and buttons. `src/ui/theme.rs`: colors and styles.
- `src/ui/settings.rs`: `key = value` settings file under the platform config directory.

## Conventions

- Format with rustfmt. Code should be clippy-clean.
- Keep dependencies minimal. Only ratatui (with its crossterm re-export) is used at
  runtime. Base64 for OSC 52, for example, is hand-rolled.
- Short module-level `//!` docs. Comments explain *why*, not what.
- Text uses the terminal's default foreground. Use `theme::` styles for accents only.
- Mouse hit-testing goes through `Hit`/`Field`. When adding an interactive element,
  register it the same way the existing widgets do, so it is clickable.
- When you change keybindings, update `help_lines`, the status bar hints in
  `src/ui/mod.rs` and the key tables in `README.md`.
- When you add protocol support, update `examples/mock_server.rs` and add tests in
  `src/protocol.rs`.
- Record user-visible changes under **Unreleased** in `CHANGELOG.md`.

## Releases

Pushing a `v*` tag triggers `.github/workflows/release.yml`. It builds every target
with `scripts/build.sh` / `scripts/build.ps1` and creates a draft GitHub release. The
scripts read the version from `Cargo.toml` and build with `--locked`, so keep
`Cargo.lock` committed and up to date. `target/` and `dist/` are git-ignored.
