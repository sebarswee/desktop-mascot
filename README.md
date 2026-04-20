# 桌面宠物

> 一只住在屏幕上的 AI 小猫，陪你工作、陪你聊天。

![platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)
![tauri](https://img.shields.io/badge/Tauri-2.0-24C8DB)
![license](https://img.shields.io/badge/license-MIT-green)

---

## 这是什么？

一个跨平台的桌面 AI 宠物应用。一只小猫会随机在你的屏幕上走动、探头、玩消失，你可以点击它进行 AI 对话。支持 Claude、OpenAI、Gemini、Ollama 等多个模型，API Key 只在本地 Rust 层调用，不会暴露给前端。

## 功能

- **自主行为** — 待机、走动、探头、消失、出现、互动，6 种状态随机切换
- **AI 对话** — 点击宠物弹出对话窗口，支持多模型切换
- **全局快捷键** — 一键唤醒对话窗口（macOS 默认 `Cmd+Shift+C`，Windows/Linux 默认 `Ctrl+Alt+C`）
- **固定角落** — 可以让宠物固定在屏幕四角，不再乱走
- **菜单栏图标** — 通过系统托盘快速唤出设置、退出
- **配置持久化** — 行为权重、LLM 配置等自动保存在本地

## 截图

> 截图待补充

## 下载

前往 [Releases](https://github.com/sebarswee/desktop-mascot/releases) 页面下载对应平台的安装包。

## 开发

### 环境要求

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/) 1.70+

### 本地运行

```bash
# 安装依赖
npm install

# 开发模式（热重载）
npm run tauri dev

# 构建生产包
npm run tauri build
```

## 技术栈

| 层级 | 技术 |
|:---|:---|
| 前端框架 | Vanilla TypeScript + Vite |
| 桌面框架 | Tauri 2.0 (Rust) |
| 动画引擎 | lottie-web |
| HTTP 客户端 | reqwest |
| 全局快捷键 | tauri-plugin-global-shortcut |

## 项目结构

```
desktop-mascot/
├── src/                          # 前端源码
│   ├── main.ts                   # 主窗口：Lottie 渲染 + 状态监听
│   ├── chat.ts                   # 对话窗口：消息收发
│   ├── settings.ts               # 设置窗口：表单交互
│   ├── chat.css / settings.css   # 窗口样式
│   └── cat.json                  # 小猫 Lottie 动画数据
├── src-tauri/src/lib.rs          # Rust 后端：状态机 + 窗口控制 + LLM 代理
├── index.html / chat.html / settings.html  # 页面入口
└── design.md                     # 设计方案文档
```

## 开源协议

[MIT](LICENSE)
