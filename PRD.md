Here is the updated, optimized PRD. I have completely rewritten Section 5 to instruct the AI to use the high-performance local directory parsing method rather than slow, piped shell commands. 

You can copy and paste this directly into your Gemini CLI or Cursor prompt.

***

# PRD: sdtui

## 1. Overview
**sdtui** is a Terminal User Interface (TUI) application written in Rust. Its purpose is to provide a fast, visual interface for managing user-level systemd services (`systemctl --user`), specifically focusing on custom user daemons located in `~/.config/systemd/user/`.

## 2. Tech Stack
* **Language:** Rust
* **TUI Framework:** `ratatui`
* **Terminal Backend:** `crossterm` (Handling raw mode, alternate screen, and input events)
* **Concurrency:** Standard library synchronous execution (No `tokio` required).
* **Dependencies:** `dirs` crate (for resolving the user's home directory).

## 3. Core Features & Keybindings
The application must intercept keyboard events and execute the corresponding commands:
* `Up` / `Down` (or `k` / `j`): Navigate the service list.
* `s`: **Start** the currently selected service.
* `x`: **Stop** the currently selected service.
* `r`: **Daemon Reload** (`systemctl --user daemon-reload`).
* `e`: **Edit** the selected service. *Crucial constraint: Must suspend the TUI to allow the system's default editor (e.g., Neovim) to take over the terminal.*
* `q` / `Esc` / `Ctrl+C`: **Quit** the application gracefully.

## 4. UI / Layout Specification
Using `ratatui`'s `Layout` engine, the screen should be divided into three main areas:

1.  **Left Pane (50% width): "Local Services"**
    * A `List` widget showing the parsed user services.
    * Indicate status visually (e.g., Green text for 'active', Red for 'failed', Gray for 'inactive').
    * Selected item should be highlighted with a background color and a `>>` prefix.
2.  **Right Pane (50% width): "Details"**
    * A `Paragraph` widget showing details of the currently highlighted service.
    * Should display: Service Name, Load State, Active State, and Sub State.
3.  **Bottom Pane (Fixed height 3): "Keybindings"**
    * A `Paragraph` acting as a footer, displaying available commands: `[s] Start  [x] Stop  [r] Reload  [e] Edit  [q] Quit`.

## 5. System Interactions & High-Performance Fetching

### A. Fetching Services (The Fast Path)
To prevent spawning unnecessary child processes or parsing system-wide user daemons (like pipewire), the AI must implement a two-step fetch:

1.  **Read Local Directory:** Use `dirs::home_dir()` to locate `~/.config/systemd/user/`. Use `std::fs::read_dir` to collect all filenames in this directory that end with the `.service` extension.
2.  **Batch Status Query:** Pass *only* those specific filenames as arguments to a single systemctl command:
    ```rust
    Command::new("systemctl")
        .args(["--user", "list-units", "--all", "--plain", "--no-legend"])
        .args(&local_service_names) // Pass the vector of names here
    ```
3.  **Parsing:** Parse the whitespace-separated stdout into the `ServiceUnit` struct (columns: `UNIT`, `LOAD`, `ACTIVE`, `SUB`, `DESCRIPTION`).

### B. State Management Actions
* Start: `std::process::Command::new("systemctl").args(["--user", "start", "<unit_name>"])`
* Stop: `std::process::Command::new("systemctl").args(["--user", "stop", "<unit_name>"])`
* Reload: `std::process::Command::new("systemctl").args(["--user", "daemon-reload"])`

*(Note: The application should re-trigger the "Fetching Services" logic immediately after any of these commands finish to update the UI).*

### C. The "Edit" Action (Terminal Context Switch)
Executing `systemctl --user edit <unit_name>` launches an interactive text editor. Because Ratatui runs in an Alternate Screen with Raw Mode enabled, the AI **must** implement this exact sequence for the `e` keybind:
1.  Disable Raw Mode (`crossterm::terminal::disable_raw_mode()`).
2.  Leave Alternate Screen (`crossterm::terminal::LeaveAlternateScreen`).
3.  Spawn the `systemctl edit` command using `.spawn()` (NOT `.output()`) so it attaches to the current TTY.
4.  `.wait()` for the child process to exit.
5.  Re-enter Alternate Screen.
6.  Re-enable Raw Mode.
7.  Clear the Ratatui terminal state to force a full redraw.

## 6. Target Application State (Rust Structs)
```rust
struct ServiceUnit {
    name: String,
    load: String,
    active: String,
    sub: String,
    description: String,
}

struct App {
    services: Vec<ServiceUnit>,
    list_state: ratatui::widgets::ListState,
    should_quit: bool,
}
```

## 7. Implementation Milestones for the AI
1.  **Setup & Boilerplate:** Initialize the `ratatui` + `crossterm` alternate screen setup and the main event loop. Add the `dirs` crate.
2.  **Targeted Data Fetching:** Implement the `std::fs` directory reading to isolate local `.service` files, then pass those to `systemctl list-units` and parse the output.
3.  **UI Rendering:** Build the layout chunks and render the `List` and `Paragraph` widgets based on the `App` state.
4.  **Action Dispatcher:** Map the basic keybindings (`s`, `x`, `r`) to synchronous `std::process::Command` calls.
5.  **The Edit Handoff:** Implement the critical terminal teardown/rebuild logic specifically for the `e` (edit) keybind.

***

Feed that directly to your CLI. Do you want to review the code it generates together once you run the prompt?
