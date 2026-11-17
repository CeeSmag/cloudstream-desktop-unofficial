# CloudStream RS

[![CI](https://github.com/CeeSmag/cloudstream-desktop-unofficial/actions/workflows/ci.yml/badge.svg)](https://github.com/CeeSmag/cloudstream-desktop-unofficial/actions/workflows/ci.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/CeeSmag/cloudstream-desktop-unofficial?label=release)](https://github.com/CeeSmag/cloudstream-desktop-unofficial/releases/latest)

A desktop app for watching shows and movies through **plugins** you choose. It is a rewrite of [CloudStream](https://github.com/recloudstream/cloudstream) for Windows and Linux, with a modern interface and a fast video player.

<p align="center">
  <img src="docs/images/home-with-content.jpg" alt="CloudStream RS Home page with featured content and poster rows" width="800">
</p>

---

> **Disclaimer**  
> This is an independent, unofficial community project. It is **not** affiliated with, endorsed by, or maintained by the official CloudStream team.  
>  
> CloudStream RS does **not** host, scrape, bundle, or distribute media files or copyrighted material. You add your own plugin repositories. Plugins and repositories are made by other people — only add ones you trust.

---

## Download

Get the latest build from the [Releases page](https://github.com/CeeSmag/cloudstream-desktop-unofficial/releases/latest):

| File | For |
|---|---|
| `CloudStream_<version>_x64-setup.exe` | Windows (x64) — installer |
| `CloudStream_<version>_x64_portable.exe` | Windows (x64) — one file, nothing to install |
| `CloudStream-<version>-x86_64.AppImage` / `-aarch64.AppImage` | Linux (Intel/AMD or ARM) |

Everything the app needs (video player, plugin runtime, and Java) is included. After install, the app can check for updates itself under **Settings → General**.

---

## Getting started

CloudStream comes with **no shows or movies of its own**. Plugins bring them in, and your Home page is built from them.

<p align="center">
  <img src="docs/images/home-add-plugins.png" alt="Home page prompt: Add a plugin to fill your Home page" width="800">
</p>

### 1. Open Extensions

On first launch you will see a short guide on Home. Click **Add plugins →**, or go to **Settings → Extensions**.

### 2. Add a repository

Paste a repository link (a `repo.json` URL) and confirm with **+ Add**.

<p align="center">
  <img src="docs/images/settings-repositories.png" alt="Settings → Extensions → Repositories: add a repository URL" width="800">
</p>

### 3. Install plugins

Open the repository, install one or more plugins, then pick a provider from the dropdown in the top bar. Home fills with shelves and a featured banner right away.

---

## What you can do

- **Home** — featured titles and rows from the provider you selected  
- **Browse** — filter and explore the catalog  
- **Search** — look up shows and movies across providers  
- **Library** — bookmarks and continue watching  
- **Player** — hardware-accelerated playback, subtitles, source switching, and skip intro when available  
- **Settings** — preferences, storage, updates, and extension management  

---

## Tips

- Switch providers with the dropdown in the top-right of the window.  
- If a stream fails, the app tries other working sources automatically.  
- Prefer trusted repositories only; you can remove a repository or uninstall a plugin anytime under **Settings → Extensions**.  
- **Delete all saved data** (Settings → General) wipes local settings, history, and caches and restarts the app.

---

## For developers

Build instructions, architecture, and contribution rules live in:

- [AGENTS.md](AGENTS.md) — codebase map and how-to recipes  
- [RULES.md](RULES.md) — coding invariants and quality gates  
- [docs/RELEASING.md](docs/RELEASING.md) — how releases are built and signed  

Quick start from source (Windows):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\setup_sidecar.ps1
cd crates\desktop_tauri\frontend; npm install; cd ..\..\..
powershell -ExecutionPolicy Bypass -File .\scripts\run_tauri.ps1
```

---

## License

GNU General Public License v3.0 or later (GPL-3.0-or-later). See [LICENSE](LICENSE).
