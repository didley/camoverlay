<h1 align="center">
  <img src="data/icons/hicolor/scalable/apps/io.github.didley.CamOverlay.svg" alt="Cam Overlay Icon" width="128" height="128"/>
  <br>
  GNOME Cam Overlay
</h1>

A minimal GNOME app that displays a webcam preview as a borderless overlay, for use during screen recording.

No recording — just a live preview with zoom, shape, and flip controls via right-click menu.

<table align="center">
  <tr>
    <td><img src="data/screenshots/screenshot1.png" alt="Screenshot 1" width="400"/></td>
    <td><img src="data/screenshots/screenshot2.png" alt="Screenshot 2" width="400"/></td>
  </tr>
  <tr>
    <td><img src="data/screenshots/screenshot3.png" alt="Screenshot 3" width="400"/></td>
    <td><img src="data/screenshots/screenshot4.png" alt="Screenshot 4" width="400"/></td>
  </tr>
</table>

## Features

- Live webcam preview (PipeWire)
- Multiple camera support with hot-plug detection
- Circle or rounded rectangle shape clipping
- Crop or stretch video fit modes
- 1×, 1.5×, and 2× zoom
- Horizontal mirror/flip
- Double-click to toggle fullscreen, Escape key to exit
- Drag to move, drag edges/corners to resize
- All settings persist across restarts

## Tips

- Use your compositor's window manager (e.g. `Super+Right Click` on GNOME) to set **Always on Top**
- Right-click the overlay to access all controls

## Build

Requires [`just`](https://github.com/casey/just). See [`justfile`](justfile).

## Requirements

- GNOME Platform 50
- GStreamer with PipeWire support
- Rust stable toolchain

## Alternatives

**MacOS:** This is quite close to the feature set of [Quick Camera](https://apps.apple.com/app/quick-camera/id598853070)

## License

GPL-3.0-or-later
