# Package Manager GUI

A clean, modern desktop app that provides a GUI for package managers:
- **macOS** — Homebrew (`brew`)
- **Windows** — winget _(coming soon)_

Built with [Tauri](https://tauri.app) and vanilla HTML/CSS/JavaScript.

## Features

- Search for packages by name
- Install packages with one click and a live log
- View all installed packages and available updates

## Development Setup

### Prerequisites

- macOS with [Homebrew](https://brew.sh) installed
- Node.js (`brew install node`)
- Rust (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

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
