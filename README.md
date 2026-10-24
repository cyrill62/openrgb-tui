# OpenRGB TUI

A fast terminal UI for [OpenRGB](https://openrgb.org), written in Rust with
[ratatui](https://ratatui.rs).

It doesn't replace OpenRGB. It's a client that connects to a running OpenRGB
instance through its SDK server and controls the devices OpenRGB already
detected. It works over SSH, too.

## Features

- **Overview**: every device at a glance with live LED strips, plus a
  "sync all" color and an "all off" button.
- **Lighting**: switch hardware modes and set speed, brightness, direction and
  mode colors, with "save to device" for controllers that support it.
- **LEDs**: a visual LED map (keyboards render as labelled keys) where you move
  a cursor, select LEDs (or drag over them with the mouse) and paint them live.
- **Zones**: fill zones and resize addressable ARGB headers.
- **Profiles**: load, save and delete profiles stored by the OpenRGB server.
- Reacts to device hot-plug notifications from the server, and remembers the
  server address and paint color between runs.

Needs a terminal with 24-bit color (most modern terminals).

## Usage

1. Start OpenRGB's SDK server: run `openrgb --server`, or click
   *Start Server* in OpenRGB's **SDK Server** tab.
2. Run this app:

   ```sh
   cargo run --release                       # connects to 127.0.0.1:6742
   cargo run --release -- 192.168.1.20:6742  # or another host[:port]
   ```

Supports SDK protocol versions 0–4. Newer servers negotiate down to v4.

### Keys

| Key                     | Action                                          |
|-------------------------|-------------------------------------------------|
| `Tab`                   | switch between the sidebar and the page         |
| `↑` `↓` / `j` `k`       | move                                            |
| `←` `→` / `h` `l`       | adjust a slider, color or choice (`Shift`: faster) |
| `Enter` / `Space`       | open, select or press                           |
| `[` `]` or `1`–`4`      | device tabs: Lighting, LEDs, Zones, Info        |
| `Esc`                   | back to the sidebar                             |
| `r`                     | rescan devices                                  |
| `?`                     | all keys                                        |
| `q` / `Ctrl-C`          | quit                                            |

On the LEDs tab: arrows move the cursor (`Shift` extends the selection),
`Space` toggles an LED, `Enter` paints the selection, `c` changes the paint
color, `p` picks it from the cursor LED, `a` / `n` / `z` select all / none /
the current zone, `f` fills the device and `o` turns the selection off.

Mouse clicks work everywhere: click and drag along sliders and color rails, or
over LEDs to select them (`Ctrl`/`Shift`-click toggles).

Settings are stored in `$XDG_CONFIG_HOME/openrgb-tui/settings.conf`
(`~/.config` on Linux, `~/Library/Application Support` on macOS, `%APPDATA%` on
Windows).

## Building releases

```sh
scripts/build.sh                # Linux/macOS: both architectures of the host OS
scripts/build.sh linux-x64      # or pick targets: linux-arm64, macos, windows-x64, all, …
```

```powershell
.\scripts\build.ps1              # Windows: x64 + ARM64
```

Archives are written to `dist/`. Run `scripts/build.sh --help` to see the
cross-compilation options. Pushing a `v*` tag runs
`.github/workflows/release.yml`, which builds Linux (glibc and musl), macOS and
Windows for x64 and ARM64, then attaches the archives to a draft GitHub release.

## Development

```sh
cargo test
cargo run --example mock_server -- 26742        # fake server with virtual devices
cargo run -- 127.0.0.1:26742
cargo run --example probe -- 127.0.0.1 6742     # read-only: list a server's devices
```

Layout:

| Path              | Purpose                                                  |
|-------------------|----------------------------------------------------------|
| `src/protocol.rs` | SDK packet framing and controller (de)serialization      |
| `src/client.rs`   | Blocking TCP client                                      |
| `src/worker.rs`   | Background thread that owns the connection, so the UI never blocks |
| `src/ui/`         | ratatui app: event loop, pages, LED map, color picker    |
