# Z

Multi-platform live streaming aggregator: **Huya / Douyu / Bilibili / Douyin / Kuaishou** — in-app playback, read-only chat, and follow management. Runs locally, no server required.

> Current version v0.0.2 · Tauri 2 + React · Windows 10/11 (WebView2 Runtime required)

## Install

Two options (see GitHub Releases):

- **MSI installer**: `Z_0.0.2_x64_en-US.msi`
- **Portable**: single executable from the release bundle

## Features

| Feature | Description |
|---|---|
| Cross-platform search | Type a keyword, search all five platforms at once; filter by platform, sort by viewers / start time |
| In-app playback | Native HLS/FLV streams; quality switching (Douyin/Kuaishou/Douyu/Bilibili multi-tier, defaults to best free tier) |
| Read-only chat | Real-time chat for all five platforms. Never sends, never injects |
| My follows | Favorite rooms in-app + one-click import from each platform; per-platform visibility toggles |
| Watch history | Auto-recorded with retention options (forever / 7 / 30 / 90 days) |
| Login management | Official QR / embedded login per platform; sessions stay on your machine |
| Session migration | Export login sessions to a file → import on a new device |
| Themes | Follow system / light / dark (dual-theme app icon included) |

## Quick Start

1. **Log in to platforms** (optional, unlocks follows & search): Settings → platform row → QR login (Bilibili/Douyu/Kuaishou) or embedded login (Huya/Douyin)
2. **Search or import follows**: use the top search bar, or click "Sync platform follows" on the Follows page
3. **Watch**: click a card — quality menu at top-right of the player, chat on the right panel, volume / external open in the bottom bar
4. **Live status**: the "Follows live now" strip on the home page shows which followed streamers are live

## Settings

- **Platforms**: per-platform login/logout, enable/disable (disabled = excluded from search), connectivity check (portal latency + real API verification with root-cause hints)
- **Follow visibility**: hide follows from specific platforms (feeds / home / counters linked)
- **Appearance**: three theme modes
- **Playback**: volume memory, default mute, card click behavior (in-app / external browser)
- **Data & privacy**: export/import login sessions, clear web cache (clears logins), history retention
- **About**: version info, check for updates

## In-App Updates

Settings → About → "Check for updates": fetches version info from the update feed, verifies the signature, downloads and installs automatically, then restarts.

## Data & Privacy

- Login sessions (cookies) are stored only in the local app data directory; nothing goes through third parties
- Favorites / history / settings live in a local SQLite database (`streamflow.db`)
- Chat is read-only — the app never sends or injects chat messages on your behalf
- Logging out clears the local session immediately; changing your password on the platform also invalidates it

## Known Limitations

- **Douyin**: guest hot-feed is blocked by platform risk control (search / playback / chat are unaffected; search requires in-app login)
- **Kuaishou**: some catalog endpoints are degraded by TLS-fingerprint risk control (search / playback / chat are unaffected)
- **Douyin login**: requires completing QR / captcha inside the login window (platform security requirement)
- Signature / risk-control countermeasures may break as platforms update — maintained on a best-effort basis

## Development

```bash
# Frontend
cd frontend && npm install && npm run build

# Backend (custom-protocol feature is mandatory, otherwise the window loads a dev URL)
cd src-tauri && cargo build --release --features custom-protocol

# Artifacts
# src-tauri/target/release/streamflow.exe (portable)
# src-tauri/target/release/bundle/msi/*.msi (installer)
```

Stack: Tauri 2 + React 19 + Rust (reqwest/tokio/sqlx) + hls.js + mpegts.js.

## License

For learning and personal use only. All platform content belongs to its respective owners.
