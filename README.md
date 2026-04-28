# Brewinget

A clean, modern desktop GUI for package managers — named after the two it supports:

- **macOS** — Homebrew (`brew`)
- **Windows** — winget

Built with [Tauri](https://tauri.app) and vanilla HTML/CSS/JavaScript.

## Features

- Search for packages by name, with display name, ID, version, and source
- Install packages with one click and a live streaming log
- View all installed packages with update badges
- Filter out Windows registry (ARP) entries and Microsoft Store noise
- Update packages in place

## Development Setup

### Prerequisites — macOS

- macOS with [Homebrew](https://brew.sh) installed
- Node.js (`brew install node`)
- Rust (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

### Prerequisites — Windows

- Windows 10/11 with winget installed (comes with Windows by default)
- Node.js (`winget install OpenJS.NodeJS.LTS`)
- Rust (`winget install Rustlang.Rustup`)
- Visual Studio C++ Build Tools (`winget install Microsoft.VisualStudio.2022.BuildTools`)

### Run in development

```bash
npm install
npm run tauri dev
```

### Build for production

```bash
npm run tauri build
```

The built app will be in `src-tauri/target/release/bundle/`.
