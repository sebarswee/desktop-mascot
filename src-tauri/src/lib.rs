use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, PhysicalPosition, State};

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
}

fn default_true() -> bool {
    true
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
        }
    }
}

fn config_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("behavior.json")
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
    if let Ok(json) = serde_json::to_string_pretty(&default) {
        let _ = std::fs::write(&path, json);
        println!("[Config] Created default at {:?}", path);
    }
    default
}

fn save_config(config: &BehaviorConfig) -> Result<(), String> {
    let path = config_path();
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ── macOS Dock control ─────────────────────────────────────────────

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

#[cfg(not(target_os = "macos"))]
fn set_dock_visibility(_show: bool) {}

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

fn get_primary_bounds(app: &tauri::AppHandle) -> (i32, i32, i32, i32) {
    let monitor = app.primary_monitor().unwrap().unwrap();
    let size = monitor.size();
    let pos = monitor.position();
    (
        pos.x,
        pos.y,
        pos.x + size.width as i32,
        pos.y + size.height as i32,
    )
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

fn transition_to(
    sm: &mut StateMachineInner,
    new_state: MascotState,
    bounds: (i32, i32, i32, i32),
) {
    let (_, _, max_x, max_y) = bounds;
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
    }
    sm.state = new_state;
}

fn pick_next_state(sm: &StateMachineInner) -> MascotState {
    let mut rng = rand::thread_rng();
    let total = sm.config.idle_weight
        + sm.config.walk_weight
        + sm.config.peek_weight
        + sm.config.disappear_weight
        + sm.config.interact_weight;
    let r = rng.gen_range(0..total);

    let mut cum = 0;
    cum += sm.config.idle_weight;
    if r < cum { return MascotState::Idle; }
    cum += sm.config.walk_weight;
    if r < cum { return MascotState::Walk; }
    cum += sm.config.peek_weight;
    if r < cum { return MascotState::Peek; }
    cum += sm.config.disappear_weight;
    if r < cum { return MascotState::Disappear; }
    MascotState::Interact
}

async fn tick(state_machine: StateMachine, app: tauri::AppHandle) {
    let bounds = get_primary_bounds(&app);
    let mut sm = state_machine.lock().unwrap();
    let window = app.get_webview_window("main").unwrap();
    let pos = window.outer_position().unwrap();

    match sm.state {
        MascotState::Idle => {
            sm.idle_ticks += 1;
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
                let mut rng = rand::thread_rng();
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
    }
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
) -> Result<(), String> {
    let old_dock = {
        let sm = state_machine.lock().unwrap();
        sm.config.show_in_dock
    };

    save_config(&config)?;

    // Update running state machine
    {
        let mut sm = state_machine.lock().unwrap();
        sm.config = config.clone();
    }

    // Apply Dock visibility only when value changes (macOS only)
    if config.show_in_dock != old_dock {
        set_dock_visibility(config.show_in_dock);
    }

    Ok(())
}

#[tauri::command]
fn reset_config(
    state_machine: State<StateMachine>,
) -> Result<BehaviorConfig, String> {
    let default = BehaviorConfig::default();
    save_config(&default)?;

    {
        let mut sm = state_machine.lock().unwrap();
        let old_dock = sm.config.show_in_dock;
        sm.config = default.clone();
        if default.show_in_dock != old_dock {
            set_dock_visibility(default.show_in_dock);
        }
    }

    Ok(default)
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    open_settings_window_impl(&app).map_err(|e| e.to_string())
}

// ── Entry point ────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));

            // Center window initially
            let initial_pos = if let Some(monitor) = app.primary_monitor().unwrap() {
                let size = monitor.size();
                let pos = monitor.position();
                let x = pos.x + (size.width as i32 - 200) / 2;
                let y = pos.y + (size.height as i32 - 200) / 2;
                let _ = window.set_position(PhysicalPosition::new(x, y));
                PhysicalPosition::new(x, y)
            } else {
                PhysicalPosition::new(0, 0)
            };

            let config = load_or_create_config();
            println!("[Config] {:?}", config);

            // Apply initial Dock visibility
            set_dock_visibility(config.show_in_dock);

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
        .invoke_handler(tauri::generate_handler![greet, get_config, set_config, reset_config, open_settings_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
