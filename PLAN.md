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

### Phase 5: 工作空间（IDEA 式仓库管理，完成，2026-08-14）
用户新增需求：打开本地含 `.git` 的文件夹即可托管仓库，参考 IDEA 的右键 Git 菜单 → 拉取/推送/解决冲突。
- **后端 git-bridge**（✓ `crates/git-bridge` workspace 模块 + 4 个集成测试，真实本地 bare remote）：
  - `repo_status`：根目录 / 分支 / upstream（`rev-list --left-right --count` 算 ahead/behind）/ merging 状态
    （detect MERGE_HEAD、rebase-merge…）/ 未合并文件数（index stage≠0）/ 工作树脏标记 / 人话 summary（`master ↑2 ↓1`）
  - `pull_now`（`pull --no-edit`，conflict 时吞 Err 进 message、报 conflicted）、`push_now`（`GIT_TERMINAL_PROMPT=0` 永不挂起）、
    `refresh_workspace`（fetch + status）、`open_workspace`（校验 → 设 workspace_root → 记 recents）
  - recent 持久化：`app_data_dir/workspaces.json`（去重插头、上限 10）（✓）
  - workspace_root 参与 `scan_repo`/`open_file`（工作区模式打开冲突文件不再只认 mergetool 会话）
  - **非 mergetool 启动退出码 0**：`on_window_event` 仅在 `launch.is_mergetool_launch()` 时 `std::process::exit(code)`
- **前端**（✓ `WorkspaceHome` 首页最近打开/选择文件夹、`WorkspaceBar` 状态卡片 + 刷新/拉取/推送/解决冲突按钮）：
  - 打开后自动扫未合并文件并打开第一个冲突（✓）
  - 拉取产生冲突 → toast 提示 → 自动转入三栏逐块解决 → 保存（✓ 与既有决策流程完全打通）
- **真实应用 CDP E2E（✓ 2026-08-14，`scripts/e2e-workspace.mjs` 四阶段全部 PASS）**：
  - open：首页点「最近打开」→ 卡片 `master ↑1` ✓
  - push：点「推送」→ `master` 同步 ✓
  - pull：`刷新` → `↓1` → `拉取` 快进 ✓
  - conflict：双端改同一行 → 刷新/拉取 → 冲突 toast + 三栏 1 个内嵌操作条 → 取左 → 保存「已保存并已 git add」→
    列表「已解决」，git 状态核验：index 无 unmerged、工作树为取左内容 ✓
- **E2E 踩的两类坑（已写回脚本）**：
  - V8 对 `A() && () => {}` / `const c = () => {}` 后跟调用在嵌套上下文有解析歧义 →
    组合片段必须整体加括号 `(fn)()`（脚本里 `call(fnSrc)` 助手）
  - 后端命令不能直接 `invoke` 驱动 UI（绕过 React）→ 必须点真实按钮/最近条目走 App handler；
    且 recent 是 mount 时快照，改完 recents 要重启 app 才刷新

### Phase 6: 分支 / 暂存 / 提交（Git 操作面板，完成，2026-08-14）
工作空间增强：「非 MVP」迭代里的分支切换/新建、stash、提交对话框先落地。
- **后端 git-bridge**（✓ 5 个新集成测试，真实 bare remote fixture）：
  - `list_branches`（`git branch --format=%(refname:short)` + current 标记）、
    `create_branch`（空名报可读错误）、`switch_branch(name, force)`（`git checkout [-f]`：
    脏树被拒则分支不变，force 丢弃改动，单测锁定两种行为）
  - `list_stashes`（`stash list --format=%gs`，index 即 `stash@{n}`）、
    `stash_push(message)`（干净工作树给「没有可保存的改动」而非 git 原文）、
    `stash_pop` / `stash_pop_index`、`stash_drop(index)`
  - `commit_all(message)`（`add -A` + `commit -m`，返回 `短hash 标题`；空信息拒绝）
  - **顺带修掉一个潜在 bug**：`load_upstream` 曾把「`@{upstream}` 无上游」的 git 非零退出当作
    致命错误传播——无上游分支（新建分支/新仓库）会让整张状态卡失败；现视为「无上游」，
    远端 URL 的 config 查询同样容错回退 origin
- **Tauri 命令**（✓ 8 个注册进 invoke_handler）：list_branches / create_branch /
  switch_branch / list_stashes / stash_push / stash_pop / stash_drop / commit_all，
  全部经 `current_workspace_root` 定位仓库
- **前端**（✓ WorkspaceBar 第二行「分支 / 暂存 / 提交」切换 + 内联面板 `git-panels.tsx`）：
  - 分支：列表（当前 ✓）、切换 / 强制切换（丢弃改动）、新建、新建并切换；
    面板懒加载，卸载再开自动重取
  - 暂存：列表（应用 / 删除，删除弹确认）、保存当前改动（可填说明）
  - 提交：多行输入（Ctrl+Enter）+ 「提交全部」（toast「已提交 短hash 标题」）
  - 任一生效后经 `onChanged` 静默刷新状态卡（不影响按钮 busy 态）
- **真实应用 CDP E2E（✓ PHASE=gitops 全流程 PASS）**：打开工作区（master ↑1 ↓1 · 有改动）→
  分支面板「新建并切换」feature-gitops（脏改动随切换带过去）→ 暂存保存（工作树干净、列表 1 条）→
  应用（改动恢复、列表清空）→ 提交全部（`已提交 cce1ce1 gitops: 提交全部改动`）→ 切回 master。
  git 状态核验：feature-gitops 含该提交、master 未动、工作树干净、stash 空
- **E2E 脚本加固**：connect() 空白页自动 reload；CDP 轮询体一律只返回布尔（WebView2 的
  returnByValue 对 DOM 元素或 `__TAURI_INTERNALS__` 深对象报 `Object reference chain is too long`）；
  面板「初始空态」不是就绪信号，必须等非空行再断言

### Phase 7: 合并收尾 / 暂存带未跟踪 / 逐文件提交（完成，2026-08-14）
承接 Phase 6：合并冲突解决后的「完成合并 / 放弃合并」收尾、stash 可选带未跟踪文件、
提交面板从「一键全提交」升级为逐文件勾选提交。
- **后端 git-bridge**（✓ 集成测试 14 个函数级通过，含 5 个新增）：
  - `merge_continue`：前置校验 MERGE_HEAD 存在 + **index 无未合并文件**，否则给「还有未解决的冲突文件」
    而非 git 原文；收尾为合并提交（`git merge --continue`）
  - `merge_abort`：`git merge --abort`，回合并前工作树（单测锁定 MERGE_HEAD 清除 + porcelain 干净 + 内容复原）
  - `list_changes`：`git status --porcelain` 解析成 `(路径, M/A/D/R/?)`；**修了一个隐蔽切片 bug**——
    `run_git` 对整段 stdout 做 `.trim()` 会把首行状态列 ` M xxx` 的前导空格吃掉，
    导致路径从第 4 个字符切起而丢首字符（表现为 `data.txt` → `ata.txt`）；
    新增 `run_git_aligned`（只剥行尾换行、不动行首空白）专供列表/对齐文本用
  - `commit_files(paths, message)`：只暂存并提交勾选文件；重命名路径（`旧 -> 新`）自动拆两边一并 add
  - `stash_push(message, include_untracked)`：`-u` 时未跟踪文件也进 stash（单测验证两种语义）
- **Tauri 命令**（✓ 注册进 invoke_handler）：list_changes / commit_files / merge_continue / merge_abort；
  stash_push 增加 `includeUntracked`
- **前端**：
  - WorkspaceBar：`merging && unmergedCount==0` 时「解决冲突」让位为「完成合并」+「放弃合并」
    （放弃合并先 confirm），经 App handler 调命令后刷新状态卡 + 重扫左栏
  - Stash 面板：保存行下加「连同未跟踪文件（git stash push -u）」勾选框
  - 提交面板：进入即加载 `list_changes`，逐文件 checkbox 列表（改/新/删/移/未跟踪 徽标）+ 全选 +
    「提交选中 (N)」（`commit_files`）+ 保留「提交全部」兜底；提交成功后清空选择并重载列表
- **真实应用 CDP E2E（✓ 三个新阶段全 PASS）**：
  - `gitops2`：勾选 -u 保存 → 工作树干净且磁盘上未跟踪文件消失 → 应用一并恢复 →
    只勾 data.txt 提交选中（toast `已提交 8139b77 gitops2: 只提交 data.txt`、列表剩 untracked2.txt）→
    提交全部收尾
  - `mergeops`：双端改同一行制造冲突 → 卡片「合并中 · 1 个冲突」+ 解决冲突(1) → 三栏逐块取左 + 保存 →
    刷新后卡片出现「完成合并 / 放弃合并」→ 点完成合并 → toast `合并完成：[master f878b6c] ...`、
    无合并中、`.git/MERGE_HEAD` 清除、左栏清空
  - `mergeops-abort`：同流程到按钮出现 → 点放弃合并（confirm 由脚本 stub）→ 合并状态消失、
    MERGE_HEAD 清除、data.txt 回合并前内容
  - 关键认知写进脚本：三栏决策只改前端 hunk 状态，**工作区卡片跟随 git index**（保存后 git add
    才清零 unmerged），所以「保存 → 刷新 → 完成合并按钮出现」是真实用户序列

## 7. 风险

| 风险 | 缓解 |
|------|------|
| Rust 不在 PATH / 需 MSVC 链接器 | 已装；`scripts/cargo-msvc.bat` 走 vcvars64 环境 |
| Monaco 体积 | 只打需要的语言 worker |
| 三方合并正确性 | merge-core 先写测试再写实现 |
| Git 版本差异 | 只用 `git ls-files -u` / `git add` / `git rev-parse` |
| 大文件卡顿 | 10 MB 阈值 + 虚拟化 |

## 8. 下一步

Phase 1–7（MVP：三栏 + 多文件 + 保存/退出码协议 + NSIS 安装包 + 自动登记 +
工作空间拉取/推送/冲突解决 + 分支切换/新建、stash、提交面板 +
合并收尾、暂存带未跟踪、逐文件提交）已全部完成，真实 `git mergetool` 全流程、
工作空间四/六步与 gitops2 / mergeops / mergeops-abort CDP E2E 均验证通过。

后续迭代（非 MVP）：
- 恢复进行中合并（打开即接管 MERGE_HEAD 状态）、代理/推送配置
- macOS `.dmg`（同一套代码，补 bundled 前端 + 公证）
- 拖拽冲突块、批量处理、历史记录
- 主题切换（浅色）、完整 i18n
- 工作空间增强：提交历史浏览、检出旧版本、rebase 流程
- Windows ARM / 便携版、非 UTF-8 完整编码、二进制冲突

**仓库**: https://github.com/1540962572/MergeDrag.git
**许可**: Apache-2.0
**最后更新**: 2026-08-14
