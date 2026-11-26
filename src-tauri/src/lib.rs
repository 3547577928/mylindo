use std::sync::Mutex;

use tauri::{
    menu::{AboutMetadata, MenuBuilder, MenuItem, SubmenuBuilder},
    webview::{PageLoadEvent, WebviewWindowBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, RunEvent, Url, WebviewUrl, WebviewWindow,
    WindowEvent,
};

// Supported sites. Only the active site's window exists at launch; the rest
// are created on first switch and then kept for the whole session, so every
// site keeps its position, history and login state.
const LINUXDO_URL: &str = "https://linux.do/";
const JUEJIN_URL: &str = "https://juejin.cn/";
const V2EX_URL: &str = "https://v2ex.com/";
const NODESEEK_URL: &str = "https://www.nodeseek.com/";
const SEGMENTFAULT_URL: &str = "https://segmentfault.com/";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Site {
    LinuxDo,
    Juejin,
    V2ex,
    NodeSeek,
    SegmentFault,
}

impl Site {
    fn all() -> [Site; 5] {
        [
            Site::LinuxDo,
            Site::Juejin,
            Site::V2ex,
            Site::NodeSeek,
            Site::SegmentFault,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Site::LinuxDo => "main",
            Site::Juejin => "juejin",
            Site::V2ex => "v2ex",
            Site::NodeSeek => "nodeseek",
            Site::SegmentFault => "segmentfault",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Site::LinuxDo => "LinuxDo",
            Site::Juejin => "稀土掘金",
            Site::V2ex => "V2EX",
            Site::NodeSeek => "NodeSeek",
            Site::SegmentFault => "思否",
        }
    }

    fn home_url(self) -> &'static str {
        match self {
            Site::LinuxDo => LINUXDO_URL,
            Site::Juejin => JUEJIN_URL,
            Site::V2ex => V2EX_URL,
            Site::NodeSeek => NODESEEK_URL,
            Site::SegmentFault => SEGMENTFAULT_URL,
        }
    }

    /// Key used in settings.json; matches the pre-multi-site values so old
    /// settings files keep working.
    fn persist_key(self) -> &'static str {
        match self {
            Site::LinuxDo => "linuxdo",
            Site::Juejin => "juejin",
            Site::V2ex => "v2ex",
            Site::NodeSeek => "nodeseek",
            Site::SegmentFault => "segmentfault",
        }
    }

    fn from_persisted(key: &str) -> Option<Site> {
        Site::all()
            .into_iter()
            .find(|site| site.persist_key() == key)
    }

    fn from_label(label: &str) -> Option<Site> {
        Site::all().into_iter().find(|site| site.label() == label)
    }
}

fn site_for_url(url: &Url) -> Option<Site> {
    if url.scheme() != "https" {
        return None;
    }
    match url.host_str()? {
        "linux.do" | "www.linux.do" => Some(Site::LinuxDo),
        "juejin.cn" | "www.juejin.cn" => Some(Site::Juejin),
        "v2ex.com" | "www.v2ex.com" => Some(Site::V2ex),
        "nodeseek.com" | "www.nodeseek.com" => Some(Site::NodeSeek),
        "segmentfault.com" | "www.segmentfault.com" => Some(Site::SegmentFault),
        _ => None,
    }
}

/// State shared between the window event handlers and the menu handlers.
struct AppState {
    /// Most recently focused site window; used by menu commands that act on
    /// "the" page (reload, back, open in browser). Site windows never both
    /// have focus, so this stays in sync with what the user sees.
    active: Site,
    /// Whether the floating toolbar is shown, remembered across restarts.
    show_toolbar: bool,
    /// Last URL each site was left on, reported by the toolbar script;
    /// remembered across restarts so a site reopens where reading stopped.
    site_urls: std::collections::BTreeMap<Site, Url>,
}

type SharedState = Mutex<AppState>;

// This UI lives in a Shadow DOM so the websites' own CSS cannot restyle the
// controls. The script is run again after each top-level navigation. The host
// element is left in the light DOM so Rust can reach it (focus the address
// bar, flip visibility) even though the shadow root itself is closed.
const TOOLBAR_SCRIPT: &str = r#"
(() => {
  if (window.top !== window) return;
  const mount = () => {
    if (!document.documentElement || document.getElementById('__linuxdo_desktop_toolbar')) return;

    const host = document.createElement('div');
    host.id = '__linuxdo_desktop_toolbar';
    // Anchored to the bottom so the sites' own top navigation bars stay
    // visible and clickable; both child pills position within this host.
    host.style.cssText = 'position:fixed;bottom:16px;left:0;right:0;height:46px;z-index:2147483647;pointer-events:none;';
    host.tabIndex = -1;
    const shadow = host.attachShadow({ mode: 'closed' });
    shadow.innerHTML = `
      <style>
        :host { all: initial; }
        .address {
          box-sizing: border-box;
          position: absolute;
          left: 50%;
          transform: translateX(-50%);
          width: min(480px, calc(100vw - 640px));
          height: 46px;
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 0 14px;
          border: 1px solid rgba(20, 28, 40, .14);
          border-radius: 12px;
          background: rgba(255, 255, 255, .96);
          box-shadow: 0 4px 18px rgba(20, 28, 40, .12);
          color: #657082;
          pointer-events: auto;
        }
        .address:focus-within {
          outline: 2px solid #3274d9;
          outline-offset: 1px;
        }
        .address svg { flex: 0 0 16px; width: 16px; height: 16px; }
        .address input {
          box-sizing: border-box;
          width: 100%;
          min-width: 0;
          margin: 0;
          padding: 0;
          border: 0;
          outline: none;
          background: transparent;
          color: #273142;
          text-align: center;
          text-overflow: ellipsis;
          font: 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
        }
        .toolbar {
          position: absolute;
          right: 18px;
          pointer-events: auto;
          display: flex;
          align-items: center;
          gap: 6px;
          width: max-content;
          padding: 5px;
          border: 1px solid rgba(20, 28, 40, .14);
          border-radius: 12px;
          background: rgba(255, 255, 255, .94);
          box-shadow: 0 4px 18px rgba(20, 28, 40, .22);
          -webkit-backdrop-filter: blur(12px);
          backdrop-filter: blur(12px);
          font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
        }
        button {
          flex: 0 0 34px;
          width: 34px;
          height: 34px;
          display: grid;
          place-items: center;
          padding: 0;
          border: 0;
          border-radius: 8px;
          color: #273142;
          background: transparent;
          cursor: pointer;
          font: 20px/1 -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
        }
        button:hover { background: rgba(40, 77, 120, .10); }
        button:active { background: rgba(40, 77, 120, .18); }
        button:focus-visible { outline: 2px solid #3274d9; outline-offset: 1px; }
        select {
          box-sizing: border-box;
          width: 124px;
          height: 34px;
          margin: 0;
          padding: 0 8px;
          border: 1px solid rgba(20, 28, 40, .14);
          border-radius: 8px;
          color: #273142;
          background: #fff;
          cursor: pointer;
          font: 13px -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
        }
        select:focus-visible { outline: 2px solid #3274d9; outline-offset: 1px; }
        svg { width: 20px; height: 20px; display: block; }
        @media (prefers-color-scheme: dark) {
          .address, .toolbar { border-color: rgba(255, 255, 255, .16); color: #8b95a6; }
          .address { background: rgba(28, 32, 40, .96); box-shadow: 0 4px 18px rgba(0, 0, 0, .45); }
          .toolbar { background: rgba(28, 32, 40, .94); box-shadow: 0 4px 18px rgba(0, 0, 0, .55); }
          .address input, button { color: #d7dce5; }
          button:hover { background: rgba(120, 160, 220, .18); }
          button:active { background: rgba(120, 160, 220, .28); }
          select { border-color: rgba(255, 255, 255, .16); color: #d7dce5; background: #2a2f3a; }
        }
      </style>
      <div class="address">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M3 12h18M12 3c5 5 5 13 0 18-5-5-5-13 0-18Z" fill="none" stroke="currentColor" stroke-width="1.6"/></svg>
        <input id="address" type="text" readonly aria-label="当前网页网址" spellcheck="false" />
      </div>
      <div class="toolbar" role="toolbar" aria-label="网页导航">
        <button id="back" type="button" aria-label="返回" title="返回">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14.5 5.5 8 12l6.5 6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button id="forward" type="button" aria-label="前进" title="前进">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9.5 5.5 16 12l-6.5 6.5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <button id="refresh" type="button" aria-label="刷新" title="刷新">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19 8.5A7.5 7.5 0 1 0 20 13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><path d="M19 4.5v4h-4" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
        </button>
        <select id="site" aria-label="选择站点" title="选择站点">
          <option value="" disabled hidden>选择站点</option>
          <option value="https://linux.do/">LinuxDo</option>
          <option value="https://juejin.cn/">稀土掘金</option>
          <option value="https://v2ex.com/">V2EX</option>
          <option value="https://www.nodeseek.com/">NodeSeek</option>
          <option value="https://segmentfault.com/">思否</option>
        </select>
      </div>
    `;

    const stop = (event) => event.stopPropagation();
    const back = shadow.getElementById('back');
    const forward = shadow.getElementById('forward');
    const refresh = shadow.getElementById('refresh');
    const site = shadow.getElementById('site');
    const address = shadow.getElementById('address');
    let selectedSite = '';
    let reportedUrl = '';
    const reportUrl = () => {
      const href = window.location.href;
      // Tell Rust where this site was left so the next launch can reopen
      // the same page. Deduplicate: SPA apps fire this on every route change.
      if (href === reportedUrl) return;
      reportedUrl = href;
      window.__TAURI_INTERNALS__?.invoke('plugin:app|report_site_url', {
        url: href,
      }).catch(() => {});
    };
    const updateAddress = () => {
      address.value = window.location.href;
      address.title = window.location.href;
      selectedSite = Array.from(site.options).find((option) =>
        option.value && new URL(option.value).origin === window.location.origin
      )?.value ?? '';
      site.value = selectedSite;
      reportUrl();
    };
    updateAddress();
    // SPA routes do not reload the document. Update on history changes rather
    // than polling, and retain the original return values and receiver.
    for (const method of ['pushState', 'replaceState']) {
      const original = window.history[method];
      window.history[method] = function (...args) {
        const result = original.apply(this, args);
        updateAddress();
        return result;
      };
    }
    for (const event of ['popstate', 'hashchange', 'pageshow']) {
      window.addEventListener(event, updateAddress);
    }
    address.addEventListener('focus', () => address.select());
    for (const control of [back, forward, refresh, site, address]) {
      control.addEventListener('mousedown', stop);
      control.addEventListener('click', stop);
    }
    back.addEventListener('click', () => window.history.back());
    forward.addEventListener('click', () => window.history.forward());
    refresh.addEventListener('click', () => window.location.reload());
    site.addEventListener('change', (event) => {
      event.stopPropagation();
      if (site.value === selectedSite) return;
      site.disabled = true;
      window.location.assign(site.value);
    });
    window.addEventListener('desktop-site-switch-failed', () => {
      site.value = selectedSite;
      site.disabled = false;
    });
    // Focusing the host focuses the address input inside the closed shadow
    // root; the input's focus handler selects the URL for copying.
    host.addEventListener('focus', () => address.focus());
    document.documentElement.appendChild(host);
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount, { once: true });
  } else {
    mount();
  }
})();
"#;

/// Geometry a new site window is born with, captured from another window.
struct WindowOrigin {
    size: LogicalSize<f64>,
    position: LogicalPosition<f64>,
    maximized: bool,
}

fn visible_window_origin(app: &AppHandle) -> Option<WindowOrigin> {
    let outgoing = Site::all()
        .into_iter()
        .find_map(|site| app.get_webview_window(site.label()))
        .filter(|window| window.is_visible().unwrap_or(false))?;
    let scale = outgoing.scale_factor().ok()?;
    let size = outgoing.inner_size().ok()?.to_logical::<f64>(scale);
    let position = outgoing.outer_position().ok()?.to_logical::<f64>(scale);
    Some(WindowOrigin {
        size,
        position,
        maximized: outgoing.is_maximized().ok()?,
    })
}

fn build_site_window(
    app: &AppHandle,
    site: Site,
    url: Url,
    origin: Option<WindowOrigin>,
) -> tauri::Result<WebviewWindow> {
    let navigation_app = app.clone();
    let navigation_label = site.label().to_owned();
    let mut builder = WebviewWindowBuilder::new(app, site.label(), WebviewUrl::External(url))
        .title(site.title())
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .resizable(true)
        .accept_first_mouse(true)
        .visible(false);
    // New windows inherit the visible window's geometry at construction:
    // resizing a never-shown window is unreliable on macOS. First launch (no
    // origin) centers instead.
    match origin {
        Some(origin) => {
            builder = builder
                .inner_size(origin.size.width, origin.size.height)
                .position(origin.position.x, origin.position.y);
            if origin.maximized {
                builder = builder.maximized(true);
            }
        }
        None => builder = builder.center(),
    }
    builder
        .zoom_hotkeys_enabled(true)
        .initialization_script(TOOLBAR_SCRIPT)
        .on_page_load(move |window, payload| {
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let app = window.app_handle();
            let (active, show_toolbar) = app
                .try_state::<SharedState>()
                .map(|state| {
                    let state = state.lock().unwrap();
                    (state.active, state.show_toolbar)
                })
                .unwrap_or((Site::LinuxDo, true));
            // First paint: show only the window that is supposed to be the
            // active site; the hidden one stays hidden until a site switch.
            if active.label() == window.label() && !window.is_visible().unwrap_or(true) {
                let _ = window.show();
                let _ = window.set_focus();
            }
            // The toolbar host is recreated on each top-level navigation;
            // re-apply the remembered visibility.
            if !show_toolbar {
                let _ = window.eval("document.getElementById('__linuxdo_desktop_toolbar')?.style.setProperty('display','none');");
            }
        })
        .on_navigation(move |target| {
            // Navigations to a different supported site reveal that site's
            // window instead of replacing this WebView. Everything else
            // (articles, OAuth redirects, third-party links) loads normally.
            let Some(target_site) = site_for_url(target) else {
                return true;
            };
            if target_site.label() == navigation_label {
                return true;
            }
            let deep_url = deep_navigation_url(target);
            let app = navigation_app.clone();
            // run_on_main_thread executes inline when already on the UI thread.
            // Queue from a worker so the navigation callback returns before
            // the current window is hidden.
            tauri::async_runtime::spawn(async move {
                let ui_app = app.clone();
                if let Err(error) = app.run_on_main_thread(move || {
                    if let Err(error) = reveal_site_window(&ui_app, target_site, deep_url) {
                        eprintln!("Could not switch site: {error}");
                        // The failed target never took the screen; tell the
                        // still-visible pages so their dropdown rolls back.
                        for site in Site::all() {
                            if let Some(window) = ui_app.get_webview_window(site.label()) {
                                let _ = window.eval(
                                    "window.dispatchEvent(new Event('desktop-site-switch-failed')); \
                                     window.alert('站点切换失败，请重试。');",
                                );
                            }
                        }
                    }
                }) {
                    eprintln!("Could not schedule site switch: {error}");
                }
            });
            false
        })
        .on_new_window({
            let new_window_app = app.clone();
            move |url, _features| {
                // target=_blank / window.open never spawns an app window:
                // juejin opens every post with target=_blank, so supported
                // site URLs are routed back into the shell and the rest go
                // to the system browser. Queue the work so this delegate
                // callback returns before we touch any webview.
                let app = new_window_app.clone();
                tauri::async_runtime::spawn(async move {
                    let ui_app = app.clone();
                    let _ = app.run_on_main_thread(move || route_new_window(&ui_app, url));
                });
                tauri::webview::NewWindowResponse::Deny
            }
        })
        .build()
}

/// Show `target`'s window and hide every other site window. The window is
/// created on first use — loading `url`, else the page that site was last
/// left on, else its home page — and then stays alive, keeping its scroll
/// position, history and login state. New windows are born with the visible
/// window's geometry so the switch does not move anything; a deep `url` (a
/// specific post or topic) loads in the incoming window; a site-root switch
/// keeps whatever page that window already shows.
fn reveal_site_window(app: &AppHandle, target: Site, url: Option<Url>) -> tauri::Result<()> {
    if let Some(incoming) = app.get_webview_window(target.label()) {
        if let Some(url) = url {
            incoming.navigate(url)?;
        }
        if incoming.is_visible().unwrap_or(false) {
            // Already the visible site (e.g. Cmd+1 while on LinuxDo); the
            // deep link above was enough.
            incoming.set_focus()?;
            return Ok(());
        }
        sync_visible_geometry(app, &incoming)?;
        incoming.show()?;
        incoming.set_focus()?;
    } else {
        let resume = url
            .or_else(|| resume_url(app, target))
            .unwrap_or_else(|| target.home_url().parse().expect("valid site URL"));
        let origin = visible_window_origin(app);
        let incoming = build_site_window(app, target, resume, origin)?;
        incoming.show()?;
        incoming.set_focus()?;
    }
    for site in Site::all() {
        if site != target {
            if let Some(window) = app.get_webview_window(site.label()) {
                let _ = window.hide();
            }
        }
    }
    if let Some(state) = app.try_state::<SharedState>() {
        state.lock().unwrap().active = target;
    }
    Ok(())
}

/// Carry the visible window's geometry over to `incoming` before it is
/// shown. `visible_window_origin` only picks currently visible windows, and
/// `incoming` is not visible yet, so the origin is always the other window.
fn sync_visible_geometry(app: &AppHandle, incoming: &WebviewWindow) -> tauri::Result<()> {
    let Some(origin) = visible_window_origin(app) else {
        return Ok(());
    };
    incoming.set_size(tauri::Size::Logical(origin.size))?;
    incoming.set_position(tauri::Position::Logical(origin.position))?;
    if origin.maximized {
        let _ = incoming.maximize();
    }
    Ok(())
}

/// Deep URLs (a specific post or topic) load at the target; site-root
/// navigations keep the target window's current page, so switching sites
/// via the dropdown never resets the other site's reading position.
fn deep_navigation_url(url: &Url) -> Option<Url> {
    let is_root = matches!(url.path(), "/" | "");
    (!is_root).then(|| url.clone())
}

/// Decide where a `window.open` / `target=_blank` request goes. Links to
/// supported sites stay in the app — same-site links navigate the active
/// window, cross-site links load in the other site's window — and everything
/// else opens in the system browser.
fn route_new_window(app: &AppHandle, url: Url) {
    let Some(target_site) = site_for_url(&url) else {
        // Ignore non-web schemes (about:blank, custom schemes) instead of
        // puzzling the user with an empty browser tab.
        if matches!(url.scheme(), "http" | "https") {
            open_in_browser(&url);
        }
        return;
    };
    let active = app
        .try_state::<SharedState>()
        .map(|state| state.lock().unwrap().active)
        .unwrap_or(Site::LinuxDo);
    if target_site == active {
        if let Some(window) = app.get_webview_window(active.label()) {
            if let Err(error) = window.navigate(url) {
                eprintln!("Could not follow new-window link in-app: {error}");
            }
        }
        return;
    }
    if let Err(error) = reveal_site_window(app, target_site, deep_navigation_url(&url)) {
        eprintln!("Could not switch site for new-window link: {error}");
    }
}

/// Open `url` with the user's default browser. Uses each platform's native
/// "open this URL" command instead of the opener plugin dependency.
fn open_in_browser(url: &Url) {
    let url = url.as_str().to_owned();
    let _ = std::thread::spawn(move || {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = std::process::Command::new("open");
            command.arg(&url);
            command
        };
        #[cfg(target_os = "windows")]
        let mut command = {
            // cmd starts the URL with its default handler; /c waits for it.
            let mut command = std::process::Command::new("cmd");
            command.args(["/c", "start", "", &url]);
            command
        };
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        let mut command = {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(&url);
            command
        };
        let _ = command
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    });
}

/// Small JSON state file remembering the last site, per-site last URLs and
/// toolbar visibility.
fn settings_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> AppState {
    let defaults = AppState {
        active: Site::LinuxDo,
        show_toolbar: true,
        site_urls: std::collections::BTreeMap::new(),
    };
    let Some(path) = settings_path(app) else {
        return defaults;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return defaults;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return defaults;
    };
    let site_urls = value
        .get("site_urls")
        .and_then(|v| v.as_object())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(key, url)| {
                    let site = Site::from_persisted(key)?;
                    let url = url.as_str()?.parse().ok()?;
                    Some((site, url))
                })
                .collect()
        })
        .unwrap_or_default();
    AppState {
        active: value
            .get("active")
            .and_then(|v| v.as_str())
            .and_then(Site::from_persisted)
            .unwrap_or(Site::LinuxDo),
        show_toolbar: value
            .get("show_toolbar")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        site_urls,
    }
}

fn save_settings(app: &AppHandle, state: &AppState) {
    let Some(path) = settings_path(app) else {
        return;
    };
    let site_urls: serde_json::Map<String, serde_json::Value> = state
        .site_urls
        .iter()
        .map(|(site, url)| {
            (
                site.persist_key().to_owned(),
                serde_json::json!(url.as_str()),
            )
        })
        .collect();
    let text = serde_json::json!({
        "active": state.active.persist_key(),
        "show_toolbar": state.show_toolbar,
        "site_urls": site_urls,
    })
    .to_string();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, text);
}

/// Where `site` was last left, if the toolbar script reported it and it is
/// still a page of that site.
fn resume_url(app: &AppHandle, site: Site) -> Option<Url> {
    let state = app.try_state::<SharedState>()?;
    let url = state.lock().unwrap().site_urls.get(&site).cloned()?;
    if site_for_url(&url) == Some(site) {
        Some(url)
    } else {
        None
    }
}

/// Toolbar-script command: record the page a site is on so the next launch
/// (or first lazy open) reopens on that page.
#[tauri::command]
fn report_site_url(app: AppHandle, url: String) -> Result<(), String> {
    let Ok(url) = Url::parse(&url) else {
        return Ok(());
    };
    let Some(site) = site_for_url(&url) else {
        return Ok(());
    };
    let Some(state) = app.try_state::<SharedState>() else {
        return Err("app state missing".into());
    };
    let mut state = state.lock().unwrap();
    let unchanged = state
        .site_urls
        .get(&site)
        .is_some_and(|current| current == &url);
    if !unchanged {
        state.site_urls.insert(site, url);
        save_settings(&app, &state);
    }
    Ok(())
}

fn current_site_window(app: &AppHandle) -> Option<WebviewWindow> {
    let state = app.try_state::<SharedState>()?;
    let active = state.lock().unwrap().active;
    app.get_webview_window(active.label())
}

/// Toggle the floating toolbar on every loaded site page by flipping the
/// host element's display style from outside the closed shadow root.
fn apply_toolbar_visibility(app: &AppHandle, visible: bool) {
    for site in Site::all() {
        let Some(window) = app.get_webview_window(site.label()) else {
            continue;
        };
        let display = if visible { "''" } else { "'none'" };
        let _ = window.eval(format!(
            "(() => {{ \
               const host = document.getElementById('__linuxdo_desktop_toolbar'); \
               if (host) host.style.display = {display}; \
             }})();"
        ));
    }
}

fn build_app_menu(app: &AppHandle) -> tauri::Result<()> {
    // On macOS the first submenu becomes the system app menu; its standard
    // items (services, hide, hide_others) only exist there.
    #[cfg(target_os = "macos")]
    let app_menu = SubmenuBuilder::new(app, "LinuxDo")
        .about(Some(AboutMetadata {
            name: Some("LinuxDo Desktop".into()),
            version: Some("0.1.0".into()),
            ..Default::default()
        }))
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .separator()
        .quit()
        .build()?;

    #[cfg(not(target_os = "macos"))]
    let app_menu = SubmenuBuilder::new(app, "文件").quit().build()?;

    let edit_menu = SubmenuBuilder::new(app, "编辑")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    // Standard macOS navigation shortcuts, driven through menu accelerators
    // so they work everywhere in the page, and the system Edit menu provides
    // Cmd+C/X/V/A with real pasteboard behavior.
    let reload = MenuItem::with_id(app, "reload", "刷新页面", true, Some("CmdOrCtrl+R"))?;
    let back = MenuItem::with_id(app, "back", "返回", true, Some("CmdOrCtrl+["))?;
    let forward = MenuItem::with_id(app, "forward", "前进", true, Some("CmdOrCtrl+]"))?;
    let focus_address = MenuItem::with_id(
        app,
        "focus-address",
        "聚焦地址栏",
        true,
        Some("CmdOrCtrl+L"),
    )?;
    let open_external = MenuItem::with_id(
        app,
        "open-external",
        "在浏览器中打开当前页面",
        true,
        Some("CmdOrCtrl+Shift+E"),
    )?;
    let toggle_toolbar = MenuItem::with_id(
        app,
        "toggle-toolbar",
        "切换浮动工具栏",
        true,
        Some("CmdOrCtrl+Shift+B"),
    )?;

    let view_menu = SubmenuBuilder::new(app, "显示")
        .item(&reload)
        .separator()
        .item(&back)
        .item(&forward)
        .separator()
        .item(&focus_address)
        .item(&open_external)
        .item(&toggle_toolbar)
        .separator()
        .fullscreen()
        .build()?;

    // One menu entry per site with Cmd+1..5 accelerators, mirroring the
    // toolbar dropdown for keyboard switching.
    let site_menu = SubmenuBuilder::new(app, "站点");
    let mut site_menu = site_menu;
    for (index, site) in Site::all().into_iter().enumerate() {
        let item = MenuItem::with_id(
            app,
            format!("site-{}", site.persist_key()),
            site.title(),
            true,
            Some(format!("CmdOrCtrl+{}", index + 1)),
        )?;
        site_menu = site_menu.item(&item);
    }
    let site_menu = site_menu.build()?;

    let window_menu = SubmenuBuilder::new(app, "窗口")
        .minimize()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&site_menu)
        .item(&window_menu)
        .build()?;

    app.set_menu(menu)?;
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "reload" => {
            if let Some(window) = current_site_window(app) {
                let _ = window.eval("window.location.reload();");
            }
        }
        "back" => {
            if let Some(window) = current_site_window(app) {
                let _ = window.eval("window.history.back();");
            }
        }
        "forward" => {
            if let Some(window) = current_site_window(app) {
                let _ = window.eval("window.history.forward();");
            }
        }
        "focus-address" => {
            if let Some(window) = current_site_window(app) {
                // The shadow root is closed, but focusing the host element
                // forwards focus to the address input, which selects the URL.
                let _ = window.eval(
                    "(() => { \
                       const host = document.getElementById('__linuxdo_desktop_toolbar'); \
                       if (host) host.focus(); \
                     })();",
                );
            }
        }
        "open-external" => {
            if let Some(window) = current_site_window(app) {
                if let Ok(url) = window.url() {
                    open_in_browser(&url);
                }
            }
        }
        "toggle-toolbar" => {
            let Some(state) = app.try_state::<SharedState>() else {
                return;
            };
            let mut state = state.lock().unwrap();
            state.show_toolbar = !state.show_toolbar;
            let visible = state.show_toolbar;
            save_settings(app, &state);
            drop(state);
            apply_toolbar_visibility(app, visible);
        }
        id if id.starts_with("site-") => {
            let Some(site) = Site::from_persisted(id.trim_start_matches("site-")) else {
                return;
            };
            // Menu-driven switching is a site-root reveal: keep that site's
            // current page (or its home page on first open).
            if let Err(error) = reveal_site_window(app, site, None) {
                eprintln!("Could not switch site from menu: {error}");
            }
        }
        _ => {}
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![report_site_url])
        .setup(|app| {
            let settings = load_settings(app.handle());
            let show_toolbar = settings.show_toolbar;
            let active = settings.active;

            // Manage state before building windows: the page-load handler
            // reads it to decide which site's window may show itself.
            app.manage(Mutex::new(settings));

            // Lazy windows: only the remembered (or default) site's window
            // exists at launch, reopening the page that site was last left
            // on. Other sites are created on first switch and then kept
            // alive for the whole session.
            let resume = resume_url(app.handle(), active)
                .unwrap_or_else(|| active.home_url().parse().expect("valid site URL"));
            build_site_window(app.handle(), active, resume, None)?;

            build_app_menu(app.handle())?;
            if !show_toolbar {
                apply_toolbar_visibility(app.handle(), false);
            }

            Ok(())
        })
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .on_window_event(|window, event| {
            if !matches!(event, WindowEvent::Focused(true)) {
                return;
            }
            // Track which site window the user is interacting with so menu
            // commands act on the right page.
            let app = window.app_handle();
            let Some(site) = Site::from_label(window.label()) else {
                return;
            };
            let Some(state) = app.try_state::<SharedState>() else {
                return;
            };
            let mut state = state.lock().unwrap();
            if state.active != site {
                state.active = site;
                save_settings(app, &state);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running LinuxDo desktop application");

    app.run(|app, event| match event {
        // Clicking the dock icon with every window hidden reveals the
        // remembered site again. (macOS only; other platforms exit when the
        // last window closes, so there is nothing to reopen.)
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => {
            let active = app
                .try_state::<SharedState>()
                .map(|state| state.lock().unwrap().active)
                .unwrap_or(Site::LinuxDo);
            if let Some(window) = app.get_webview_window(active.label()) {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        // Closing the last remaining site window quits the app: "close
        // window" keeps meaning "quit app" even with lazy per-site windows.
        RunEvent::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        } => {
            let remaining = Site::all()
                .into_iter()
                .filter_map(|site| app.get_webview_window(site.label()))
                .count();
            if remaining == 0 {
                app.exit(0);
            }
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_site_pages_without_intercepting_login_providers() {
        for url in ["https://linux.do/", "https://linux.do/t/topic/123"] {
            assert!(site_for_url(&url.parse().unwrap()) == Some(Site::LinuxDo));
        }
        for url in ["https://juejin.cn/", "https://juejin.cn/post/123"] {
            assert!(site_for_url(&url.parse().unwrap()) == Some(Site::Juejin));
        }
        for url in [
            "https://v2ex.com/",
            "https://www.v2ex.com/t/1092000",
            "https://www.nodeseek.com/",
            "https://segmentfault.com/q/1010000000000000",
        ] {
            assert!(site_for_url(&url.parse().unwrap()).is_some());
        }
        for url in [
            "https://github.com/login/oauth/authorize",
            "https://linux.do.example.com/",
            "http://juejin.cn/",
            "https://v2ex.com.example.org/",
        ] {
            assert_eq!(site_for_url(&url.parse().unwrap()), None);
        }
    }

    #[test]
    fn site_labels_and_titles_stay_paired() {
        assert_eq!(Site::LinuxDo.label(), "main");
        assert_eq!(Site::Juejin.label(), "juejin");
        assert_eq!(Site::V2ex.label(), "v2ex");
        assert_eq!(Site::NodeSeek.label(), "nodeseek");
        assert_eq!(Site::SegmentFault.label(), "segmentfault");
        // Every label and persist key must be unique within Site::all().
        let sites = Site::all();
        for (i, a) in sites.iter().enumerate() {
            for b in sites.iter().skip(i + 1) {
                assert_ne!(a.label(), b.label(), "duplicate label");
                assert_ne!(a.persist_key(), b.persist_key(), "duplicate persist key");
            }
        }
        // Round-trip persistence keys, including the pre-multi-site values.
        for site in sites {
            assert_eq!(Site::from_persisted(site.persist_key()), Some(site));
            assert_eq!(Site::from_label(site.label()), Some(site));
        }
        assert_eq!(Site::from_persisted("linuxdo"), Some(Site::LinuxDo));
        assert_eq!(Site::from_persisted("juejin"), Some(Site::Juejin));
        assert_eq!(Site::from_persisted("unknown"), None);
    }

    #[test]
    fn site_roots_reveal_only_but_deep_links_navigate() {
        for url in [
            "https://juejin.cn/",
            "https://linux.do/",
            "https://linux.do/?page=2",
            "https://www.nodeseek.com/",
            "https://v2ex.com/",
        ] {
            assert!(
                deep_navigation_url(&url.parse().unwrap()).is_none(),
                "site root should reveal without navigating: {url}"
            );
        }
        for url in [
            "https://juejin.cn/post/7360000000000000000",
            "https://linux.do/t/topic/123/5",
            "https://v2ex.com/t/1092000",
            "https://www.nodeseek.com/post-1280000-1",
            "https://segmentfault.com/q/1010000000000000",
        ] {
            assert!(
                deep_navigation_url(&url.parse().unwrap()).is_some(),
                "deep link should navigate the target window: {url}"
            );
        }
    }
}
