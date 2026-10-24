# openrgb-tui

A terminal user interface for [OpenRGB](https://openrgb.org). It talks to a running
OpenRGB SDK server over the network, so you can control your RGB lighting from any
terminal, including over SSH.

## Features

- **Devices**: browse every controller the server reports, with a live LED strip preview.
- **Lighting**: switch modes and tune speed, brightness, direction and mode colors.
  Optionally save a mode to the device's onboard memory.
- **LEDs**: select single LEDs or keys with the keyboard or mouse and paint them.
  You can fill a whole zone or device, or pick a color from an existing LED.
- **Zones**: fill a zone with one color, or resize zones that support it.
- **Sync all devices**: apply one color to every device at once, or turn them all off.
- **Profiles**: save, load and delete OpenRGB profiles.
- **Color picker**: HSV sliders plus hex input, and copy to the clipboard through OSC 52.
- **Mouse support**: click buttons, drag over LEDs and along sliders.
- Settings persist between runs. Text uses your terminal's colors, so the app reads
  well on light and dark themes.

## Requirements

- OpenRGB with its SDK server running. Start OpenRGB with `--server`, or enable the
  server in OpenRGB's **SDK Server** tab. The default port is `6742`.
- A terminal with true-color support gives the best results.

The client negotiates SDK protocol versions up to v4 and also works with older servers.

## Installation

### Prebuilt binaries

Download an archive for your platform from the
[Releases](../../releases) page. Builds are available for:

| OS      | Architectures                     |
| ------- | --------------------------------- |
| Linux   | x86_64, arm64 (glibc and musl)    |
| macOS   | x86_64, arm64                     |
| Windows | x64, ARM64                        |

Extract the archive and put `openrgb-tui` (or `openrgb-tui.exe`) on your `PATH`.

### From source

You need a recent stable [Rust](https://rustup.rs) toolchain.

```sh
cargo install --path .
# or
cargo build --release   # binary in target/release/openrgb-tui
```

## Usage

```sh
openrgb-tui                  # connect to the saved address (default 127.0.0.1:6742)
openrgb-tui 192.168.1.20     # connect to another host on the default port
openrgb-tui myhost:6743      # host and port
openrgb-tui --help
```

Press `?` inside the app for the full list of keys.

### Keys

| Key                      | Action                                           |
| ------------------------ | ------------------------------------------------ |
| `Tab`                    | Switch between the sidebar and the page          |
| `↑` `↓` / `j` `k`        | Move                                             |
| `Enter` / `Space`        | Open, select or press                            |
| `←` `→` / `h` `l`        | Adjust a slider or choice (`Shift`: faster)      |
| `Esc`                    | Back to the sidebar                              |
| `r`                      | Rescan devices                                   |
| `q` / `Ctrl-C`           | Quit                                             |

**Device pages**

| Key              | Action                                       |
| ---------------- | -------------------------------------------- |
| `[` `]` or `1`–`4` | Switch tab: Lighting, LEDs, Zones, Info    |
| `+` / `-`        | Add or remove a mode color                   |

**LEDs tab**

| Key           | Action                                          |
| ------------- | ----------------------------------------------- |
| arrows        | Move the cursor (`Shift`: extend the selection) |
| `Space`       | Select or unselect the LED under the cursor     |
| `Enter`       | Paint the selection (or the cursor LED)         |
| `c`           | Change the paint color                          |
| `p`           | Pick the paint color from the cursor LED        |
| `a` / `n` / `z` | Select all, none, or the cursor's zone        |
| `f` / `o`     | Fill the device / turn the selection off        |

**Profiles**: `Enter` loads a profile, `d` or `Del` deletes it.

### Settings file

The host, port, "connect on startup" option and paint color are saved on exit to:

| OS      | Path                                                         |
| ------- | ------------------------------------------------------------ |
| Linux   | `$XDG_CONFIG_HOME/openrgb-tui/settings.conf` or `~/.config/openrgb-tui/settings.conf` |
| macOS   | `~/Library/Application Support/openrgb-tui/settings.conf`    |
| Windows | `%APPDATA%\openrgb-tui\settings.conf`                        |

`XDG_CONFIG_HOME` takes precedence on every platform when it is set.

## Development without hardware

The repository includes a mock SDK server with a few virtual devices:

```sh
cargo run --example mock_server            # listens on 127.0.0.1:6742
cargo run --example mock_server -- 6743    # custom port
cargo run                                  # in another terminal
```

To list what a real server reports, without changing anything:

```sh
cargo run --example probe -- 127.0.0.1 6742
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build releases and contribute.

## License

[MIT](LICENSE). OpenRGB is a separate project. openrgb-tui is not affiliated with it.
