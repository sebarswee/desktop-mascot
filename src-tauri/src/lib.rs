#![allow(unexpected_cfgs)]

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, PhysicalPosition, State};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

// ── State definitions ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum MascotState {
    Idle,
    Walk,
    Peek,
    Disappear,
    Reappear,
    Interact,
    Chat,
}

#[derive(Debug, Clone, Copy)]
enum PeekEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy)]
enum InteractType {
    Wave,
    Jump,
    Spin,
}

// ── Config ─────────────────────────────────────────────────────────

// ── LLM Config ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LlmProvider {
    Claude,
    Openai,
    #[serde(rename = "openai_compatible")]
    OpenaiCompatible,
    Gemini,
    Ollama,
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::Claude
    }
}

const KEYRING_SERVICE: &str = "desktop-mascot";
const KEYRING_USERNAME: &str = "llm_api_key";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmConfig {
    #[serde(default)]
    provider: LlmProvider,
    #[serde(default, skip_serializing)]
    api_key: String,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default)]
    base_url: Option<String>,
}

fn default_model() -> String {
    "claude-3-5-sonnet-20241022".to_string()
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: LlmProvider::Claude,
            api_key: String::new(),
            model: default_model(),
            base_url: None,
        }
    }
}

// ── Behavior Config ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BehaviorConfig {
    idle_weight: u32,
    walk_weight: u32,
    peek_weight: u32,
    disappear_weight: u32,
    interact_weight: u32,
    #[serde(default)]
    show_in_dock: bool,
    #[serde(default = "default_true")]
    show_in_menu_bar: bool,
    #[serde(default)]
    fixed_corner: Option<String>,
    #[serde(default)]
    llm: LlmConfig,
    #[serde(default = "default_true")]
    auto_close_chat: bool,
    #[serde(default = "default_chat_shortcut")]
    chat_shortcut: String,
    #[serde(default)]
    auto_start: bool,
}

fn default_true() -> bool {
    true
}

#[cfg(target_os = "macos")]
fn default_chat_shortcut() -> String {
    "Cmd+Shift+C".to_string()
}

#[cfg(not(target_os = "macos"))]
fn default_chat_shortcut() -> String {
    "Ctrl+Alt+C".to_string()
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            idle_weight: 70,
            walk_weight: 15,
            peek_weight: 8,
            disappear_weight: 5,
            interact_weight: 2,
            show_in_dock: false,
            show_in_menu_bar: true,
            fixed_corner: None,
            llm: LlmConfig::default(),
            auto_close_chat: true,
            chat_shortcut: default_chat_shortcut(),
            auto_start: false,
        }
    }
}

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("desktop-mascot")
}

fn config_path() -> PathBuf {
    config_dir().join("behavior.json")
}

fn ensure_config_dir() -> Result<(), String> {
    let dir = config_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn load_or_create_config() -> BehaviorConfig {
    let path = config_path();

    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(config) = serde_json::from_str::<BehaviorConfig>(&content) {
                println!("[Config] Loaded from {:?}", path);
                return config;
            }
        }
    }

    let default = BehaviorConfig::default();
    if ensure_config_dir().is_ok() {
        if let Ok(json) = serde_json::to_string_pretty(&default) {
            let _ = std::fs::write(&path, json);
            println!("[Config] Created default at {:?}", path);
        }
    }
    default
}

fn save_config(config: &BehaviorConfig) -> Result<(), String> {
    ensure_config_dir()?;
    let path = config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn get_api_key_from_keyring() -> String {
    match keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME) {
        Ok(entry) => match entry.get_password() {
            Ok(key) => key,
            Err(_) => String::new(),
        },
        Err(_) => String::new(),
    }
}

fn set_api_key_in_keyring(api_key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USERNAME)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    if api_key.is_empty() {
        let _ = entry.delete_credential();
        Ok(())
    } else {
        entry.set_password(api_key)
            .map_err(|e| format!("Keyring save failed: {}", e))
    }
}

fn load_or_create_llm_config() -> LlmConfig {
    let mut config = load_or_create_config().llm;
    config.api_key = get_api_key_from_keyring();
    config
}

fn save_llm_config(config: &LlmConfig) -> Result<(), String> {
    let mut behavior = load_or_create_config();
    let api_key = config.api_key.clone();
    behavior.llm = config.clone();
    save_config(&behavior)?;
    set_api_key_in_keyring(&api_key)
}

// ── macOS Window helpers ───────────────────────────────────────────

#[cfg(target_os = "macos")]
fn set_dock_visibility(show: bool) {
    use objc::runtime::{Class, Object};
    use objc::*;
    use std::os::raw::c_long;

    unsafe {
        let cls = Class::get("NSApplication").unwrap();
        let app: *mut Object = msg_send![cls, sharedApplication];
        let policy: c_long = if show { 0 } else { 1 }; // 0 = Regular, 1 = Accessory
        let _: () = msg_send![app, setActivationPolicy: policy];
    }
}

// Windows / Linux: no Dock concept; window already hidden from taskbar via `skip_taskbar: true`
#[cfg(not(target_os = "macos"))]
fn set_dock_visibility(_show: bool) {}

#[cfg(target_os = "macos")]
fn set_window_all_spaces(window: &tauri::WebviewWindow) {
    use objc::runtime::Object;
    use objc::*;
    use std::os::raw::c_ulong;

    unsafe {
        let ns_window = window.ns_window().expect("ns_window");
        let behavior: c_ulong = 1; // NSWindowCollectionBehaviorCanJoinAllSpaces
        let _: () = msg_send![ns_window as *mut Object, setCollectionBehavior: behavior];
    }
}

// Windows / Linux: window manager differences are too large to unify; rely on OS defaults
#[cfg(not(target_os = "macos"))]
fn set_window_all_spaces(_window: &tauri::WebviewWindow) {}

// ── Tray (Menu Bar) ────────────────────────────────────────────────

fn build_tray_menu(app: &tauri::AppHandle) -> Result<tauri::menu::Menu<tauri::Wry>, tauri::Error> {
    let menu = tauri::menu::Menu::new(app)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let settings = tauri::menu::MenuItem::with_id(app, "settings", "设置...", true, None::<&str>)?;
    let toggle_visible = tauri::menu::MenuItem::with_id(app, "toggle", "显示 / 隐藏", true, None::<&str>)?;
    menu.append(&settings)?;
    menu.append(&toggle_visible)?;
    menu.append(&tauri::menu::PredefinedMenuItem::separator(app)?)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn open_settings_window_impl(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("settings") {
        window.show()?;
        window.set_focus()?;
    } else {
        let _window = tauri::WebviewWindowBuilder::new(
            app,
            "settings",
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("桌面宠物设置")
        .inner_size(280.0, 420.0)
        .resizable(false)
        .center()
        .build()?;
    }
    Ok(())
}

fn open_chat_window_impl(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("chat") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _window = tauri::WebviewWindowBuilder::new(
            app,
            "chat",
            tauri::WebviewUrl::App("chat.html".into()),
        )
        .title("与宠物对话")
        .inner_size(580.0, 720.0)
        .center()
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()?;

        if let Some(window) = app.get_webview_window("chat") {
            let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
        }
    }

    Ok(())
}

fn close_chat_window_impl(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("chat") {
        let _ = window.hide();
    }
    Ok(())
}

fn create_tray(app: &tauri::AppHandle) -> Result<tauri::tray::TrayIcon, tauri::Error> {
    let menu = build_tray_menu(app)?;

    let mut builder = tauri::tray::TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("桌面宠物")
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "quit" => {
                    app.exit(0);
                }
                "toggle" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let visible = window.is_visible().unwrap_or(true);
                        let _ = if visible { window.hide() } else { window.show() };
                    }
                }
                "settings" => {
                    let _ = open_settings_window_impl(app);
                }
                _ => {}
            }
        });

    // Try to load tray icon from app bundle or fallback to icon file
    let icon_result = if let Some(icon) = app.default_window_icon() {
        Ok(icon.clone())
    } else {
        let icon_path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
            .join("src-tauri")
            .join("icons")
            .join("32x32.png");
        tauri::image::Image::from_path(icon_path)
    };

    match icon_result {
        Ok(icon) => {
            builder = builder.icon(icon);
            println!("[Tray] Icon loaded successfully");
        }
        Err(e) => {
            eprintln!("[Tray] Failed to load icon: {}", e);
        }
    }

    builder.build(app)
}

// ── Event payload ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct StatePayload {
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    peek_edge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interact_type: Option<String>,
}

fn emit_state(
    app: &tauri::AppHandle,
    state: MascotState,
    peek_edge: Option<PeekEdge>,
    interact_type: Option<InteractType>,
) {
    let payload = StatePayload {
        state: format!("{:?}", state).to_lowercase(),
        peek_edge: peek_edge.map(|e| format!("{:?}", e).to_lowercase()),
        interact_type: interact_type.map(|t| format!("{:?}", t).to_lowercase()),
    };
    let _ = app.emit("mascot:state", payload);
}

// ── Screen helpers ─────────────────────────────────────────────────

fn get_primary_bounds(app: &tauri::AppHandle) -> Option<(i32, i32, i32, i32)> {
    let monitor = app.primary_monitor().ok()??;
    let size = monitor.size();
    let pos = monitor.position();
    Some((
        pos.x,
        pos.y,
        pos.x + size.width as i32,
        pos.y + size.height as i32,
    ))
}

fn is_near_edge(x: i32, y: i32, bounds: (i32, i32, i32, i32)) -> bool {
    let threshold = 80;
    let (min_x, min_y, max_x, max_y) = bounds;
    x - min_x < threshold
        || max_x - x < threshold
        || y - min_y < threshold
        || max_y - y < threshold
}

fn pick_random_position(bounds: (i32, i32, i32, i32), window_size: i32, margin: i32) -> (i32, i32) {
    let (_, _, max_x, max_y) = bounds;
    let mut rng = rand::thread_rng();
    (
        rng.gen_range(margin..(max_x - window_size - margin).max(margin + 1)),
        rng.gen_range(margin..(max_y - window_size - margin).max(margin + 1)),
    )
}

fn pick_corner_position(corner: &str, bounds: (i32, i32, i32, i32), window_size: i32, margin: i32) -> (i32, i32) {
    let (min_x, min_y, max_x, max_y) = bounds;
    let offset = margin * 3;

    let base_x = match corner {
        "top_left" | "bottom_left" => min_x + offset,
        "top_right" | "bottom_right" => max_x - window_size - offset,
        _ => min_x + offset,
    };

    let base_y = match corner {
        "top_left" | "top_right" => min_y + offset,
        "bottom_left" | "bottom_right" => max_y - window_size - offset,
        _ => min_y + offset,
    };

    let mut rng = rand::thread_rng();
    let jitter = 30;
    (
        (base_x + rng.gen_range(-jitter..jitter)).clamp(margin, max_x - window_size - margin),
        (base_y + rng.gen_range(-jitter..jitter)).clamp(margin, max_y - window_size - margin),
    )
}

// ── State machine ──────────────────────────────────────────────────

struct StateMachineInner {
    state: MascotState,
    target_x: i32,
    target_y: i32,
    speed: f64,
    idle_ticks: u32,
    peek_edge: PeekEdge,
    peek_timer: u32,
    disappear_timer: u32,
    reappear_timer: u32,
    interact_type: InteractType,
    interact_timer: u32,
    config: BehaviorConfig,
    pre_peek_x: i32,
    pre_peek_y: i32,
}

type StateMachine = Arc<Mutex<StateMachineInner>>;

// ── Chat History ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
    timestamp: u64,
}

#[derive(Clone)]
struct ChatHistory(Arc<Mutex<Vec<ChatMessage>>>);

impl ChatHistory {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn push(&self, role: &str, content: String) {
        let msg = ChatMessage {
            role: role.to_string(),
            content,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        let mut hist = self.0.lock().unwrap();
        hist.push(msg);
        // Keep last 100 messages to prevent unbounded growth
        if hist.len() > 100 {
            let excess = hist.len() - 100;
            hist.drain(0..excess);
        }
    }

    fn get_all(&self) -> Vec<ChatMessage> {
        self.0.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

fn transition_to(
    sm: &mut StateMachineInner,
    new_state: MascotState,
    bounds: (i32, i32, i32, i32),
) {
    let (_, _, _max_x, _max_y) = bounds;
    let window_size = 200;
    let margin = 80;
    let mut rng = rand::thread_rng();

    match new_state {
        MascotState::Walk => {
            let (tx, ty) = if let Some(ref corner) = sm.config.fixed_corner {
                pick_corner_position(corner, bounds, window_size, margin)
            } else {
                pick_random_position(bounds, window_size, margin)
            };
            sm.target_x = tx;
            sm.target_y = ty;
            sm.speed = rng.gen_range(4.0..10.0);
        }
        MascotState::Idle => {
            sm.idle_ticks = 0;
        }
        MascotState::Peek => {
            let edge = match rng.gen_range(0..4) {
                0 => PeekEdge::Top,
                1 => PeekEdge::Bottom,
                2 => PeekEdge::Left,
                _ => PeekEdge::Right,
            };
            sm.peek_edge = edge;
            sm.peek_timer = rng.gen_range(30..50); // 3–5 s
            sm.pre_peek_x = sm.target_x;
            sm.pre_peek_y = sm.target_y;

            let depth = rng.gen_range(40..80);
            let (min_x, min_y, max_x, max_y) = bounds;
            match edge {
                PeekEdge::Top => {
                    sm.target_x = sm.pre_peek_x;
                    sm.target_y = min_y - (window_size - depth);
                }
                PeekEdge::Bottom => {
                    sm.target_x = sm.pre_peek_x;
                    sm.target_y = max_y - depth;
                }
                PeekEdge::Left => {
                    sm.target_x = min_x - (window_size - depth);
                    sm.target_y = sm.pre_peek_y;
                }
                PeekEdge::Right => {
                    sm.target_x = max_x - depth;
                    sm.target_y = sm.pre_peek_y;
                }
            }
            sm.speed = rng.gen_range(6.0..12.0);
        }
        MascotState::Disappear => {
            sm.disappear_timer = 15; // 1.5 s for CSS animation
        }
        MascotState::Reappear => {
            sm.reappear_timer = 15; // 1.5 s for CSS animation
        }
        MascotState::Interact => {
            sm.interact_type = match rng.gen_range(0..3) {
                0 => InteractType::Wave,
                1 => InteractType::Jump,
                _ => InteractType::Spin,
            };
            sm.interact_timer = rng.gen_range(20..40); // 2–4 s
        }
        MascotState::Chat => {
            // No transition setup needed; Chat state is entered directly
        }
    }
    sm.state = new_state;
}

fn pick_next_state(sm: &StateMachineInner) -> MascotState {
    let mut rng = rand::thread_rng();

    // Fixed corner mode: never walk or peek; stay put with idle / interact / disappear
    let (walk_w, peek_w) = if sm.config.fixed_corner.is_some() {
        println!("[pick_next_state] fixed_corner={:?}, disabling walk/peek", sm.config.fixed_corner);
        (0, 0)
    } else {
        (sm.config.walk_weight, sm.config.peek_weight)
    };

    let total = sm.config.idle_weight
        + walk_w
        + peek_w
        + sm.config.disappear_weight
        + sm.config.interact_weight;
    let r = rng.gen_range(0..total);

    let mut cum = 0;
    cum += sm.config.idle_weight;
    if r < cum { return MascotState::Idle; }
    cum += walk_w;
    if r < cum { return MascotState::Walk; }
    cum += peek_w;
    if r < cum { return MascotState::Peek; }
    cum += sm.config.disappear_weight;
    if r < cum { return MascotState::Disappear; }
    MascotState::Interact
}

async fn tick(state_machine: StateMachine, app: tauri::AppHandle) {
    let Some(bounds) = get_primary_bounds(&app) else { return; };
    let mut sm = state_machine.lock().unwrap_or_else(|e| e.into_inner());
    let Some(window) = app.get_webview_window("main") else { return; };
    let pos = window.outer_position().unwrap_or(PhysicalPosition::new(0, 0));

    // Auto-sync Chat/Idle state based on chat window visibility (commands no longer hold the lock)
    let chat_visible = app.get_webview_window("chat")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);

    if chat_visible && sm.state != MascotState::Chat {
        sm.state = MascotState::Chat;
        emit_state(&app, MascotState::Chat, None, None);
    } else if !chat_visible && sm.state == MascotState::Chat {
        transition_to(&mut sm, MascotState::Idle, bounds);
        emit_state(&app, MascotState::Idle, None, None);
    }

    match sm.state {
        MascotState::Idle => {
            sm.idle_ticks += 1;

            // Fixed corner mode: gently pull back to anchor position
            if let Some(ref corner) = sm.config.fixed_corner {
                let anchor = pick_corner_position(corner, bounds, 200, 80);
                let dx = anchor.0 - pos.x;
                let dy = anchor.1 - pos.y;
                let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
                if dist > 5.0 {
                    let step = (dist * 0.05).max(1.0).min(6.0);
                    let ratio = step / dist;
                    let new_x = pos.x + (dx as f64 * ratio) as i32;
                    let new_y = pos.y + (dy as f64 * ratio) as i32;
                    let _ = window.set_position(PhysicalPosition::new(new_x, new_y));
                }
            }

            if sm.idle_ticks > 30 {
                let next = pick_next_state(&sm);
                transition_to(&mut sm, next, bounds);
                emit_state(&app, sm.state, None, None);
                println!(
                    "[Mascot] Idle -> {:?} | target=({}, {})",
                    sm.state, sm.target_x, sm.target_y
                );
            }
        }

        MascotState::Walk => {
            let dx = sm.target_x - pos.x;
            let dy = sm.target_y - pos.y;
            let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();

            if dist < sm.speed {
                let _ = window.set_position(PhysicalPosition::new(sm.target_x, sm.target_y));

                // Near edge? chance to peek instead of idle
                if is_near_edge(sm.target_x, sm.target_y, bounds) {
                    let mut rng = rand::thread_rng();
                    if rng.gen_bool(0.3) {
                        transition_to(&mut sm, MascotState::Peek, bounds);
                        emit_state(&app, sm.state, Some(sm.peek_edge), None);
                        println!("[Mascot] Walk -> Peek | edge={:?}", sm.peek_edge);
                        return;
                    }
                }

                transition_to(&mut sm, MascotState::Idle, bounds);
                emit_state(&app, sm.state, None, None);
                println!("[Mascot] Walk -> Idle | arrived at=({}, {})", sm.target_x, sm.target_y);
            } else {
                let ratio = sm.speed / dist;
                let new_x = pos.x + (dx as f64 * ratio) as i32;
                let new_y = pos.y + (dy as f64 * ratio) as i32;
                let _ = window.set_position(PhysicalPosition::new(new_x, new_y));
            }
        }

        MascotState::Peek => {
            let dx = sm.target_x - pos.x;
            let dy = sm.target_y - pos.y;
            let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();

            if dist < sm.speed {
                let _ = window.set_position(PhysicalPosition::new(sm.target_x, sm.target_y));
                sm.peek_timer = sm.peek_timer.saturating_sub(1);

                if sm.peek_timer == 0 {
                    // Return to pre-peek position
                    sm.target_x = sm.pre_peek_x;
                    sm.target_y = sm.pre_peek_y;
                    sm.speed = 6.0;
                    // Switch to Walk to move back, then it'll go Idle
                    sm.state = MascotState::Walk;
                    emit_state(&app, MascotState::Walk, None, None);
                    println!("[Mascot] Peek -> Walk | returning to=({}, {})", sm.target_x, sm.target_y);
                }
            } else {
                let ratio = sm.speed / dist;
                let new_x = pos.x + (dx as f64 * ratio) as i32;
                let new_y = pos.y + (dy as f64 * ratio) as i32;
                let _ = window.set_position(PhysicalPosition::new(new_x, new_y));
            }
        }

        MascotState::Disappear => {
            sm.disappear_timer = sm.disappear_timer.saturating_sub(1);
            if sm.disappear_timer == 0 {
                // Move to new random position while invisible
                let margin = 80;
                let window_size = 200;
                let (new_x, new_y) = if let Some(ref corner) = sm.config.fixed_corner {
                    pick_corner_position(corner, bounds, window_size, margin)
                } else {
                    pick_random_position(bounds, window_size, margin)
                };
                let _ = window.set_position(PhysicalPosition::new(new_x, new_y));

                transition_to(&mut sm, MascotState::Reappear, bounds);
                emit_state(&app, sm.state, None, None);
                println!("[Mascot] Disappear -> Reappear | new_pos=({}, {})", new_x, new_y);
            }
        }

        MascotState::Reappear => {
            sm.reappear_timer = sm.reappear_timer.saturating_sub(1);
            if sm.reappear_timer == 0 {
                transition_to(&mut sm, MascotState::Idle, bounds);
                emit_state(&app, sm.state, None, None);
                println!("[Mascot] Reappear -> Idle");
            }
        }

        MascotState::Interact => {
            sm.interact_timer = sm.interact_timer.saturating_sub(1);
            if sm.interact_timer == 0 {
                transition_to(&mut sm, MascotState::Idle, bounds);
                emit_state(&app, sm.state, None, None);
                println!("[Mascot] Interact -> Idle");
            }
        }

        MascotState::Chat => {
            // Frozen while chatting
            return;
        }
    }
}

// ── LLM Proxy ──────────────────────────────────────────────────────

use reqwest::Client;

async fn send_chat_message(message: &str, config: &LlmConfig) -> Result<String, String> {
    let client = Client::new();
    match config.provider {
        LlmProvider::Claude => call_claude(&client, message, config).await,
        LlmProvider::Openai => call_openai(&client, message, config).await,
        LlmProvider::OpenaiCompatible => call_openai_compatible(&client, message, config).await,
        LlmProvider::Gemini => call_gemini(&client, message, config).await,
        LlmProvider::Ollama => call_ollama(&client, message, config).await,
    }
}

#[derive(Serialize)]
struct ClaudeMsg {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ClaudeReq {
    model: String,
    max_tokens: u32,
    messages: Vec<ClaudeMsg>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

#[derive(Deserialize)]
struct ClaudeResp {
    content: Vec<ClaudeContent>,
}

async fn call_claude(client: &Client, message: &str, config: &LlmConfig) -> Result<String, String> {
    if config.api_key.is_empty() {
        return Err("API Key 未设置".into());
    }

    // 第三方代理通常使用 OpenAI 兼容接口
    if let Some(ref base) = config.base_url {
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        return call_openai_inner(client, message, config, &url).await;
    }

    let body = ClaudeReq {
        model: config.model.clone(),
        max_tokens: 1024,
        messages: vec![ClaudeMsg {
            role: "user".into(),
            content: message.into(),
        }],
    };
    let res = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("API 错误: {}", res.text().await.unwrap_or_default()));
    }
    let json: ClaudeResp = res.json().await.map_err(|e| e.to_string())?;
    json.content.into_iter().next().map(|c| c.text).ok_or_else(|| "空响应".into())
}

#[derive(Serialize, Deserialize)]
struct OpenAiMsg {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OpenAiReq {
    model: String,
    messages: Vec<OpenAiMsg>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMsg,
}

#[derive(Deserialize)]
struct OpenAiResp {
    choices: Vec<OpenAiChoice>,
}

async fn call_openai_inner(
    client: &Client,
    message: &str,
    config: &LlmConfig,
    url: &str,
) -> Result<String, String> {
    let body = OpenAiReq {
        model: config.model.clone(),
        messages: vec![OpenAiMsg {
            role: "user".into(),
            content: message.into(),
        }],
    };
    let mut req = client.post(url).json(&body);
    if !config.api_key.is_empty() {
        req = req.header("authorization", format!("Bearer {}", &config.api_key));
    }
    let res = req.send().await.map_err(|e| format!("请求失败: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("API 错误: {}", res.text().await.unwrap_or_default()));
    }
    let json: OpenAiResp = res.json().await.map_err(|e| e.to_string())?;
    json.choices.into_iter().next().map(|c| c.message.content).ok_or_else(|| "空响应".into())
}

async fn call_openai(client: &Client, message: &str, config: &LlmConfig) -> Result<String, String> {
    call_openai_inner(client, message, config, "https://api.openai.com/v1/chat/completions").await
}

async fn call_openai_compatible(client: &Client, message: &str, config: &LlmConfig) -> Result<String, String> {
    let base = config.base_url.as_deref().unwrap_or("http://localhost:8080/v1");
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    call_openai_inner(client, message, config, &url).await
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiReq {
    contents: Vec<GeminiContent>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
struct GeminiResp {
    candidates: Vec<GeminiCandidate>,
}

async fn call_gemini(client: &Client, message: &str, config: &LlmConfig) -> Result<String, String> {
    if config.api_key.is_empty() {
        return Err("API Key 未设置".into());
    }
    let model = if config.model.is_empty() { "gemini-1.5-flash" } else { &config.model };
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, &config.api_key
    );
    let body = GeminiReq {
        contents: vec![GeminiContent {
            parts: vec![GeminiPart { text: message.into() }],
        }],
    };
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("API 错误: {}", res.text().await.unwrap_or_default()));
    }
    let json: GeminiResp = res.json().await.map_err(|e| e.to_string())?;
    json.candidates
        .into_iter()
        .next()
        .and_then(|c| c.content.parts.into_iter().next())
        .map(|p| p.text)
        .ok_or_else(|| "空响应".into())
}

#[derive(Serialize)]
struct OllamaReq {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResp {
    response: String,
}

async fn call_ollama(client: &Client, message: &str, config: &LlmConfig) -> Result<String, String> {
    let base = config.base_url.as_deref().unwrap_or("http://localhost:11434");
    let url = format!("{}/api/generate", base.trim_end_matches('/'));
    let body = OllamaReq {
        model: config.model.clone(),
        prompt: message.into(),
        stream: false,
    };
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败: {}", e))?;
    if !res.status().is_success() {
        return Err(format!("API 错误: {}", res.text().await.unwrap_or_default()));
    }
    let json: OllamaResp = res.json().await.map_err(|e| e.to_string())?;
    Ok(json.response)
}

// ── Commands ───────────────────────────────────────────────────────

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn get_config() -> BehaviorConfig {
    load_or_create_config()
}

#[tauri::command]
fn set_config(
    config: BehaviorConfig,
    state_machine: State<StateMachine>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    println!("[set_config] called");
    let (old_dock, old_shortcut, old_auto_start) = {
        let sm = state_machine.lock().unwrap();
        (sm.config.show_in_dock, sm.config.chat_shortcut.clone(), sm.config.auto_start)
    };

    // Save API key to keyring
    if let Err(e) = set_api_key_in_keyring(&config.llm.api_key) {
        println!("[set_config] api_key save failed: {}", e);
        return Err(format!("API Key 保存失败: {}", e));
    }
    println!("[set_config] api_key saved to keyring");

    if let Err(e) = save_config(&config) {
        println!("[set_config] save_config failed: {}", e);
        return Err(e);
    }
    println!("[set_config] save_config ok");

    // Update running state machine
    {
        let mut sm = state_machine.lock().unwrap();
        let old_fixed = sm.config.fixed_corner.clone();
        sm.config = config.clone();

        // If fixed corner is newly set, immediately reset to Idle and move to corner
        if old_fixed.is_none() && config.fixed_corner.is_some() {
            if let Some(bounds) = get_primary_bounds(&app) {
                if let Some(ref corner) = config.fixed_corner {
                    let (tx, ty) = pick_corner_position(corner, bounds, 200, 80);
                    sm.target_x = tx;
                    sm.target_y = ty;
                    sm.state = MascotState::Idle;
                    sm.idle_ticks = 0;
                    println!("[set_config] Fixed corner activated, reset to ({}, {})", tx, ty);
                }
            }
        }
    }

    // Apply Dock visibility only when value changes (macOS only)
    if config.show_in_dock != old_dock {
        set_dock_visibility(config.show_in_dock);
    }

    // Re-register global shortcut if changed
    if config.chat_shortcut != old_shortcut {
        update_chat_shortcut(&app, &config.chat_shortcut);
    }

    // Apply auto-start if changed
    if config.auto_start != old_auto_start {
        let autostart_manager = app.autolaunch();
        if config.auto_start {
            if let Err(e) = autostart_manager.enable() {
                eprintln!("[Autostart] Failed to enable: {}", e);
            } else {
                println!("[Autostart] Enabled");
            }
        } else {
            let _ = autostart_manager.disable();
            println!("[Autostart] Disabled");
        }
    }

    Ok(())
}

#[tauri::command]
fn reset_config(
    state_machine: State<StateMachine>,
    app: tauri::AppHandle,
) -> Result<BehaviorConfig, String> {
    let default = BehaviorConfig::default();
    save_config(&default)?;

    let old_shortcut = {
        let mut sm = state_machine.lock().unwrap();
        let old_dock = sm.config.show_in_dock;
        let old_shortcut = sm.config.chat_shortcut.clone();
        sm.config = default.clone();
        if default.show_in_dock != old_dock {
            set_dock_visibility(default.show_in_dock);
        }
        old_shortcut
    };

    // Re-register global shortcut if changed
    if default.chat_shortcut != old_shortcut {
        update_chat_shortcut(&app, &default.chat_shortcut);
    }

    Ok(default)
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    open_settings_window_impl(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    open_chat_window_impl(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn close_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    close_chat_window_impl(&app).map_err(|e| e.to_string())
}

fn update_chat_shortcut(app: &tauri::AppHandle, shortcut_str: &str) {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    if shortcut_str.trim().is_empty() {
        return;
    }
    if let Err(e) = manager.on_shortcut(shortcut_str, move |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            let _ = open_chat_window_impl(app);
        }
    }) {
        eprintln!("[Shortcut] Failed to register '{}': {}", shortcut_str, e);
    } else {
        println!("[Shortcut] Registered '{}'", shortcut_str);
    }
}

#[tauri::command]
async fn chat_send(
    message: String,
    history: State<'_, ChatHistory>,
) -> Result<String, String> {
    history.push("user", message.clone());
    let config = load_or_create_llm_config();
    match send_chat_message(&message, &config).await {
        Ok(reply) => {
            history.push("ai", reply.clone());
            Ok(reply)
        }
        Err(e) => {
            history.push("error", e.clone());
            Err(e)
        }
    }
}

#[tauri::command]
fn get_chat_history(history: State<'_, ChatHistory>) -> Vec<ChatMessage> {
    history.get_all()
}

#[tauri::command]
fn clear_chat_history(history: State<'_, ChatHistory>) {
    history.clear();
}

#[tauri::command]
fn get_llm_config() -> LlmConfig {
    load_or_create_llm_config()
}

#[tauri::command]
fn set_llm_config(config: LlmConfig) -> Result<(), String> {
    save_llm_config(&config)
}

#[derive(Deserialize)]
struct SetApiKeyPayload {
    api_key: String,
}

#[tauri::command]
fn get_api_key() -> String {
    get_api_key_from_keyring()
}

#[tauri::command]
fn set_api_key(payload: SetApiKeyPayload) -> Result<(), String> {
    println!("[set_api_key] called, key length: {}", payload.api_key.len());
    let result = set_api_key_in_keyring(&payload.api_key);
    println!("[set_api_key] result: {:?}", result);
    result
}

// ── Entry point ────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));

            // Ignore mouse events on transparent areas of the mascot window
            let _ = window.set_ignore_cursor_events(true);

            // macOS: show window on all Spaces
            set_window_all_spaces(&window);

            let config = load_or_create_config();

            // Initial position: fixed corner if set, otherwise center
            let initial_pos = if let Some(ref corner) = config.fixed_corner {
                if let Some(bounds) = get_primary_bounds(app.handle()) {
                    let (x, y) = pick_corner_position(corner, bounds, 200, 80);
                    let _ = window.set_position(PhysicalPosition::new(x, y));
                    PhysicalPosition::new(x, y)
                } else {
                    PhysicalPosition::new(0, 0)
                }
            } else if let Some(monitor) = app.primary_monitor().unwrap_or(None) {
                let size = monitor.size();
                let pos = monitor.position();
                let x = pos.x + (size.width as i32 - 200) / 2;
                let y = pos.y + (size.height as i32 - 200) / 2;
                let _ = window.set_position(PhysicalPosition::new(x, y));
                PhysicalPosition::new(x, y)
            } else {
                PhysicalPosition::new(0, 0)
            };
            println!("[Config] {:?}", config);

            // Apply initial Dock visibility
            set_dock_visibility(config.show_in_dock);

            // Apply auto-start setting
            let autostart_manager = app.autolaunch();
            if config.auto_start {
                if let Err(e) = autostart_manager.enable() {
                    eprintln!("[Autostart] Failed to enable: {}", e);
                } else {
                    println!("[Autostart] Enabled");
                }
            } else {
                let _ = autostart_manager.disable();
            }

            // Create menu bar tray if enabled
            if config.show_in_menu_bar {
                if let Err(e) = create_tray(app.handle()) {
                    eprintln!("[Tray] Failed to create tray: {}", e);
                } else {
                    println!("[Tray] Menu bar icon created");
                }
            }

            let state_machine = Arc::new(Mutex::new(StateMachineInner {
                state: MascotState::Idle,
                target_x: initial_pos.x,
                target_y: initial_pos.y,
                speed: 0.0,
                idle_ticks: 0,
                peek_edge: PeekEdge::Right,
                peek_timer: 0,
                disappear_timer: 0,
                reappear_timer: 0,
                interact_type: InteractType::Wave,
                interact_timer: 0,
                config,
                pre_peek_x: 0,
                pre_peek_y: 0,
            }));

            // Register state machine as Tauri managed state
            app.manage(state_machine.clone());

            // Register chat history
            app.manage(ChatHistory::new());

            // Register global shortcut for chat
            let shortcut_str = state_machine.lock().unwrap().config.chat_shortcut.clone();
            update_chat_shortcut(app.handle(), &shortcut_str);

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
                loop {
                    interval.tick().await;
                    tick(state_machine.clone(), app_handle.clone()).await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
    greet, get_config, set_config, reset_config, open_settings_window,
    open_chat_window, close_chat_window, chat_send, get_chat_history, clear_chat_history,
    get_llm_config, set_llm_config, get_api_key, set_api_key
])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_roundtrip() {
        let config = BehaviorConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        println!("Serialized:\n{}", json);
        let deserialized: BehaviorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.llm.api_key, "");
    }

    #[test]
    fn test_config_deserialize_with_api_key() {
        let json = r#"{
            "idle_weight": 70,
            "walk_weight": 15,
            "peek_weight": 8,
            "disappear_weight": 5,
            "interact_weight": 2,
            "show_in_dock": false,
            "show_in_menu_bar": true,
            "fixed_corner": null,
            "llm": {
                "provider": "claude",
                "api_key": "secret123",
                "model": "claude-3-5-sonnet",
                "base_url": null
            },
            "auto_close_chat": true,
            "chat_shortcut": "Cmd+Shift+C",
            "auto_start": false
        }"#;
        let config: BehaviorConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.llm.api_key, "secret123"); // skip_serializing reads on deser
        assert_eq!(config.llm.model, "claude-3-5-sonnet");
        assert!(!config.auto_start);
    }

    #[test]
    fn test_keyring_roundtrip() {
        let test_key = "test_api_key_12345";
        set_api_key_in_keyring(test_key).expect("save to keyring failed");
        let retrieved = get_api_key_from_keyring();
        assert_eq!(retrieved, test_key);
        // cleanup
        let _ = set_api_key_in_keyring("");
    }

    #[test]
    fn test_save_and_load_config() {
        let mut config = BehaviorConfig::default();
        config.auto_start = true;
        config.llm.model = "test-model".to_string();
        save_config(&config).expect("save_config failed");
        let loaded = load_or_create_config();
        assert_eq!(loaded.auto_start, true);
        assert_eq!(loaded.llm.model, "test-model");
        // cleanup
        let _ = std::fs::remove_file(config_path());
    }
}
