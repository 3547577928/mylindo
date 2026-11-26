# LinuxDo Desktop (macOS)

A minimal Tauri 2 + Rust desktop wrapper for five community sites in native
WebViews: [LinuxDo](https://linux.do/), 稀土掘金 (juejin.cn), V2EX,
NodeSeek and SegmentFault 思否. The floating toolbar is anchored to the
**bottom edge** with **返回**, **前进**, **刷新**, and a **选择站点** dropdown
for switching between sites, so the sites' own top navigation bars stay
visible and clickable. The dropdown shows the current site. The current URL
appears at the bottom center in a read-only address field; focus it (or
press `Cmd+L`) to select and copy the full URL. It updates on page
navigation and SPA history changes without a polling timer. Navigation
controls sit at the bottom right. Menu items **站点** also switch sites with
`Cmd+1`..`Cmd+5`.

Each site gets its own WebView, created on first visit and kept for the
whole session. Switching sites reveals the other window and hides the
current one (geometry is carried over), so each site keeps its scroll
position, back/forward history and login state; cookies are shared through
the persistent data store either way. The last active site and toolbar
visibility are remembered across launches
(`~/Library/Application Support/<identifier>/settings.json`).

Links that would open a new window (`target=_blank`, `window.open`) never
spawn an app window: URLs on the five supported sites stay in the app (a
juejin.cn post opens in the app window; a link to another supported site
switches windows and loads there), while any other site goes to the system
browser. Top-level navigations to other sites (articles, OAuth login
redirects) still load in the app, and **在浏览器中打开当前页面**
(`Cmd+Shift+E`) always hands the current page to the system browser.

Standard macOS menu shortcuts are wired up: `Cmd+R` 刷新, `Cmd+[` / `Cmd+]`
返回/前进, `Cmd+L` 聚焦地址栏, `Cmd+Shift+B` 切换浮动工具栏,
`Cmd+Shift+E` 在浏览器中打开, plus the usual Edit menu (`Cmd+C/X/V/A/Z`)
and App menu items. `Cmd +/-/0` (and Ctrl+mouse wheel) zoom the page.

The toolbar follows the system dark mode and is injected into a closed
Shadow DOM, so the websites' page styles do not affect it.

## Prerequisites

- macOS 11+
- Xcode Command Line Tools (`xcode-select --install`)
- Rust (`rustup` recommended)
- Node.js 18+

## Run in development

```bash
npm install
npm run dev
```

## Build an unsigned app or DMG

```bash
npm run build:app
npm run build:dmg
```

Artifacts are under `src-tauri/target/release/bundle/`.

The remote page URLs and toolbar are created from Rust in
`src-tauri/src/lib.rs`; the config window uses `create: false` so Tauri does
not create a second default window.

## Release signing and notarization

For distribution outside the App Store, use an Apple Developer `Developer ID Application` certificate and set:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="app-specific-password"
export APPLE_TEAM_ID="TEAMID"
npm run build:dmg
```

Alternatively use App Store Connect API credentials (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`).

Before publishing, change `identifier` in `src-tauri/tauri.conf.json` to a reverse-DNS identifier you control, such as `com.yourcompany.linuxdo`.
