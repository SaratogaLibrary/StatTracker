# StatTracker

A chromeless, cross-platform desktop widget for recording reference-desk question tallies. It talks to a REST compatible web app, caches clicks in a local SQLite file when the network is down, and ships with HTML/CSS/JS templates that can be customized per machine.

## Requirements to Run From Source

- Rust (stable) and a C toolchain (Visual Studio Build Tools on Windows)
- Node.js (for the Tauri CLI only)
- WebView2 on Windows (already present on current Windows 10/11)

## Develop

```bash
npm install
npm run dev
```

The first launch opens **Settings**. Enter the web app base URL with a trailing slash (for example `https://example.com/`), pick a desk from `desks/index.json`, and choose where the local PC's SQLite database, `config.toml`, and templates should live.

`npm run dev` / `npm run build` compile into a per-user data directory: `%LOCALAPPDATA%\StatTracker\target` on Windows, `~/Library/Application Support/StatTracker/target` on macOS, and `$XDG_DATA_HOME/StatTracker/target` (or `~/.local/share/StatTracker/target`) on Linux.

## Build installers

```bash
npm run build
```

Artifacts are written under `StatTracker/target/release/bundle/` in that same directory (NSIS on Windows) when you use the npm scripts.

## Configuration

After install, settings live in `config.toml` inside the chosen storage directory. A pointer file in the OS app-data folder remembers that location.

| Field | Default | Notes |
| --- | --- | --- |
| `base_url` | (required) | Trailing slash expected |
| `desk_id` / `desk_name` | (required) | Associated desk for this machine |
| `autostart` | `true` | Launch at login |
| `always_on_top` | `false` | Pin toggle; unfocused opacity applies only when this is on |
| `unfocused_opacity` | `55` | 0–100 |
| `update_mode` | `notify` | `notify` or `silent` |
| `active_template` | `horizontal` | Folder name under `templates/` |

The application interface supports browser-style zooming. Zoom (Ctrl/Cmd `+` `-` `0`) is session-only and resets to 100% on every application startup. The widget window is not user-resizable; size comes from the active template.

## Templates

Bundled templates (`horizontal`, `vertical`) are copied into the storage `templates/` folder (the path chosen in Settings). Installed/release builds do not overwrite those copies, so customizations survive. `npm run dev` recopies the bundled templates on each launch so edits in the repo `templates/` folder show up.

Each template is a named folder with `manifest.json`, `index.html`, `style.css`, and `script.js`. Extra folders dropped into the storage `templates/` directory appear in Settings after a refresh. Template HTML, CSS, and JS run in a sandboxed iframe and cannot call Tauri APIs. Front-end JS may use `data-action` / `data-slot` hooks and the host-provided `window.StatTracker` helpers (`recordTally`, `refresh`, `cycleTemplate`, `openSettings`, `openHelp`). There is no `invoke`.

Required hooks:

- Drag: `data-tauri-drag-region`
- Close / minimize / settings / help: `data-action="close|minimize|settings|help"`

Optional: `data-slot="buttons|counter|status|desk-name|org-name|title"`, `data-action="always-on-top|refresh|cycle-template"`, placeholders `{{deskName}}`, `{{orgName}}`, `{{windowTitle}}`.

## Local API

All HTTP calls are made from Rust (not the webview), so CORS is not required.

- `GET {base}desks/index.json`.
- `GET {base}desks/view/{deskId}.json`.
- `POST {base}question-tallies/add` as JSON (`X-Requested-With: XMLHttpRequest`) with `desk_id` and `question_type_id` in the body.

Self-signed certificates are accepted only for localhost and `*.loc` / `*.local` / `*.test` hosts.  
_Windows Domain administrators may be able to bypass SmartScreen warnings for Windows clients by using Group Policy Objects._

## License

This project is MIT licensed.