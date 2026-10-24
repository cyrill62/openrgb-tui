# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-24

First release.

### Added

- Terminal UI for the OpenRGB SDK server, built with ratatui.
- SDK client that negotiates protocol versions up to v4 and supports servers that
  predate protocol versioning. It runs on a background worker thread so the UI never blocks.
- Connection page with host, port and connect-on-startup options. An optional
  `host[:port]` command-line argument.
- Device pages with Lighting, LEDs, Zones and Info tabs:
  - mode selection with speed, brightness, direction and mode colors, and saving
    modes to device memory;
  - per-LED painting with selection, a color picker and fill/off actions;
  - zone fill and zone resize.
- "Sync all devices" to apply one color to every device, or turn them all off.
- Profile save, load and delete.
- HSV color picker with hex input and clipboard copy (OSC 52).
- Mouse support, including dragging over LEDs and sliders.
- Settings saved between runs.
- `mock_server` and `probe` examples for development and diagnostics.
- Release build scripts for Linux (glibc and musl), macOS and Windows (x86_64 and
  arm64), and a GitHub Actions release workflow.

[Unreleased]: ../../compare/v0.1.0...HEAD
[0.1.0]: ../../releases/tag/v0.1.0
