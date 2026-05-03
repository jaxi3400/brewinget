<div align="center">
  <img src="path/to/icon.png" width="128" alt="Brewinget icon" />
  
  # Brewinget
  
  **A modern, clean GUI for winget — install, update, and manage Windows apps without the command line.**
  
  [![Latest Release](https://img.shields.io/github/v/release/jaxi3400/brewinget)](https://github.com/jaxi3400/brewinget/releases/latest)
  [![License](https://img.shields.io/github/license/jaxi3400/brewinget)](LICENSE)
</div>

---

## Download

**[⬇️ Download the latest release](https://github.com/jaxi3400/brewinget/releases/latest)**

Pick the `.exe` (NSIS installer) for normal use, or the `.msi` for enterprise / Group Policy deployment.

> **Requires Windows 10/11 (x64).** UAC prompt on launch is expected — Brewinget runs elevated so winget operations never ask for credentials mid-session.

<img width="826" height="564" alt="Brewwinget-frontpage" src="https://github.com/user-attachments/assets/352e0ed0-9192-4e6e-9aa9-50ee66174dcb" />

---

## Features

### Search
- Search winget and Microsoft Store by name
- Quick-pick chips for common apps
- Microsoft Store results hidden by default
- Marks already-installed packages

### Manage installed apps
- Filter and sort the full list
- Per-row Update with silent install support
- Per-row Uninstall with confirmation safeguards
- "Update All" queue with live progress, skip, and abort
- ARP-only registry entries hidden by default

### Smart failure handling
- Detects when an update fails because the app is still running — shows actionable warning, not a generic error
- Reads installer log files (UTF-8 + UTF-16 LE) to find the real failure reason
- App-specific hints for VS Code, Chrome, Slack, Teams, Discord, Spotify

### Reliability
- Silent installs auto-retry without `--silent` if the installer doesn't support it
- Streaming log output for every operation
- Fully offline — no telemetry, no update checks, no network calls except to winget

<img width="825" height="673" alt="Brewwinget-installed" src="https://github.com/user-attachments/assets/afd28678-8144-4a4e-a889-51238dc96e44" />
<img width="828" height="672" alt="Brewwinget-update-all" src="https://github.com/user-attachments/assets/e4766336-7cfc-4081-b372-68d441e8e10b" />

---

## About this project

Brewinget started as a learning project — built with [Claude Code](https://claude.com/claude-code) over a week to scratch a personal itch (a winget GUI that doesn't feel like Windows 7). It's a hobby project, not commercial software, but it's the daily-driver package manager on my own machine.

Bug reports and feature ideas welcome via [GitHub Issues](../../issues).

The name comes from the two package managers it was designed to support: **brew** (macOS, currently in code but not distributed) and **winget** (Windows, the actively supported platform).

---

## Building from source

### Windows
```powershell
winget install OpenJS.NodeJS.LTS
winget install Rustlang.Rustup
winget install Microsoft.VisualStudio.2022.BuildTools
```

### macOS (development only — not distributed)
```bash
brew install node
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Run in development
```bash
git clone https://github.com/jaxi3400/brewinget.git
cd brewinget
npm install
npm run tauri dev
```

> **On Windows, run dev mode from an already-elevated PowerShell** to avoid a UAC prompt on every hot-reload. `Start-Process pwsh -Verb RunAs` then `cd` to the project.

### Build for production
```bash
npm run tauri build
```

The built app and installers will be in `src-tauri/target/release/bundle/`.

---

## Tech stack

Built with [Tauri 2](https://tauri.app), Rust on the backend, vanilla HTML/CSS/JavaScript on the frontend. No frameworks, no build pipeline beyond Tauri's own tooling.
