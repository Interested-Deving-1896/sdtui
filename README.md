# sdtui 🚀

[![codecov](https://codecov.io/github/abhijeetmohanan/sdtui/graph/badge.svg?token=HUYG0XS5QV)](https://codecov.io/github/abhijeetmohanan/sdtui) [![asciicast](https://asciinema.org/a/DPpzRcXw3hOmiNNK.svg)](https://asciinema.org/a/DPpzRcXw3hOmiNNK)

**sdtui** is a high-performance Terminal User Interface (TUI) written in Rust for managing both user-level and system-wide systemd units. It prioritizes speed, clean aesthetics, and deep log integration.
n## 📺 Demo

![sdtui demo](demo.gif)


## Features

- **Multi-threaded Engine**: Background worker for non-blocking system queries and log fetching.
- **Dual-View Mode**: Instant switching between Local User and Full System views.
- **Live Log Streaming**: Real-time journal viewing with automatic text wrapping.
- **Smart Tailing**: Auto-scrolls to the latest logs, with the ability to pause and scroll manually.
- **Regex Highlighting**: Highlight specific terms in logs using regex patterns.
- **Interactive Action Menu**: Start, stop, enable, disable, and edit units with a single key or a menu.
- **Multi-select Filtering**: Filter unit types (Services, Sockets, Targets, etc.) dynamically.
- **Fuzzy Search**: Find units instantly by name or description.
- **Robust Terminal Handoff**: Safely handles `sudo` prompts and external editors without breaking the TUI.

## Keybindings

| Key | Action |
|-----|--------|
| `Tab` | Switch between User and System modes |
| `s` | Smart Toggle (Start/Stop) |
| `Enter` | Open Selection Menu |
| `/` | Start Search |
| `f` | Toggle Unit Type Filters |
| `l` | Open Full-Screen Log View |
| `?` | Toggle Help Modal |
| `j`/`k` | Move Selection / Scroll Logs |
| `PgUp`/`PgDn` | Fast Scroll / Jump |
| `G` | Snap to Bottom (Resume Tailing) |
| `g` | Snap to Top |
| `q` / `Esc` | Quit / Close Modals |

### Log View Actions
| Key | Action |
|-----|--------|
| `h` | Enter Regex for Highlighting |
| `c` | Clear Highlighting |
| `l` / `Esc` | Close Log View |

## Installation

### From Source
```bash
git clone https://github.com/abhijeetmohanan/sdtui.git
cd sdtui
cargo build --release
```
The binary will be available at `./target/release/sdtui`.

## Requirements

- **OS**: Linux (with `systemd`)
- **Dependencies**: `systemctl`, `journalctl`, `sudo` (for system-wide actions)

## License

This project is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
