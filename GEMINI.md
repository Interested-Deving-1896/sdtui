# sdtui

## Project Overview
**sdtui** is a high-performance Terminal User Interface (TUI) written in Rust for managing both user-level and system-wide systemd units. It prioritizes speed, clean aesthetics, and deep log integration, providing a modern alternative to standard `systemctl` commands.

### Key Technologies
- **Language:** Rust
- **TUI Framework:** [ratatui](https://github.com/ratatui-org/ratatui)
- **Terminal Backend:** `crossterm`
- **Concurrency:** Multi-threaded architecture with `mpsc` channels for non-blocking UI.
- **Data Source:** Native `systemctl` and `journalctl` integration.

## Architecture & Logic
- **Background Worker:** A dedicated thread handles all disk I/O and system calls (scanning `~/.config/systemd/user/`, querying unit statuses, and fetching journal logs).
- **Dual-Scan Discovery:** Combines `list-unit-files` (disk) and `list-units` (memory) to ensure even stopped/unloaded services are visible.
- **Smart Tailing Logs:** Fetches the last 500 lines of logs with an auto-scroll "tail" mode that pauses during manual navigation.
- **Terminal Handoff:** Suspends TUI mode for interactive actions (like `sudo` prompts or launching `systemctl edit`) to ensure terminal stability.

## Features & Keybindings
- **[Tab] View Switch:** Toggle between Local User and Full System views.
- **[s] Smart Toggle:** Instantly start inactive units or stop active ones.
- **[l] Log Deep-Dive:** Open a full-screen streaming log view with text wrapping.
- **[h] Log Highlight:** Enter a Regex pattern in the log view to highlight specific terms.
- **[/] Search:** Real-time fuzzy filtering of the unit list by name or description.
- **[f] Filter Menu:** Multi-select pop-up to toggle visibility of Services, Sockets, Targets, etc.
- **[PgUp/PgDn]:** Fast navigation through large lists and long logs.

## Building and Running
- **Build:** `cargo build`
- **Run:** `cargo run`
- **Test:** `cargo test`

## Development Conventions
- **UI Responsiveness:** Never perform blocking system calls on the main thread.
- **Selection Persistence:** Track units by name to maintain selection stability across refreshes.
- **Robustness:** Use the `Handoff` pattern for any action that might require user interaction or privilege escalation.
