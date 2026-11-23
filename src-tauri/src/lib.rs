use std::sync::Mutex;

use tauri::{
    menu::{AboutMetadata, MenuBuilder, MenuItem, SubmenuBuilder},
    webview::{PageLoadEvent, WebviewWindowBuilder},
    AppHandle, LogicalPosition, LogicalSize, Manager, RunEvent, Url, WebviewUrl, WebviewWindow,
    WindowEvent,
};

// Two supported sites. Both windows exist for the whole session and switching
// is a visibility flip, so each site keeps its position, history and login.
const LINUXDO_URL: &str = "https://linux.do/";
const JUEJIN_URL: &str = "https://juejin.cn/";

const LINUXDO_LABEL: &str = "main";
const JUEJIN_LABEL: &str = "juejin";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Site {
    LinuxDo,
    Juejin,
}

impl Site {
    fn label(self) -> &'static str {
        match self {
            Site::LinuxDo => LINUXDO_LABEL,
            Site::Juejin => JUEJIN_LABEL,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Site::LinuxDo => "LinuxDo",
            Site::Juejin => "稀土掘金",
        }
    }

    fn other(self) -> Site {
        match self {
            Site::LinuxDo => Site::Juejin,
            Site::Juejin => Site::LinuxDo,
        }
    }
}

fn site_for_url(url: &Url) -> Option<Site> {
    if url.scheme() != "https" {
        return None;
    }
    match url.host_str()? {
        "linux.do" | "www.linux.do" => Some(Site::LinuxDo),
        "juejin.cn" | "www.juejin.cn" => Some(Site::Juejin),
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
    host.style.cssText = 'position:fixed;top:16px;left:0;right:0;height:46px;z-index:2147483647;pointer-events:none;';
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
          width: min(480px, calc(100vw - 620px));
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
    const updateAddress = () => {
      address.value = window.location.href;
      address.title = window.location.href;
      selectedSite = Array.from(site.options).find((option) =>
        option.value && new URL(option.value).origin === window.location.origin
      )?.value ?? '';
      site.value = selectedSite;
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

fn build_site_window(
    app: &AppHandle,
    label: &str,
    url: Url,
    visible: bool,
) -> tauri::Result<WebviewWindow> {
    let navigation_app = app.clone();
    let navigation_label = label.to_owned();
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title("LinuxDo")
        .inner_size(1280.0, 820.0)
        .min_inner_size(900.0, 600.0)
        .resizable(true)
        .accept_first_mouse(true)
        .visible(visible)
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
            let app = navigation_app.clone();
            // run_on_main_thread executes inline when already on the UI thread.
            // Queue from a worker so the navigation callback returns before
            // the current window is hidden.
            tauri::async_runtime::spawn(async move {
                let ui_app = app.clone();
                if let Err(error) = app.run_on_main_thread(move || {
                    if let Err(error) = reveal_site_window(&ui_app, target_site) {
                        eprintln!("Could not switch site: {error}");
                        for label in [LINUXDO_LABEL, JUEJIN_LABEL] {
                            if let Some(window) = ui_app.get_webview_window(label) {
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
        .on_new_window(|url, _features| {
            // window.open / target=_blank should not open new app windows.
            // Hand the URL to the system browser instead, so links in posts
            // keep working without spawning weak duplicate shells.
            open_in_browser(&url);
            tauri::webview::NewWindowResponse::Deny
        })
        .build()
}

/// Show `target`'s window and hide the other site's window. The incoming
/// window inherits the outgoing one's on-screen geometry so the switch does
/// not move anything; both webviews stay loaded, each keeping its scroll
/// position, history and login state.
fn reveal_site_window(app: &AppHandle, target: Site) -> tauri::Result<()> {
    let incoming = app
        .get_webview_window(target.label())
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    let outgoing = app.get_webview_window(target.other().label());
    if let Some(outgoing) = &outgoing {
        if outgoing.is_visible().unwrap_or(false) {
            let scale = outgoing.scale_factor()?;
            let size = outgoing.inner_size()?.to_logical::<f64>(scale);
            let position = outgoing.outer_position()?.to_logical::<f64>(scale);
            incoming.set_size(tauri::Size::Logical(LogicalSize::new(
                size.width,
                size.height,
            )))?;
            incoming.set_position(tauri::Position::Logical(LogicalPosition::new(
                position.x, position.y,
            )))?;
            if outgoing.is_maximized().unwrap_or(false) {
                let _ = incoming.maximize();
            }
        }
    }
    incoming.show()?;
    incoming.set_focus()?;
    if let Some(outgoing) = outgoing {
        let _ = outgoing.hide();
    }
    if let Some(state) = app.try_state::<SharedState>() {
        state.lock().unwrap().active = target;
    }
    Ok(())
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

/// Small JSON state file remembering the last site and toolbar visibility.
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
    AppState {
        active: match value.get("active").and_then(|v| v.as_str()) {
            Some("juejin") => Site::Juejin,
            _ => Site::LinuxDo,
        },
        show_toolbar: value
            .get("show_toolbar")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
    }
}

fn save_settings(app: &AppHandle, state: &AppState) {
    let Some(path) = settings_path(app) else {
        return;
    };
    let text = serde_json::json!({
        "active": match state.active {
            Site::LinuxDo => "linuxdo",
            Site::Juejin => "juejin",
        },
        "show_toolbar": state.show_toolbar,
    })
    .to_string();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, text);
}

fn current_site_window(app: &AppHandle) -> Option<WebviewWindow> {
    let state = app.try_state::<SharedState>()?;
    let active = state.lock().unwrap().active;
    app.get_webview_window(active.label())
}

/// Toggle the floating toolbar on every loaded site page by flipping the
/// host element's display style from outside the closed shadow root.
fn apply_toolbar_visibility(app: &AppHandle, visible: bool) {
    for label in [LINUXDO_LABEL, JUEJIN_LABEL] {
        let Some(window) = app.get_webview_window(label) else {
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

    let window_menu = SubmenuBuilder::new(app, "窗口")
        .minimize()
        .separator()
        .close_window()
        .build()?;

    let menu = MenuBuilder::new(app)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&view_menu)
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
        _ => {}
    }
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
            "https://github.com/login/oauth/authorize",
            "https://linux.do.example.com/",
            "http://juejin.cn/",
        ] {
            assert!(site_for_url(&url.parse().unwrap()) == None);
        }
    }

    #[test]
    fn site_labels_and_titles_stay_paired() {
        assert_eq!(Site::LinuxDo.label(), "main");
        assert_eq!(Site::Juejin.label(), "juejin");
        assert_eq!(Site::LinuxDo.other(), Site::Juejin);
        assert_eq!(Site::Juejin.other(), Site::LinuxDo);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let settings = load_settings(app.handle());
            let show_toolbar = settings.show_toolbar;
            let active = settings.active;

            // Manage state before building windows: the page-load handler
            // reads it to decide which site's window may show itself.
            app.manage(Mutex::new(settings));

            // Both sites' windows exist from the start; only the remembered
            // (or default) site is visible. Navigating cross-site reveals the
            // other window without destroying either WebView.
            let linuxdo = build_site_window(
                app.handle(),
                LINUXDO_LABEL,
                LINUXDO_URL.parse().expect("valid LinuxDo URL"),
                active == Site::LinuxDo,
            )?;
            linuxdo.set_title(Site::LinuxDo.title())?;
            let juejin = build_site_window(
                app.handle(),
                JUEJIN_LABEL,
                JUEJIN_URL.parse().expect("valid Juejin URL"),
                active == Site::Juejin,
            )?;
            juejin.set_title(Site::Juejin.title())?;

            let (visible, hidden) = if active == Site::LinuxDo {
                (linuxdo, juejin)
            } else {
                (juejin, linuxdo)
            };
            visible.set_focus()?;
            let _ = hidden.hide();

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
            let Some(state) = app.try_state::<SharedState>() else {
                return;
            };
            let site = if window.label() == JUEJIN_LABEL {
                Site::Juejin
            } else {
                Site::LinuxDo
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
        // Closing one site's window must not leave the app running with the
        // other site hidden; quitting keeps "close window" == "quit app".
        RunEvent::WindowEvent {
            event: WindowEvent::Destroyed,
            ..
        } => {
            let remaining = [LINUXDO_LABEL, JUEJIN_LABEL]
                .iter()
                .filter_map(|label| app.get_webview_window(label))
                .count();
            if remaining == 1 {
                app.exit(0);
            }
        }
        _ => {}
    });
}
