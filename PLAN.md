# MergeDrag - 项目计划文档

**版本**: v0.2 (MVP - Windows 客户端先行)
**日期**: 2026-08-13
**项目目标**: 实现类似于 IntelliJ IDEA 的三栏 + 箭头三方合并 Git 冲突解决体验。

## 1. 项目概述

### 项目名称
- **中文**: MergeDrag - IDEA 风格三方合并工具
- **英文**: MergeDrag

### 项目描述
开箱即用的 Git mergetool。核心体验对齐 IntelliJ IDEA：

- 三栏：Local | Result | Remote
- 点箭头接受左侧 / 右侧 / 双方
- 打开时先自动合并无冲突改动，只把真冲突标出来
- 左侧列出本次 merge 的全部冲突文件
- 显示 Base、跳到下一处冲突、忽略此块
- 安装时自动登记为全局 Git mergetool
- 完全免费开源（Apache-2.0）

**当前阶段**: 先做 **Windows x64 NSIS `.exe` 安装包**，后续同一套代码打 macOS `.dmg`。

## 2. 已确认决策（2026-08-13）

| 决策 | 选择 |
|------|------|
| UI 壳 | Tauri 2 + 网页前端（不用 Electron） |
| 语言 | Rust 核心 + TypeScript 前端 |
| 编辑器 | Monaco |
| 主交互 | 完全按 IDEA：三栏 + 箭头，拖拽不做进 MVP |
| 启动方式 | Git mergetool（`$LOCAL $REMOTE $BASE $MERGED`） |
| 冲突操作 | 接受左/右/双方、全部接受左/右、显示 Base、跳下一块、忽略此块 |
| 打开文件 | 先自动合并无冲突部分 |
| 多文件 | 打开时扫描仓库，左侧列出全部未合并文件 |
| 安装包 | Windows NSIS `.exe`（MVP）；Mac `.dmg` 后补 |
| mergetool 登记 | 安装时自动写全局 git config；没装 Git 则提示，安装不失败 |
| 合并算法 | Rust `merge-core` 自己做三方合并，不调 `git merge-file` 当主路径 |
| 前端框架 | React 18 + TypeScript + Tailwind |
| 许可 | Apache-2.0（仓库已有 LICENSE） |

## 3. MVP 需求规格（Windows 客户端）

### 核心功能
1. **三栏冲突解析**
   - Local | Result | Remote
   - gutter 箭头：接受左侧 / 右侧
   - 接受双方（默认 Local 在前）
   - 全部接受左侧 / 右侧
   - 忽略此块（该块不写入 Result）
   - 显示 / 隐藏 Base
   - 跳到下一个 / 上一个未解决冲突
   - 中间栏可手改；手改后该块变为 Manual，可用「重置此块」恢复

2. **打开即自动合并**
   - 读 Local / Remote / Base
   - 无冲突行收成 Clean hunk，直接写入 Result
   - 只把真冲突标成 Conflict { Unresolved }

3. **文件列表**
   - 由 `$MERGED` 向上找 `.git`，扫描全部未合并文件
   - 显示路径、未解决数、状态（Unresolved / Resolved / Binary）
   - 点文件切换；未保存先确认

4. **保存与退出**
   - 仍有 Unresolved 时保存要确认；确认后写入 `$MERGED` 但不 `git add`
   - 全部解决后写入 `$MERGED` 并 `git add`
   - 全部解决后关窗口退出码 `0`；否则 `1`
   - `mergetool.mergedrag.trustExitCode true`

5. **Git mergetool 协议**
   ```
   MergeDrag.exe --local "$LOCAL" --remote "$REMOTE" --base "$BASE" --merged "$MERGED"
   ```
   安装时写入：
   ```
   git config --global merge.tool mergedrag
   git config --global mergetool.mergedrag.cmd "..."
   git config --global mergetool.mergedrag.trustExitCode true
   ```

### 非 MVP（后续迭代）
- macOS `.dmg` / Apple 公证
- 拖拽冲突块
- 批量处理、历史记录
- 主题切换（先做深色，浅色后补）
- 完整 i18n（先中英字符串写死，再抽）
- Windows ARM / 便携版
- 非 UTF-8 完整编码支持
- 二进制冲突解决

### 错误处理（MVP）
- 无 Base：回退双边合并，打开时不自动合并无冲突行
- 非 UTF-8：lossy 读，提示可能乱码
- 二进制：列表标 Binary，点开只提示，不进三栏
- 大文件（> 10 MB）：关 minimap / 行号，按 hunk 虚拟化
- 找不到 Git / 仓库：提示，并提供「打开示例冲突」
- 仍有 Unresolved 就保存 / 退出：弹确认

## 4. 技术选型

```
Frontend:
- Tauri 2
- React 18 + TypeScript
- Vite
- Monaco Editor
- Tailwind CSS

Rust:
- merge-core      纯库：三方合并，零 Tauri 依赖
- git-bridge      扫未合并文件、写 mergetool 配置、git add
- src-tauri       Tauri command 薄封装 + CLI 解析

Packaging:
- Windows: NSIS .exe（x64）
- macOS: .dmg（后补）
```

不选 Electron（体积）。不选纯原生 GUI（没有 Monaco）。不把 `git merge-file` 当主路径（要结构化 hunk）。

## 5. 开发架构

```
MergeDrag/
├── PLAN.md
├── LICENSE
├── package.json
├── vite.config.ts
├── src/                         # React
│   ├── main.tsx
│   ├── App.tsx
│   ├── features/
│   │   ├── file-list/
│   │   ├── three-pane/
│   │   └── conflict-nav/
│   └── shared/                  # 与 Rust 对齐的 TS 类型
├── src-tauri/
│   ├── Cargo.toml               # workspace
│   ├── tauri.conf.json
│   ├── crates/
│   │   ├── merge-core/
│   │   └── git-bridge/
│   └── src/
│       ├── lib.rs
│       ├── commands.rs
│       └── launch.rs
└── fixtures/                    # 开发期示例冲突
```

### 关键类型

```rust
struct LaunchArgs {
    local: PathBuf,
    remote: PathBuf,
    base: Option<PathBuf>,
    merged: PathBuf,
    repo_root: PathBuf,
}

enum Side { Local, Remote, Base }

struct LineRange { start: u32, end: u32 } // 半开区间，0-based

enum Hunk {
    Clean { text: String },
    Conflict {
        id: HunkId,
        local: String,
        remote: String,
        base: Option<String>,
        decision: Decision,
    },
}

enum Decision {
    Unresolved,
    TakeLocal,
    TakeRemote,
    TakeBoth { local_first: bool },
    Ignore,
    Manual(String),
}

struct MergeDocument {
    file: PathBuf,
    hunks: Vec<Hunk>,
    encoding: Encoding, // MVP: Utf8 | Utf8Lossy
}

struct ConflictFile {
    path: PathBuf,
    status: FileStatus, // Unresolved | Resolved | Binary
}
```

### 数据流
1. `git mergetool` 传入四个路径
2. `open_session`：找 repo → 列未合并文件 → 对当前文件 `three_way`
3. 前端渲染三栏 + 文件列表
4. `set_decision` 更新 hunk，返回新 Result 文本
5. `save`：`apply(doc)` 写 `$MERGED`；无 Unresolved 则 `git add`
6. 退出码告诉 Git 成败

无 CLI 参数时打开空壳，菜单「打开示例冲突」加载 `fixtures/`。

## 6. 开发阶段

### Phase 1: 环境与 merge-core（基本完成，待 E2E 验证）
- 初始化 Tauri 2 + React + TypeScript + Monaco（✓ 编译、构建全绿）
- `merge-core`：三方合并 + 单测（fixture）（✓ 20/20 测试通过）
- 最小 CLI：四个路径 → 打印 hunk 统计（由应用内 `LaunchArgs` 承担，clap 解析 `--local/--remote/--base/--merged`）
- 开发期可加载示例冲突（✓ `loadSample` 内建示例）
- 环境坑：Rust 不在 PATH、需 VS Build Tools（vcvars64）+ Windows SDK、git-bash 下用 `scripts/cargo-msvc.bat` 跑 cargo
- 端到端验证（✓ 见 Phase 4）：真实冲突仓库下 `git-bridge` 扫未合并 → `three_way` 打开 → 决策 → 写盘 + `git add` → 退出码

### Phase 2: 三栏 UI 与决策（已完成）
- 三栏并排（Local | Result | Remote），可切「显示 Base」四栏（✓）
- 工具栏：上一块/下一块（Alt+↑/↓）、块计数、全部取左/全部取右（✓）
- 冲突块 Monaco 行高亮（未解决红色、活动块加框）+ 定位滚动（✓）
- 每块操作条：← 取左 / 取右 → / 双方 / ✕ 忽略（✓）
- Result 可手改（manualOverride：手改后以手改文本为准，点决策即恢复 hunk 模型）（✓）
- 实测：本地 Monaco 打包（除 CDN）、serde tag 用 PascalCase + localFirst 字段重命名（契约测试锁定）（✓）
- 多文件左栏切换（✓）、保存按钮 + Ctrl+S（✓，浏览器模式给出提示）
- 决策后左栏「未解决 n / 已解决」徽标同步（✓）

### Phase 3: Git 会话与多文件（完成）
- 解析 mergetool CLI（✓ clap）
- 扫描未合并文件列表（✓ scan_repo / open_file：git2 index stage 1/2/3 + three_way；含真实冲突仓库单测）
- 保存 / 退出码（✓ persist + exit_code_for：已保存且 0 未解决 → 0，否则 → 1）
- 未解决时保存弹确认（✓ window.confirm，确认后写盘但不 git add）
- 全部解决后写入并 git add（✓ git-bridge::stage_file，单测验证文件脱离 unmerged、进 staged）
- native 冒烟（✓ debug exe 带完整 mergetool 参数启动真实冲突仓库，进程稳定不崩）

### Phase 4: 安装包与登记（完成，2026-08-14）
- Windows NSIS `.exe` 安装包（✓ `tauri build`，per-user 装到 `%LOCALAPPDATA%\MergeDrag`；NSIS 工具链国内网络超时的离线预处理见 memory）
- 安装时登记 mergetool：首次启动自动写全局 `merge.tool` / `mergetool.mergedrag.cmd` / `trustExitCode`（✓ CDP + `git config --global --get` 双重验证），`merge.tool` 被改走可点「重新登记」（✓）
- 没装 Git 时提示、应用不崩（✓ 空壳 + 示例冲突）
- README + 手动登记说明（✓ `README.md`）
- **真实 `git mergetool` 全流程 E2E（✓ 2026-08-14，已安装产物）**：
  - resolved：取右 → 「已保存并已 git add」→ 关窗退出码 0 → `git mergetool` rc=0，文件脱离 unmerged、进 staged
  - unresolved：仍含标记保存、不 git add → 退出码 1（trustExitCode 逐文件中止）
  - **两个 Windows 专属修复**：① `app.exit(code)` 不传播退出码 → CloseRequested 里 `std::process::exit(code)`；
    ② git 传 `$MERGED`/`$LOCAL` 为**相对路径**（相对进程 CWD）→ 启动时 `canonicalize`（并剥 `\\?\`），
    否则自带的「全部解决后 git add」静默跳过（`strip_prefix` 相对 vs 绝对失配），退出码/协议照常但提示误导

## 7. 风险

| 风险 | 缓解 |
|------|------|
| Rust 不在 PATH / 需 MSVC 链接器 | 已装；`scripts/cargo-msvc.bat` 走 vcvars64 环境 |
| Monaco 体积 | 只打需要的语言 worker |
| 三方合并正确性 | merge-core 先写测试再写实现 |
| Git 版本差异 | 只用 `git ls-files -u` / `git add` / `git rev-parse` |
| 大文件卡顿 | 10 MB 阈值 + 虚拟化 |

## 8. 下一步

Phase 1–4（MVP：三栏 + 多文件 + 保存/退出码协议 + NSIS 安装包 + 自动登记）已全部完成，真实 `git mergetool` 全流程验证通过。

后续迭代（非 MVP）：
- macOS `.dmg`（同一套代码，补 bundled 前端 + 公证）
- 拖拽冲突块、批量处理、历史记录
- 主题切换（浅色）、完整 i18n
- Windows ARM / 便携版、非 UTF-8 完整编码、二进制冲突

**仓库**: https://github.com/1540962572/MergeDrag.git
**许可**: Apache-2.0
**最后更新**: 2026-08-14
