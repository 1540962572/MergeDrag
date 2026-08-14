// 工作空间 E2E：通过 CDP 驱动真实 Tauri 应用（WEBVIEW2 远程调试端口 9336）。
// 多阶段：git 状态变化由外层 bash 在阶段之间准备，脚本只做 UI/应用驱动。
//
//   PHASE=open     脚本：点首页「最近打开」的 work 仓库 → 读卡片（期望 ↑1，bash 已准备本地提交）
//   PHASE=push     脚本：点「推送」→ 读 toast + 卡片（期望 ↑0）
//   PHASE=pull     脚本：点「刷新」→ 落后1 → 点「拉取」→ 期望快进、落后0
//   PHASE=conflict 脚本：点「刷新」「拉取」→ 期望冲突、打开 data.txt、
//                      点第一个内嵌「← 取左」→ 点「保存」→ 期望已解决
//   PHASE=gitops  脚本：打开工作区（bash 已准备：master + data.txt 一段未提交改动）→
//                      分支面板新建并切换 feature-gitops → 暂存保存（工作树干净、列表 1 条）→
//                      应用（改动恢复）→ 提交面板提交全部（toast「已提交」）→ 分支面板切回 master
//
// 说明：open_workspace 等后端命令不在脚本里直接 invoke —— 那会绕过 React 状态。
//      必须驱动真实 UI（点按钮 / 点最近条目）让 App.tsx 的 handler 去调命令。
//
// 用法：node scripts/e2e-workspace.mjs   （E2E_WORKSPACE_ROOT + PHASE 环境变量）
// 可选 E2E_SHOT_DIR=目录 时在关键节点截图（Page.captureScreenshot）。

import fs from "node:fs";
import path from "node:path";

const APP_ROOT = process.env.E2E_WORKSPACE_ROOT;
const PHASE = process.env.PHASE || "open";
if (!APP_ROOT) {
  console.error("设置 E2E_WORKSPACE_ROOT=（工作区绝对路径）后运行");
  process.exit(2);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log(`[${PHASE}]`, ...a);

async function findPage(timeoutMs = 120_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const pages = await (await fetch("http://127.0.0.1:9336/json")).json();
      const page = pages.find((p) => p.type === "page" && p.webSocketDebuggerUrl);
      if (page) return page;
    } catch {
      /* app 还没起来 */
    }
    if (PHASE === "open") log("...等待应用窗口…");
    await sleep(1500);
  }
  throw new Error("超时未找到可调试页面");
}

class CDP {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.pending = new Map();
    this.ready = new Promise((res, rej) => {
      ws.addEventListener("open", res, { once: true });
      ws.addEventListener("error", () => rej(new Error("WebSocket 连接失败")), {
        once: true,
      });
    });
    ws.addEventListener("message", (ev) => {
      const msg = JSON.parse(ev.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
      }
    });
  }
  async send(method, params = {}) {
    await this.ready;
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }
  async eval(expression) {
    if (process.env.E2E_DUMP_EXPR) {
      console.error("=== DUMP EXPR ===\n" + expression + "\n=== END ===");
      return null;
    }
    const r = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) {
      const snippet =
        expression.length > 400
          ? expression.slice(0, 400) + "…"
          : expression;
      throw new Error(
        "eval 异常: " +
          (r.exceptionDetails.exception?.description ?? r.exceptionDetails.text) +
          "\n--- expression ---\n" +
          snippet,
      );
    }
    return r.result?.value;
  }
  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

// 页面内注入片段：轮询等待某元素出现后返回其内容/操作。
const pollFor = (body, timeoutMs = 10_000, pollMs = 300) => `
  new Promise((res, rej) => {
    const deadline = Date.now() + ${timeoutMs};
    const tick = () => {
      let v;
      try { v = ${body}; } catch (e) { return rej(e); }
      if (v) return res(v);
      if (Date.now() > deadline) return rej(new Error('轮询超时'));
      setTimeout(tick, ${pollMs});
    };
    tick();
  })`;

const readCard = `
  () => {
    const card = document.querySelector('.workspace-card');
    if (!card) return null;
    const q = (s) => card.querySelector(s)?.textContent?.trim() ?? null;
    return {
      folder: q('.ws-folder'),
      branch: q('.ws-branch'),
      summary: q('.ws-summary'),
      ops: Array.from(card.querySelectorAll('.ws-ops button')).map((b) => b.textContent.trim()),
      toast: q('.ws-toast'),
    };
  }`;

const readPane = `
  () => {
    const li = document.querySelector('.file-list li');
    return {
      file: li?.textContent?.trim() ?? null,
      status: li?.querySelector('.status')?.textContent?.trim() ?? null,
      widgets: document.querySelectorAll('.md-block-actions').length,
      header: document.querySelector('.app-header h1')?.textContent?.trim() ?? null,
      toast: document.querySelector('.ws-toast')?.textContent?.trim() ?? null,
      notice: document.querySelector('.status-bar span:last-child')?.textContent?.trim() ?? null,
    };
  }`;

// 把箭头函数源码组合成“立即调用表达式”。不要写成 `fn() && () => {...}()`：
// `&&`/`||` 右侧的裸箭头在 V8 里会被当作箭头参数列表解析而报 Malformed，必须整体加括号 (fn)()。
const call = (fnSrc) => `(${fnSrc})()`;

/** HTMLInput/TextArea 赋值（React 受控组件需要走原生 setter + input 事件）。 */
const setValExpr = (selectorExpr, value) => `(() => {
  const el = ${selectorExpr};
  if (!el) throw new Error('找不到输入框（' + ${JSON.stringify(selectorExpr)} + '）');
  const proto = el instanceof HTMLTextAreaElement
    ? window.HTMLTextAreaElement.prototype
    : window.HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, ${JSON.stringify(value)});
  el.dispatchEvent(new Event('input', { bubbles: true }));
})()`;

/** 在 E2E_SHOT_DIR 下存一张截图（可选）。 */
async function shot(cdp, label) {
  if (!process.env.E2E_SHOT_DIR) return;
  const r = await cdp.send("Page.captureScreenshot", { format: "png" });
  const out = path.join(process.env.E2E_SHOT_DIR, `${PHASE}-${label}.png`);
  fs.writeFileSync(out, Buffer.from(r.data, "base64"));
  log("截图 →", out);
}

async function clickButton(cdp, selectorExpr, label) {
  await cdp.eval(`
    (() => {
      const btn = ${selectorExpr};
      if (!btn || btn.disabled) throw new Error('未找到可用按钮 ${label}');
      btn.click();
    })()`);
}

async function connect() {
  const page = await findPage();
  const cdp = new CDP(new WebSocket(page.webSocketDebuggerUrl));
  await cdp.send("Runtime.enable");
  // 轮询体必须返回布尔：直接返回 `__TAURI_INTERNALS__` 对象时 WebView2 的
  // returnByValue 序列化会报 "Object reference chain is too long"。
  const invokeReady = `!!(window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)`;
  await cdp.eval(pollFor(invokeReady));
  // 启动竞态：webview 可能在 vite 就绪前拉到空 HTML（body 里没有 .app）。
  // 此时 reload 一次再等应用挂载，避免后续所有轮询都超时。
  let mounted = false;
  try {
    await cdp.eval(pollFor(`!!document.querySelector('.app')`, 8_000));
    mounted = true;
  } catch {
    /* 空白页，走 reload 路径 */
  }
  if (!mounted) {
    log("空白页，reload 重试…");
    await cdp.send("Page.reload", { ignoreCache: true });
    await sleep(2500);
    await cdp.eval(pollFor(invokeReady));
    await cdp.eval(pollFor(`!!document.querySelector('.app')`, 15_000));
  }
  return cdp;
}

async function phaseOpen(cdp) {
  // 原生目录选择对话框无法自动化：改从首页「最近打开」点开仓库，走真实 UI 流程
  // （调用 App.openWorkspace → open_workspace 命令 → setWorkspaceStatus）。
  // 前置：后端 recent 里已有该仓库（可直接 invoke open_workspace 预埋，或等前端点了再打开）。
  await cdp.eval(
    pollFor(`document.querySelectorAll('.home-recent li').length > 0`, 15_000),
  );
  const clicked = await cdp.eval(`(() => {
      const lis = Array.from(document.querySelectorAll('.home-recent li'));
      const li = lis.find((x) => (x.textContent || '').includes('work'));
      if (!li) throw new Error('home-recent 里找不到 work 条目: ' + lis.map((x) => x.textContent.trim()).join(' | '));
      li.click();
      return lis.map((x) => x.textContent.trim());
    })()`);
  log("点开最近工作区 →", JSON.stringify(clicked));
  const card = await cdp.eval(
    pollFor(`${call(readCard)} && ${call(readCard)}`, 20_000),
  );
  log("open_workspace →", JSON.stringify(card));
  if (!(card.summary || "").includes("↑1")) {
    throw new Error(`期望 summary 含 ↑1，实际: ${card.summary}`);
  }
  // 顺便观察 initial: 没有冲突文件时三栏是否给出工作区空态文案。
  const pane = await cdp.eval(pollFor(call(readPane)));
  log("三栏空态 →", JSON.stringify(pane));
}

async function phasePush(cdp) {
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-ops button')).find((b) => b.textContent.includes('推送'))`,
    "推送",
  );
  const card = await cdp.eval(pollFor(`(() => { const c = ${call(readCard)}; return c && c.toast ? c : null; })()`));
  log("推送后卡片 →", JSON.stringify(card));
  if ((card.summary || "").includes("↑1")) {
    throw new Error(`推送后不应仍 ↑1: ${card.summary}`);
  }
}

async function phasePull(cdp) {
  // 刷新（fetch + status）应先显示 ↓1
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-ops button')).find((b) => b.textContent.includes('刷新'))`,
    "刷新",
  );
  const before = await cdp.eval(
    pollFor(`(() => { const c = ${call(readCard)}; return c && c.summary && c.summary.includes('↓') ? c : null; })()`),
  );
  log("刷新后（期望 ↓1）→", JSON.stringify(before.summary));
  if (!before.summary.includes("↓1")) {
    throw new Error(`期望 ↓1，实际: ${before.summary}`);
  }

  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-ops button')).find((b) => b.textContent.includes('拉取'))`,
    "拉取",
  );
  const after = await cdp.eval(
    pollFor(`(() => {
      const c = ${call(readCard)};
      // 拉取完成：toast 有内容，且 summary 不再 ↓1
      if (c && c.toast && c.summary && !c.summary.includes('↓')) return c;
      return null;
    })()`),
  );
  log("拉取后 →", JSON.stringify(after));
}

async function phaseConflict(cdp) {
  // 刷新看到 ↓1（有冲突的落后）
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-ops button')).find((b) => b.textContent.includes('刷新'))`,
    "刷新",
  );
  await sleep(1200);

  // 拉取 → 期望产生冲突：toast 含「冲突」，左侧出现 data.txt
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-ops button')).find((b) => b.textContent.includes('拉取'))`,
    "拉取",
  );
  const conflicted = await cdp.eval(
    pollFor(`(() => {
      const t = document.querySelector('.ws-toast')?.textContent ?? '';
      const li = document.querySelector('.file-list li');
      if (t.includes('冲突') && li) {
        return { toast: t, file: li.textContent.trim(), status: li.querySelector('.status')?.textContent?.trim() ?? '' };
      }
      return null;
    })()`, 20_000, 400),
  );
  log("拉取冲突 →", JSON.stringify(conflicted));

  // 三栏应打开 data.txt，且内嵌操作条 > 0
  await cdp.eval(
    pollFor(`document.querySelectorAll('.md-block-actions').length > 0`),
  );
  const pane = await cdp.eval(pollFor(call(readPane)));
  log("三栏 →", JSON.stringify(pane));

  // 点第一个内嵌「← 取左」
  await clickButton(
    cdp,
    `document.querySelector('.md-block-actions .md-w-a')`,
    "内嵌取左",
  );
  await sleep(800);

  // 保存
  await clickButton(cdp, `document.querySelector('.app-header button.save')`, "保存");
  const saved = await cdp.eval(
    pollFor(`(() => {
      const st = document.querySelector('.file-list li .status')?.textContent?.trim();
      return st === '已解决' ? st : null;
    })()`, 15_000, 400),
  );
  log("保存后列表状态 →", saved);
}

/** 分支面板里找按钮（新建/新建并切换/行内切换等）。 */
const branchPanelBtn = (label) =>
  `Array.from(document.querySelectorAll('.ws-panel button')).find((b) => b.textContent.trim() === '${label}')`;

/** Git 工具箱面板切换按钮（分支/暂存/提交）。 */
const toolBtn = (label) =>
  `Array.from(document.querySelectorAll('.ws-gitops-tools button')).find((b) => b.textContent.trim() === '${label}')`;

async function phaseGitOps(cdp) {
  // 前置（外层 bash 已准备）：work 干净 master，data.txt 有一段未提交改动；
  // stash 清空、无 feature-gitops 遗留。
  // 若上次运行把应用停在「工作区视图」（非首页），先 reload 回首页再点开。
  if (await cdp.eval(`!!document.querySelector('.workspace-card')`)) {
    log("已在工作区视图，reload 回首页…");
    await cdp.send("Page.reload", { ignoreCache: true });
    await sleep(2000);
    await cdp.eval(pollFor(`document.querySelectorAll('.home-recent li').length > 0`, 15_000));
  }
  // 1) 点开工作区。
  await cdp.eval(pollFor(`document.querySelectorAll('.home-recent li').length > 0`, 15_000));
  await cdp.eval(`(() => {
      const lis = Array.from(document.querySelectorAll('.home-recent li'));
      const li = lis.find((x) => (x.textContent || '').includes('work'));
      if (!li) throw new Error('home-recent 里找不到 work 条目: ' + lis.map((x) => x.textContent.trim()).join(' | '));
      li.click();
    })()`);
  const card0 = await cdp.eval(pollFor(`${call(readCard)} && ${call(readCard)}`, 20_000));
  log("打开工作区 →", JSON.stringify(card0));
  if (card0.branch !== "master") throw new Error(`期望 master，实际 ${card0.branch}`);
  if (!card0.summary.includes("有改动")) {
    throw new Error(`预期工作树有改动（bash 前置），实际 summary: ${card0.summary}`);
  }

  // 2) 分支面板：新建 feature-gitops + 新建并切换（一次性）。
  await shot(cdp, "0-opened");
  await clickButton(cdp, toolBtn("分支"), "分支");
  // 等分支从后端拉完（初始空态会渲染「还没有本地分支」，不能用它当就绪信号）。
  await cdp.eval(
    pollFor(`document.querySelectorAll('.ws-plist li:not(.empty)').length >= 1`, 10_000),
  );
  const blist = await cdp.eval(pollFor(`(() => {
      const rows = Array.from(document.querySelectorAll('.ws-plist li')).map((li) => li.textContent.trim());
      return rows.length > 0 ? rows : null;
    })()`));
  log("分支列表 →", JSON.stringify(blist));
  const hasMaster = blist.some((t) => t.includes("master"));
  if (!hasMaster) throw new Error(`分支列表里没有 master: ${JSON.stringify(blist)}`);
  await shot(cdp, "1-branch-list");

  await cdp.eval(setValExpr(`document.querySelector('.ws-panel input')`, "feature-gitops"));
  await clickButton(cdp, branchPanelBtn("新建并切换"), "新建并切换");
  const card1 = await cdp.eval(
    pollFor(`(() => { const c = ${call(readCard)}; return c && c.branch === 'feature-gitops' ? c : null; })()`, 20_000, 400),
  );
  log("新建并切换 →", JSON.stringify(card1));
  if (!card1.summary.includes("有改动")) {
    throw new Error(`切换后脏改动应被带到 feature-gitops，summary: ${card1.summary}`);
  }
  await shot(cdp, "2-switched");

  // 3) 暂存面板：保存当前改动 → 工作树干净 + stash 列表 1 条。
  await clickButton(cdp, toolBtn("暂存"), "暂存");
  await cdp.eval(pollFor(`!!document.querySelector('.ws-panel input')`, 10_000));
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-panel button')).find((b) => b.textContent.trim() === '保存')`,
    "保存 stash",
  );
  const afterSave = await cdp.eval(
    pollFor(`(() => {
      const c = ${call(readCard)};
      const rows = document.querySelectorAll('.ws-plist li:not(.empty)').length;
      if (!c || c.branch !== 'feature-gitops') return null;
      const clean = !!c.summary && !c.summary.includes('有改动');
      return clean && rows === 1 ? { summary: c.summary, toast: c.toast, rows } : null;
    })()`, 15_000, 400),
  );
  log("暂存保存后 →", JSON.stringify(afterSave));
  if (!(afterSave.toast || "").length) throw new Error("stash 保存后应出现 toast");
  await shot(cdp, "3-stash-saved");

  // 4) 应用 stash → 改动恢复 + 列表清空。
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-plist li button')).find((b) => b.textContent.trim() === '应用')`,
    "应用 stash",
  );
  await cdp.eval(
    pollFor(`(() => {
      const c = ${call(readCard)};
      const rows = document.querySelectorAll('.ws-plist li:not(.empty)').length;
      return c && c.branch === 'feature-gitops' && (c.summary || '').includes('有改动') && rows === 0 ? c : null;
    })()`, 15_000, 400),
  );
  log("应用 stash 后列表已清空、改动恢复");

  // 5) 提交面板：提交全部 → toast「已提交」+ 工作树干净。
  await clickButton(cdp, toolBtn("提交"), "提交");
  await cdp.eval(pollFor(`!!document.querySelector('.ws-commit-input')`, 10_000));
  await cdp.eval(setValExpr(`document.querySelector('.ws-commit-input')`, "gitops: 提交全部改动"));
  await clickButton(
    cdp,
    `Array.from(document.querySelectorAll('.ws-panel button')).find((b) => b.textContent.trim() === '提交全部')`,
    "提交全部",
  );
  const afterCommit = await cdp.eval(
    pollFor(`(() => {
      const c = ${call(readCard)};
      if (!c || c.branch !== 'feature-gitops' || !c.summary) return null;
      const clean = !c.summary.includes('有改动') && !c.summary.includes('冲突');
      return clean && (c.toast || '').includes('已提交') ? c : null;
    })()`, 15_000, 400),
  );
  log("提交后 →", JSON.stringify(afterCommit));
  await shot(cdp, "4-committed");

  // 6) 分支面板切回 master。
  await clickButton(cdp, toolBtn("分支"), "分支");
  await cdp.eval(
    pollFor(`document.querySelectorAll('.ws-plist li:not(.empty)').length >= 1`, 10_000),
  );
  await clickButton(
    cdp,
    `(() => {
      const li = Array.from(document.querySelectorAll('.ws-plist li')).find((x) => x.textContent.includes('master') && !x.textContent.includes('（当前）'));
      if (!li) throw new Error('列表里找不到非当前的 master');
      return Array.from(li.querySelectorAll('button')).find((b) => b.textContent.trim() === '切换');
    })()`,
    "master 切换",
  );
  const card3 = await cdp.eval(
    pollFor(`(() => { const c = ${call(readCard)}; return c && c.branch === 'master' ? c : null; })()`, 20_000, 400),
  );
  log("切回 master →", JSON.stringify(card3));
}

async function main() {
  const cdp = await connect();
  try {
    if (PHASE === "push") await phasePush(cdp);
    else if (PHASE === "pull") await phasePull(cdp);
    else if (PHASE === "conflict") await phaseConflict(cdp);
    else if (PHASE === "gitops") await phaseGitOps(cdp);
    else await phaseOpen(cdp);
    log("PASS");
  } finally {
    cdp.close();
  }
}

main().catch((e) => {
  console.error(`[${PHASE}] FAIL:`, e.message);
  process.exit(1);
});