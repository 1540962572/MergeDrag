# MergeDrag

IDEA 风格的三栏 Git 三方合并工具（mergetool），**Local | Result | Remote** + 箭头接受冲突。

打开即自动合并无冲突改动，只把真冲突标出来——和 IntelliJ IDEA 的解决冲突窗口一致的体验。完全免费开源（Apache-2.0）。

## 特性

- 三栏并排冲突视图，可显示 Base（四栏）
- 每处冲突：← 取左 / 取右 → / 双方 / ✕ 忽略
- 全部取左 / 全部取右、跳上一块 / 下一块（Alt+↑ / Alt+↓）
- Result 面板可手改，手改后该块以手改文本为准
- 自动合并无冲突部分，只标真冲突
- 左侧列出本次 merge 的全部冲突文件，点选切换；未解决数实时同步
- 全部解决后保存自动 `git add`；仍有未解决时保存弹确认，保留下冲突标记
- 保存后关窗，退出码按 mergetool 协议（`trustExitCode`）：已解决 → 0，否则 → 1
- 首次启动自动登记为全局 Git mergetool（`merge.tool = mergedrag`）；没装 Git 时给提示不崩

## 技术栈

| 层 | 技术 |
|----|------|
| UI 壳 | Tauri 2（Rust 后端 + WebView2） |
| 前端 | React 18 + TypeScript + Vite + Monaco Editor |
| 核心 | `merge-core`（纯 Rust 三方合并，diff3 语义） |
| Git | `git-bridge`（index stage 读取、`git add`、mergetool 登记） |

## 用法

```bash
git merge <分支>        # 产生冲突后
git mergetool
```

MergeDrag 会作为 `git mergetool` 打开，列出本次全部冲突文件并逐个解决。已登记过的系统（安装后首次启动自动登记）直接可用。

### 手动登记 mergetool

```bash
git config --global merge.tool mergedrag
git config --global mergetool.mergedrag.cmd '"<MergeDrag 完整路径.exe>" --local "$LOCAL" --remote "$REMOTE" --base "$BASE" --merged "$MERGED"'
git config --global mergetool.mergedrag.trustExitCode true
```

也可在应用里点「重新登记」（直接把当前运行路径写入全局配置）。

## 开发 / 构建

前置：Rust (MSVC toolchain) + VS Build Tools (C++ workload) + Node 20+。

```bash
npm install
npm run tauri dev        # 开发（本地 Monaco，不依赖 CDN）
npm run tauri build      # 产出 Windows NSIS 安装包
cargo test --workspace   # 后端单测（含真实冲突仓库 E2E）
```

fixtures 之外的说明见 [PLAN.md](PLAN.md)。

## 仓库

https://github.com/1540962572/MergeDrag