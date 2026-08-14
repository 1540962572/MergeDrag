import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { BranchInfo, StashEntry } from "../../shared/types";

/**
 * 工作区卡片里的三个内联 Git 操作面板：分支 / 暂存 / 提交。
 * 每个面板由 WorkspaceBar 的「分支 / 暂存 / 提交」按钮懒加载（打开时才挂载、取数据）。
 * 操作结果通过 onToast 上报（渲染在 .ws-toast），仓库状态变化通过 onChanged 通知
 * App 静默刷新状态卡（分支名、dirty、冲突数等）。
 */

export interface GitPanelProps {
  onToast: (m: string) => void;
  onChanged: () => void;
}

function inTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** 分支面板：列表 + 切换/强制切换 + 新建（可同时切换）。 */
export function BranchPanel({ onToast, onChanged }: GitPanelProps) {
  const [branches, setBranches] = useState<BranchInfo[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      setBranches(await invoke<BranchInfo[]>("list_branches"));
    } catch (e) {
      onToast(`读取分支失败: ${e}`);
    }
  };
  useEffect(() => {
    void load();
  }, []);

  const create = async (andSwitch: boolean) => {
    const n = name.trim();
    if (!n) {
      onToast("请输入分支名");
      return;
    }
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    setBusy(true);
    try {
      await invoke<string>("create_branch", { name: n });
      if (andSwitch) {
        const msg = await invoke<string>("switch_branch", {
          name: n,
          force: false,
        });
        onToast(msg ? `已切换到 ${n}\n${msg}` : `已切换到 ${n}`);
      } else {
        onToast(`已创建分支 ${n}`);
      }
      setName("");
      await load();
      onChanged();
    } catch (e) {
      onToast(`操作失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const doSwitch = async (b: string, force: boolean) => {
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    setBusy(true);
    try {
      const msg = await invoke<string>("switch_branch", { name: b, force });
      onToast(msg || `已切换到 ${b}`);
      await load();
      onChanged();
    } catch (e) {
      onToast(
        force
          ? `强制切换失败: ${e}`
          : `切换失败: ${e}\n如可放弃未提交改动，可用「强制」丢弃后再切。`,
      );
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ws-panel">
      <div className="ws-panel-head">
        <span className="ws-panel-title">分支</span>
        <span className="ws-panel-note">
          共 {branches.length} 个本地分支；当前为{" "}
          {branches.find((b) => b.current)?.name ?? "(detached)"}
        </span>
      </div>
      <ul className="ws-plist">
        {branches.map((b) => (
          <li key={b.name} className={b.current ? "current" : undefined}>
            <span className="ws-bname">
              {b.name}
              {b.current && <em>（当前）</em>}
            </span>
            {!b.current && (
              <span className="ws-bops">
                <button
                  type="button"
                  className="secondary"
                  disabled={busy}
                  onClick={() => void doSwitch(b.name, false)}
                >
                  切换
                </button>
                <button
                  type="button"
                  className="secondary danger"
                  disabled={busy}
                  title="丢弃未提交改动后切换到该分支"
                  onClick={() => void doSwitch(b.name, true)}
                >
                  强制
                </button>
              </span>
            )}
          </li>
        ))}
        {branches.length === 0 && <li className="empty">（还没有本地分支）</li>}
      </ul>
      <div className="ws-panel-row">
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="新分支名"
          onKeyDown={(e) => {
            if (e.key === "Enter") void create(false);
          }}
        />
        <button
          type="button"
          className="secondary"
          disabled={busy || !name.trim()}
          onClick={() => void create(false)}
        >
          新建
        </button>
        <button
          type="button"
          disabled={busy || !name.trim()}
          title="在当前位置新建并切换过去"
          onClick={() => void create(true)}
        >
          新建并切换
        </button>
      </div>
    </div>
  );
}

/** 暂存面板：列表 + 应用/删除 + 保存当前改动。 */
export function StashPanel({ onToast, onChanged }: GitPanelProps) {
  const [stashes, setStashes] = useState<StashEntry[]>([]);
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);

  const load = async () => {
    try {
      setStashes(await invoke<StashEntry[]>("list_stashes"));
    } catch (e) {
      onToast(`读取暂存失败: ${e}`);
    }
  };
  useEffect(() => {
    void load();
  }, []);

  const save = async () => {
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    setBusy(true);
    try {
      const out = await invoke<string>("stash_push", {
        message: msg.trim() || null,
      });
      onToast(out || "改动已保存到 stash");
      setMsg("");
      await load();
      onChanged();
    } catch (e) {
      onToast(`暂存失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const pop = async (index: number) => {
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    setBusy(true);
    try {
      const out = await invoke<string>("stash_pop", { index });
      onToast(out || `stash@{${index}} 已恢复`);
      await load();
      onChanged();
    } catch (e) {
      onToast(`应用失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  const drop = async (index: number) => {
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    if (!window.confirm(`确认删除 stash@{${index}}？其中的改动将丢失。`)) return;
    setBusy(true);
    try {
      await invoke<string>("stash_drop", { index });
      onToast(`已删除 stash@{${index}}`);
      await load();
    } catch (e) {
      onToast(`删除失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ws-panel">
      <div className="ws-panel-head">
        <span className="ws-panel-title">Stash</span>
        <span className="ws-panel-note">
          {stashes.length} 条保存；应用成功后该条会被移除
        </span>
      </div>
      <ul className="ws-plist">
        {stashes.map((s) => (
          <li key={s.index}>
            <span className="ws-bname" title={s.message}>
              {s.message}
            </span>
            <span className="ws-bops">
              <button
                type="button"
                className="secondary"
                disabled={busy}
                title={`git stash pop stash@{${
                  s.index
                }}：恢复并删除该条`}
                onClick={() => void pop(s.index)}
              >
                应用
              </button>
              <button
                type="button"
                className="secondary danger"
                disabled={busy}
                onClick={() => void drop(s.index)}
              >
                删除
              </button>
            </span>
          </li>
        ))}
        {stashes.length === 0 && <li className="empty">（暂存区为空）</li>}
      </ul>
      <div className="ws-panel-row">
        <input
          value={msg}
          onChange={(e) => setMsg(e.target.value)}
          placeholder="保存说明（可选）"
          onKeyDown={(e) => {
            if (e.key === "Enter") void save();
          }}
        />
        <button
          type="button"
          disabled={busy}
          title="git stash push：保存已跟踪文件的改动"
          onClick={() => void save()}
        >
          保存
        </button>
      </div>
    </div>
  );
}

/** 提交面板：一次 `git add -A` + `git commit -m`。 */
export function CommitPanel({ onToast, onChanged }: GitPanelProps) {
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);

  const commit = async () => {
    const m = msg.trim();
    if (!m) {
      onToast("请输入提交信息");
      return;
    }
    if (!inTauri()) {
      onToast("浏览器模式无法操作 Git");
      return;
    }
    setBusy(true);
    try {
      const summary = await invoke<string>("commit_all", { message: m });
      onToast(`已提交 ${summary}`);
      setMsg("");
      onChanged();
    } catch (e) {
      onToast(`提交失败: ${e}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ws-panel">
      <div className="ws-panel-head">
        <span className="ws-panel-title">提交</span>
        <span className="ws-panel-note" title="git add -A + git commit -m">
          将暂存并提交全部改动（含新增文件）
        </span>
      </div>
      <textarea
        className="ws-commit-input"
        value={msg}
        onChange={(e) => setMsg(e.target.value)}
        placeholder="提交信息（第一行为标题）"
        rows={3}
        onKeyDown={(e) => {
          if ((e.ctrlKey || e.metaKey) && e.key === "Enter") void commit();
        }}
      />
      <div className="ws-panel-row ws-commit-actions">
        <button
          type="button"
          disabled={busy || !msg.trim()}
          onClick={() => void commit()}
        >
          {busy ? "提交中…" : "提交全部"}
        </button>
        <span className="ws-commit-hint">Ctrl+Enter 提交</span>
      </div>
    </div>
  );
}