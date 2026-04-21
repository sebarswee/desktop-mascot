# Desktop Mascot 设计方案

> 一个跨平台桌面 AI 助手（类似 Clippy / 瑞星小狮子），具备自主行为、多模型 AI 交互入口能力。

---

## 1. 顶层架构

```
┌─────────────────────────────────────────────────────────────┐
│  表现层（Web Frontend）                                      │
│  · Lottie 动画渲染引擎（lottie-web）                         │
│  · 独立设置窗口（settings.html / settings.ts）               │
│  · 对话气泡 / 交互面板（Sprint 3）                           │
├─────────────────────────────────────────────────────────────┤
  ↑ ↓ 事件通信（Tauri Event System: mascot:state）
├─────────────────────────────────────────────────────────────┤
│  控制层（Tauri Core / Rust）                                 │
│  · 窗口管理（位置、尺寸、层级、透明、穿透）                  │
│  · 行为状态机调度器（100ms tick，7 状态）                    │
│  · 系统集成（Dock 显隐、菜单栏 Tray、全局快捷键、配置持久化）│
│  · LLM API 代理（reqwest，5 提供商）                         │
├─────────────────────────────────────────────────────────────┤
  ↑ ↓ 异步调用
├─────────────────────────────────────────────────────────────┤
│  服务层（外部 / 本地）                                       │
│  · LLM API 代理（支持多模型切换：Claude / OpenAI / 自定义）  │
│  · TTS / STT 语音通道（Web Speech API / Whisper）            │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. 跨平台基座：Tauri 2.0

### 2.1 选型理由

| 维度 | Electron | Tauri 2.0 | 结论 |
| :--- | :--- | :--- | :--- |
| **包体积** | ~150MB | ~5MB | Tauri 完胜，用户下载无负担 |
| **内存占用** | 200MB+ | 30-50MB | 桌面宠物长期后台运行，内存是硬指标 |
| **透明窗口** | 成熟 | 已原生支持 | 两者都能做 |
| **窗口移动 API** | `setPosition` | `setPosition` + `WindowBuilder` | Tauri 2.0 通过 Rust 原生调用更灵活 |
| **鼠标穿透** | `setIgnoreMouseEvents` | `set_ignore_cursor_events` | 两者都支持 |
| **系统托盘/快捷键** | 需额外库 | 内置支持 | Tauri 更干净 |

### 2.2 核心能力验证清单

- [x] `WindowBuilder` 创建 `transparent: true`、`decorations: false`、`always_on_top: true` 的异形窗口
- [x] `window.set_background_color(Color(0,0,0,0))` + `shadow: false` 消除透明黑边
- [x] `Monitor::all()` 获取屏幕可用工作区，作为角色活动边界
- [x] `window.set_position(PhysicalPosition { x, y })` 实时更新窗口坐标
- [x] `tauri-plugin-global-shortcut` 全局快捷键注册/动态重载
- [ ] `window.set_ignore_cursor_events(true)` 实现非角色区域鼠标穿透（待优化）

---

## 3. 动画系统：Lottie 矢量动画

### 3.1 方案对比

| 方案 | 适合场景 | 优势 | 劣势 | 采用状态 |
| :--- | :--- | :--- | :--- | :--- |
| **Lottie (JSON)** | 矢量 MG 动画 | 文件极小、可编程控制 | 复杂骨骼动画表现力弱 | **已采用** |
| **Spine 2D** | 游戏级角色 | 骨骼绑定、动作混合 | 授权限制、编辑器付费 | 远期可选 |
| **帧动画 (SpriteSheet)** | 像素风 | 实现简单 | 文件大、切换生硬 | 不推荐 |
| **Live2D Cubism** | Vtuber 级表情 | 眼神追踪、口型同步 | 包体积大、授权复杂 | 远期可选 |
| **纯 CSS/SVG** | 占位角色 | 零依赖、快速验证 | 表现力有限 | MVP 已弃用 |

### 3.2 动画状态与速度映射

```
角色主体 (200x200 透明窗口)
├── idle      → 待机循环（Lottie 速度 1.0x）
├── walk      → 移动循环（Lottie 速度 1.5x）
├── peek      → 探头（从屏幕边缘探入/缩回）
├── disappear → 消失（Lottie 速度 0.5x，CSS opacity + scale）
├── reappear  → 出现（CSS opacity + scale 动画）
├── interact  → 交互动作（招手/跳跃/转圈，Lottie 速度 2.0x）
└── chat      → 对话冻结（Lottie 速度 1.0x，窗口位置冻结）
```

前端通过监听 `mascot:state` 事件切换 CSS class，同步调整 Lottie 播放速度。

---

## 4. 行为状态机（核心设计）

### 4.1 状态定义

```rust
enum MascotState {
    Idle,       // 待机：定时评估状态转移
    Walk,       // 行走：匀速移动到目标坐标
    Peek,       // 探头：从屏幕边缘外移动到边缘内，停留后缩回
    Disappear,  // 消失：淡出为 0，期间窗口隐藏
    Reappear,   // 出现：从随机位置淡入
    Interact,   // 交互：播放一次性花哨动作
    Chat,       // 对话：冻结自动行为，等待用户关闭对话框
}
```

### 4.2 状态转移规则（带权重随机）

```
IDLE ──(70%)──→ IDLE        （继续待机）
IDLE ──(15%)──→ WALK        （决定走到新位置）
IDLE ──(8% )──→ PEEK        （决定去屏幕边缘探头）
IDLE ──(5% )──→ DISAPPEAR   （决定玩消失）
IDLE ──(2% )──→ INTERACT    （随机做一个花哨动作）

WALK       ──(100%)──→ IDLE  （到达目标；若贴近边缘 30% 概率转为 PEEK）
PEEK       ──(100%)──→ WALK  （探头结束后返回原位）
DISAPPEAR  ──(100%)──→ REAPPEAR（1.5s 后在新位置出现）
REAPPEAR   ──(100%)──→ IDLE  （1.5s 淡入完成后）
INTERACT   ──(100%)──→ IDLE  （2-4s 动作完成后）
```

权重通过 `behavior.json` 暴露，用户可在设置面板实时调整。

### 4.3 行为参数池（随机化，避免机械感）

| 状态 | 随机参数 |
| :--- | :--- |
| `WALK` | 目标坐标（限定在屏幕内）、移动速度（4-10 px/tick）、固定角落模式下带 30px jitter |
| `PEEK` | 边缘（上/下/左/右）、探头深度（40-80px）、停留时长（3-5s） |
| `DISAPPEAR` | 淡出时长 1.5s（固定） |
| `REAPPEAR` | 出现位置（随机坐标或固定角落）、淡入时长 1.5s |
| `INTERACT` | 动作类型（招手/转圈/跳跃）、时长（2-4s） |

### 4.4 边界处理

- **屏幕边缘**：Rust 层通过 `primary_monitor()` 获取屏幕 bounds，角色坐标严格限制在矩形内
- **任务栏避让**：使用 `margin = 80px` 安全边距
- **边缘反弹**：`WALK` 到达目标时，若贴近边缘（threshold = 80px），有 30% 概率触发 `PEEK`

---

## 5. 窗口控制：Rust 层职责

### 5.1 前后端分工

| 层级 | 职责 |
| :--- | :--- |
| **前端 (Web)** | 渲染 Lottie 动画、监听状态事件、设置面板表单交互 |
| **Rust (后端)** | 状态机主循环、直接调用 `window.set_position()`、窗口属性管理、配置持久化 |

### 5.2 为什么状态机放在 Rust

1. **原子性**：状态变化直接驱动窗口位置变更，减少前后端通信延迟
2. **稳定性**：前端崩溃/卡顿时，Rust 仍能维持窗口基础行为
3. **扩展性**：系统托盘、Dock 控制、开机自启都依赖 Rust 层

### 5.3 Rust 核心模块（当前单文件 lib.rs，后续按需拆分）

| 模块 | 职责 | 对应代码区域 |
| :--- | :--- | :--- |
| `config` | `BehaviorConfig` / `LlmConfig` 定义、JSON 持久化 | `lib.rs:35-140` |
| `dock_control` | macOS Dock 显隐（objc 调用） | `lib.rs:142-220` |
| `tray` | 菜单栏图标 + 右键菜单 | `lib.rs:280-310` |
| `state_machine` | 状态定义、转移、tick 循环 | `lib.rs:380-700` |
| `llm_proxy` | 5 提供商 LLM 调用（reqwest） | `lib.rs:710-960` |
| `commands` | Tauri 暴露的前端调用接口 | `lib.rs:960-1115` |

---

## 6. 配置系统

### 6.1 配置项

```json
{
  "idle_weight": 70,
  "walk_weight": 15,
  "peek_weight": 8,
  "disappear_weight": 5,
  "interact_weight": 2,
  "show_in_dock": false,
  "show_in_menu_bar": true,
  "fixed_corner": null,
  "auto_close_chat": true,
  "chat_shortcut": "Ctrl+Alt+C",
  "llm": {
    "provider": "claude",
    "api_key": "",
    "model": "claude-3-5-sonnet-20241022",
    "base_url": null
  }
}
```

### 6.2 配置交互设计

- **批量保存**：设置面板内所有修改仅本地生效，点击「保存」后一次性写入 `behavior.json` 并通知状态机热更新
- **恢复默认**：一键重置为 `BehaviorConfig::default()`，立即生效
- **Dock 显隐优化**：仅在 `show_in_dock` 值变化时才调用 `setActivationPolicy`，避免不必要的系统 API 调用

---

## 7. AI 交互入口：多模型支持（Sprint 3）

### 7.1 设计原则

**API Key 绝不暴露给前端**，所有 LLM 调用由 Rust `reqwest` 代理完成。用户可在设置面板中自主选择模型提供商并填写 Key。

### 7.2 支持模型清单

| 提供商 | 标识符 | 说明 |
| :--- | :--- | :--- |
| Anthropic Claude | `claude` | 默认推荐，通过 Messages API 调用 |
| OpenAI | `openai` | GPT-4o / GPT-4o-mini |
| OpenAI Compatible | `openai-compatible` | 支持任意兼容 OpenAI 接口格式的端点 |
| Google Gemini | `gemini` | 通过 Gemini API 调用 |
| Local / Ollama | `ollama` | 本地模型，无需外网 |

### 7.3 交互闭环

```
用户点击角色身体
        ↓
前端进入 TALKING 状态 + 展示对话气泡
        ↓
用户输入文字 / 点击麦克风语音输入
        ↓
语音 → STT（Whisper / Web Speech API）→ 文字
        ↓
文字发送到 Rust LLM Proxy
        ↓
Rust 根据用户选择的模型配置调用对应 API
        ↓
收到 AI 回复 → 推送到前端展示 + TTS 播放
        ↓
角色配合 TTS 播放 talk 动画
        ↓
对话结束 → 自动回到 IDLE
```

---

## 8. 开发路线图

### Sprint 1：透明窗口 + 基础行为 ✅

- [x] Tauri 2.0 项目初始化
- [x] 透明、无边框、置顶窗口（`macos-private-api` + `shadow: false`）
- [x] CSS 占位角色（呼吸动画）
- [x] Rust 状态机：`IDLE → WALK → IDLE`
- [x] 窗口在屏幕上随机移动
- [x] Vite 多页面配置（`main` + `settings`）

### Sprint 2：丰富动作 + 配置系统 ✅

- [x] 新增 `PEEK`、`DISAPPEAR/REAPPEAR`、`INTERACT` 状态
- [x] 屏幕边界检测 + 安全边距避让
- [x] 行为权重配置文件（`behavior.json`）
- [x] Lottie 动画接入，替换占位角色
- [x] 独立设置窗口（280x420，不可调整大小）
- [x] 设置面板：行为权重、Dock/菜单栏显隐、固定角落
- [x] 批量保存 + 恢复默认
- [x] macOS Dock 显隐控制（objc）
- [x] 菜单栏 Tray 图标 + 右键菜单

### Sprint 3：AI 对话入口 ✅

- [x] 独立对话窗口（500x600，居中显示，无边框透明）
- [x] 暗色主题气泡 UI（用户粉色 / AI 深灰 / 错误红色）
- [x] Rust LLM Proxy（reqwest，5 提供商：Claude / OpenAI / OpenAI-Compatible / Gemini / Ollama）
- [x] 点击角色触发对话，对话期间宠物冻结（`Chat` 状态）
- [x] 关闭对话后自动恢复 `Idle`
- [x] 设置面板：LLM 提供商 / API Key / 模型 / Base URL
- [x] 点击外部自动关闭（`auto_close_chat`，可开关）
- [x] 全局快捷键唤醒（`tauri-plugin-global-shortcut`，macOS 默认 `Cmd+Shift+C`，Win/Linux 默认 `Ctrl+Alt+C`）
- [x] 设置面板快捷键**按键录制**（focus → 监听 keydown → 自动格式化）
- [x] 修复无响应：消除 `Mutex` 锁竞争，命令函数无锁化，状态由 `tick()` 根据窗口可见性自动同步
- [x] 移除主窗口右上角设置按钮，减少遮挡（设置入口保留在 Tray 右键菜单）
- [x] Claude 第三方代理支持（配置 Base URL，自动降级为 OpenAI-compatible 格式调用）
- [x] 编译器 warning 清零（`#[allow(unexpected_cfgs)]` + 未使用变量前缀 `_`）

### Sprint 4： polish + 跨平台 ✅

- [x] 配置文件路径标准化（`dirs::config_dir()`，跨平台存储）
- [x] 设置项加密存储（API Key 通过 `keyring` 写入 OS 密钥链，`#[serde(skip_serializing)]` 避免明文落盘）
- [x] 开机自启（`tauri-plugin-autostart`，设置面板增加「开机自启」开关）
- [x] Linux CI 构建（GitHub Actions 新增 `ubuntu-latest`，安装 `libgtk-3-dev` 等依赖）
- [x] 鼠标穿透（主窗口 `set_ignore_cursor_events(true)`，透明区域不再阻挡鼠标）
- [x] 固定角落即时生效（`set_config` 检测到 `fixed_corner` 从 None 变为 Some 时立即重置状态机）
- [ ] Windows 适配（Dock 对应实现）
- [ ] Linux 适配（运行时适配）

---

## 9. 目录结构

```
desktop-mascot/
├── src-tauri/
│   ├── src/
│   │   └── lib.rs              # Rust 后端：状态机 + 窗口控制 + 配置 + Tray
│   ├── icons/
│   │   └── 32x32.png           # 菜单栏图标
│   ├── Cargo.toml
│   └── tauri.conf.json         # 窗口配置（200x200, transparent, alwaysOnTop）
├── src/
│   ├── main.ts                 # 主窗口：Lottie 渲染 + 状态监听 + 点击打开对话
│   ├── chat.ts                 # 对话窗口：消息收发、加载态、自动关闭配置
│   ├── chat.css                # 对话面板暗色气泡样式
│   ├── settings.ts             # 设置窗口：表单批量保存/恢复默认
│   ├── settings.css            # 设置面板暗色主题样式
│   ├── styles.css              # 主窗口样式（mascot 定位 + 状态动画）
│   └── cat.json                # Lottie 动画数据
├── index.html                  # 主窗口入口
├── chat.html                   # 对话窗口入口
├── settings.html               # 设置窗口入口
├── vite.config.ts              # Vite 多页面 Rollup 配置
├── package.json
├── design.md                   # 本文件
└── README.md
```

---

## 10. 已确认的关键决策

1. **角色美术**：已采用 **Lottie 矢量动画**（`cat.json`），通过 `lottie-web` 渲染，按状态调整播放速度
2. **AI 接入**：支持**多模型自主选择**（Claude / OpenAI / OpenAI-Compatible / Gemini / Ollama），API Key 由 Rust 层安全代理
3. **设置窗口独立化**：设置面板不再作为 overlay 嵌入主窗口，而是通过 `WebviewWindowBuilder` 创建独立窗口（280x420），避免随宠物移动
4. **配置热更新**：`set_config` 命令直接更新运行中状态机的 `config` 字段，无需重启应用
5. **macOS 私有 API**：启用 `macos-private-api` 实现真透明窗口，这是 Tauri 2.0 在 macOS 上实现无黑边透明的必要手段
6. **全局快捷键跨平台差异**：macOS 默认 `Cmd+Shift+C`，Windows/Linux 默认 `Ctrl+Alt+C`，通过 `#[cfg(target_os = "macos")]` 在编译期确定默认值
7. **无响应根因与修复**：`std::sync::Mutex` 锁竞争导致 UI 线程阻塞。根本解法是命令函数彻底不拿锁，仅做窗口操作；状态同步完全委托给 `tick()` 循环根据窗口可见性自动推断
8. **快捷键录制 UX**：设置面板中快捷键输入框采用 `focus → keydown 监听 → 自动格式化` 的交互模式，支持 `Escape` 取消、`Backspace` 清空，避免用户手动输入格式错误
