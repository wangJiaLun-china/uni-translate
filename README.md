# uni-translate

一个轻量的 AI 翻译桌面应用。只需选择目标语言，源语言自动识别；对接任何 **OpenAI 兼容接口**，DeepSeek、通义千问、智谱、Kimi、豆包、Ollama 等都能用。

A lightweight AI translator for any OpenAI-compatible API, built with Tauri 2 (Rust) and Vue 3.

## 功能

- **小卡片**：平时只有一张小卡片。输入原文后按回车，就会按设置好的目标语言显示译文；卡片高度随内容伸缩。原文旁边有播放按钮可以朗读，译文可以一键复制。卡片没有系统标题栏：按住空白处拖动，点右上角的 × 或按 Esc 关闭
- **设置与历史**：点卡片底部的图标，会在单独的大窗口中打开设置或历史记录
- **流式翻译**：译文逐字显示，可以随时停止
- **自动识别源语言**：原文已经是目标语言时，自动改译成「第二目标语言」（默认中英互译）
- **划词翻译**：在任意程序中选中文字，按全局快捷键（默认 `Alt+D`），光标旁会弹出同样的翻译卡片
- **语音朗读**：通过 OpenAI 兼容的 `/audio/speech` 接口朗读，长文本自动分段
- **翻译历史**：本地 SQLite 保存最近 1000 条，可搜索、复制、朗读、删除
- **托盘常驻**：关闭卡片只是隐藏，点托盘图标可以重新打开，从托盘菜单退出
- API Key 保存在配置目录下仅本人可读的 `secrets.json`，不写入系统钥匙串

## 配置示例

在「设置 → 翻译接口」中填写接口地址、API Key 和模型名称。以下地址仅供参考，请以各家官方文档为准：

| 服务 | 接口地址（Base URL） | 模型示例 |
|---|---|---|
| DeepSeek | `https://api.deepseek.com/v1` | `deepseek-chat` |
| 通义千问 | `https://dashscope.aliyuncs.com/compatible-mode/v1` | `qwen-plus` |
| 智谱 GLM | `https://open.bigmodel.cn/api/paas/v4` | `glm-4-flash` |
| Kimi | `https://api.moonshot.cn/v1` | `moonshot-v1-8k` |
| 豆包（火山方舟） | `https://ark.cn-beijing.volces.com/api/v3` | 模型名或接入点 ID |
| Ollama（本地） | `http://localhost:11434/v1` | `qwen2.5:7b` |

点「获取模型列表」可以读取接口的 `/models`（部分服务不支持，手动填写即可），点「测试连接」会发一条短翻译来验证配置。

### 语音朗读

在「设置 → 语音朗读」中选择语音服务：

- **OpenAI 兼容接口**：使用 `POST {Base URL}/audio/speech`，请求格式与 OpenAI 相同（`model`、`input`、`voice`、`speed`），返回 MP3。默认复用翻译接口的地址和 Key；如果你的翻译服务不提供语音接口，可以取消「复用翻译接口」，单独填写支持该接口的服务。
- **百度语音**：使用百度智能云的[短文本在线合成](https://cloud.baidu.com/doc/SPEECH/s/mlbxh7xie)。在[百度智能云控制台](https://console.bce.baidu.com/ai-engine/speech/overview/)创建应用，把 API Key 和 Secret Key 填进设置页即可（设置页里也有直达链接）。音色可以选常用的几种，也可以直接填写音色 ID；精品音色需要先在控制台开通。

用真实的百度 Key 跑一次联调测试：

```bash
cd src-tauri
BAIDU_API_KEY=… BAIDU_SECRET_KEY=… cargo test live_baidu -- --ignored
```

## 平台说明

- **macOS**：以菜单栏应用运行，不显示 Dock 图标，翻译卡片、设置和历史都从菜单栏图标打开；这样划词卡片会出现在当前桌面，而不是把你切回主卡片所在的桌面。划词翻译需要模拟 `⌘C` 读取选中文字，首次使用请在「系统设置 → 隐私与安全性 → 辅助功能」中允许 uni-translate。
- **Windows**：划词翻译通过模拟 `Ctrl+C` 实现，读取后会恢复原来的剪贴板文字。
- **Linux**：直接读取 X11 的 PRIMARY 选区，不需要模拟按键。Wayland 下全局快捷键和选区读取受合成器限制，可能无法使用。朗读需要系统装有 GStreamer 的 MP3 解码插件（如 `gstreamer1.0-plugins-good`）。

## 开发

需要 [Rust](https://rustup.rs/)（1.82+）、Node.js 20+，以及 [Tauri 的系统依赖](https://v2.tauri.app/start/prerequisites/)。Debian/Ubuntu 上：

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev
```

```bash
npm install
npm run tauri dev     # 开发模式
npm run tauri build   # 打包安装程序
```

检查与测试：

```bash
npm run build                        # vue-tsc 类型检查 + Vite 打包
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 项目结构

```
src/                    Vue 3 前端
  components/TranslateCard.vue   翻译卡片（主窗口和划词弹窗共用）
  views/                卡片、划词弹窗、历史、设置
  composables/          流式翻译、语音播放、窗口高度自适应、提示
  api/commands.ts       调用 Rust 命令的类型化封装
src-tauri/src/
  client.rs             OpenAI 兼容客户端：流式 chat、/models、/audio/speech
  translate.rs          提示词、翻译与取消命令
  lang.rs               支持的语言、源语言检测
  tts.rs                朗读命令、长文本分段
  history.rs            SQLite 翻译历史
  config.rs secrets.rs  设置（JSON）与 API Key（secrets.json）
  selection.rs          全局快捷键、读取选中文字、弹窗定位
  windows.rs            设置/历史窗口、托盘菜单
```
