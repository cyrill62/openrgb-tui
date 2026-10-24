# Contributing to openrgb-tui

Thanks for your interest! Bug reports, fixes and new features are all welcome.

## Reporting issues

Please include:

- your OS and terminal emulator,
- the OpenRGB version and the protocol versions shown on the **Settings** page,
- the device involved, if any. The output of
  `cargo run --example probe -- <host> <port>` is very helpful here.
- steps to reproduce, plus what you expected to happen.

## Getting started

1. Install a stable Rust toolchain from <https://rustup.rs>.
2. Clone the repository and build it:

   ```sh
   cargo build
   ```

3. Start the mock server so you can work without real hardware, then run the app:

   ```sh
   cargo run --example mock_server
   cargo run            # in a second terminal
   ```

## Project layout

| Path                      | Purpose                                                       |
| ------------------------- | ------------------------------------------------------------- |
| `src/protocol.rs`         | OpenRGB SDK packet framing and controller (de)serialization   |
| `src/client.rs`           | Blocking TCP client, protocol version negotiation             |
| `src/worker.rs`           | Background thread that owns the connection; `Command`/`Event` channels |
| `src/lib.rs`              | Library crate shared by the binary and the examples           |
| `src/main.rs`             | Binary entry point and argument handling                      |
| `src/ui/`                 | ratatui interface: app state and input (`mod.rs`), pages, device tabs, color picker, widgets, theme, settings |
| `examples/mock_server.rs` | Fake SDK server with virtual devices                          |
| `examples/probe.rs`       | Read-only diagnostic listing a server's devices               |
| `scripts/`                | Release build and packaging scripts                           |

The UI never does network I/O. It sends `worker::Command`s and reacts to
`worker::Event`s, so the interface stays responsive when the server is slow.

## Before submitting a pull request

Run these checks and make sure they pass without warnings:

```sh
cargo fmt --all
cargo clippy --all-targets
cargo test
```

Also:

- Test your change against the mock server. If it touches device handling, test it
  against real hardware too, when you can.
- When you add protocol features, extend `examples/mock_server.rs` so others can
  exercise them, and add unit tests in `src/protocol.rs`.
- Keep the keyboard help (`help_lines` in `src/ui/mod.rs`), the status bar hints and
  the README key tables in sync.
- Add a line under **Unreleased** in [CHANGELOG.md](CHANGELOG.md).
- Keep commits focused and write messages in the imperative mood ("Add zone resize").

## Code style

- Follow `rustfmt` defaults.
- Prefer the standard library. Add a dependency only when it clearly pays for itself.
- Comment *why* something is done, not what the code does.
- Text uses the terminal's own foreground color. Reserve the colors in `src/ui/theme.rs`
  for accents, so the app stays readable on both light and dark themes.

## Building release archives

`scripts/build.sh` (Linux/macOS) and `scripts/build.ps1` (Windows) build and package
release archives into `dist/`:

```sh
scripts/build.sh                 # both architectures of the host OS
scripts/build.sh linux-x64 windows-x64
scripts/build.sh --no-package all
scripts/build.sh --help
```

```powershell
.\scripts\build.ps1              # windows-x64 + windows-arm64
.\scripts\build.ps1 -NoPackage all
```

Cross-compiling needs extra tools: `cargo-zigbuild` + zig or `cross` for Linux,
`cargo-xwin` for Windows. macOS targets must be built on macOS. The scripts skip any
target they cannot build and tell you what is missing.

## Releasing (maintainers)

1. Bump `version` in `Cargo.toml` and move the **Unreleased** entries in `CHANGELOG.md`
   under the new version.
2. Commit, then tag and push: `git tag v0.2.0 && git push --tags`.
3. The **Release** workflow builds every target and creates a draft GitHub release
   with the archives. Review it and publish.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
