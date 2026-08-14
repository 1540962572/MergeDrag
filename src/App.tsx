import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileList } from "./features/file-list/FileList";
import { ThreePane } from "./features/three-pane/ThreePane";
import { SAMPLE_DOCUMENT, SAMPLE_FILES } from "./shared/sample";
import {
  applyHunks,
  conflictCount,
  type Decision,
  type Hunk,
  type MergeDocument,
  type SaveResult,
  type SessionSnapshot,
} from "./shared/types";

const emptySession: SessionSnapshot = {
  repoRoot: null,
  files: [],
  current: null,
  message: "请通过 git mergetool 启动，或打开示例冲突。",
};

/** Whether the @tauri-apps/api `invoke` bridge is available (inside Tauri). */
function inTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

export default function App() {
  const [session, setSession] = useState<SessionSnapshot>(emptySession);
  // 用户在 Result 面板里手改的内容：一旦手改，整份结果以它为准，
  // 之后任何一次点箭头决策都会清掉，回到 hunk 模型计算的文本。
  const [manualOverride, setManualOverride] = useState<string | null>(null);
  // 状态栏提示（保存结果等）。
  const [notice, setNotice] = useState<string | null>(null);
  // 全局 mergetool 登记状态（空壳启动时自动检测/登记）。
  const [mergetool, setMergetool] = useState<{
    status: "idle" | "done" | "error";
    message: string;
  } | null>(null);

  const refreshMergetool = useCallback(() => {
    if (!inTauri()) return;
    invoke<string>("ensure_mergetool_registered")
      .then((m) => setMergetool({ status: "done", message: m }))
      .catch((e) =>
        setMergetool({ status: "error", message: `登记失败: ${e}` }),
      );
  }, []);

  const forceRegister = useCallback(() => {
    if (!inTauri()) return;
    invoke<string>("register_mergetool")
      .then((m) => setMergetool({ status: "done", message: m }))
      .catch((e) =>
        setMergetool({ status: "error", message: `登记失败: ${e}` }),
      );
  }, []);

  // On mount: if launched by git mergetool, ask the backend to build a session.
  // 非 mergetool 启动（空壳）时顺便检查/登记全局 mergetool。
  useEffect(() => {
    if (!inTauri()) return;
    invoke<SessionSnapshot>("open_session")
      .then((s) => {
        setSession(s);
        if (!s.current) refreshMergetool();
      })
      .catch((e) =>
        setSession((s) => ({ ...s, message: `打开会话失败: ${e}` })),
      );
  }, []);

  const current: MergeDocument | null = session.current ?? null;

  /** 与 Rust 的 unresolved_count() 口径一致：Unresolved 和 Manual 都算未解决。 */
  function countUnresolved(doc: MergeDocument): number {
    return doc.hunks
      .filter(
        (h) =>
          h.kind === "Conflict" &&
          (h.decision.kind === "Unresolved" || h.decision.kind === "Manual"),
      )
      .length;
  }

  const unresolved = current ? countUnresolved(current) : 0;

  /** 决策后同步左栏该文件的未解决徽标。 */
  const refreshFileBadge = (
    s: SessionSnapshot,
    doc: MergeDocument,
  ): SessionSnapshot => ({
    ...s,
    current: doc,
    files: s.files.map((f) =>
      f.path === doc.fileLabel
        ? { ...f, unresolvedCount: countUnresolved(doc) }
        : f,
    ),
  });

  const openSample = useCallback(async () => {
    setNotice(null);
    if (inTauri()) {
      try {
        setSession(await invoke<SessionSnapshot>("load_sample"));
        return;
      } catch {
        // fall through to local sample
      }
    }
    setSession({
      repoRoot: null,
      files: SAMPLE_FILES,
      current: SAMPLE_DOCUMENT,
      message: "已加载示例冲突（开发用）。",
    });
  }, []);

  /** 保存当前文件：写入 $MERGED（或左栏选中文件的工作树路径）。
   *  仍有未解决项时先确认；全部解决后由后端顺带 git add。 */
  const save = useCallback(async () => {
    if (!current) return;
    const text = manualOverride ?? applyHunks(current.hunks);
    if (!inTauri()) {
      setNotice("浏览器模式无法写盘（示例冲突仅供演示）。");
      return;
    }
    const unfinished = countUnresolved(current);
    if (
      unfinished > 0 &&
      !window.confirm(
        `仍有 ${unfinished} 处冲突未解决，确定保存？\n未解决的块会保留冲突标记，且不会 git add。`,
      )
    ) {
      return;
    }
    try {
      const result = await invoke<SaveResult>("save_merged", { text });
      setNotice(
        result.added
          ? "已保存并已 git add"
          : result.warning ?? "已保存（有未解决项，未 git add）",
      );
    } catch (e) {
      setNotice(`保存失败: ${e}`);
    }
  }, [current, manualOverride]);

  /** 左栏点击：从 index 读三方内容并打开该文件。 */
  const openFile = useCallback(
    async (path: string) => {
      setNotice(null);
      setManualOverride(null);
      if (!inTauri()) {
        // 浏览器回退：切到唯一示例文件。
        setSession({
          repoRoot: null,
          files: SAMPLE_FILES,
          current: SAMPLE_DOCUMENT,
          message: "浏览器模式仅支持示例冲突。",
        });
        return;
      }
      try {
        setSession(await invoke<SessionSnapshot>("open_file", { relPath: path }));
      } catch (e) {
        setNotice(`打开 ${path} 失败: ${e}`);
      }
    },
    [],
  );

  /** Commit a decision for a conflict hunk; refresh the document from backend. */
  const decide = useCallback(
    async (hunkId: number, decision: Decision) => {
      setManualOverride(null);
      if (!inTauri()) {
        // Browser fallback: update the local copy optimistically.
        setSession((s) => {
          if (!s.current) return s;
          const hunks = s.current.hunks.map((h) =>
            h.kind === "Conflict" && h.id === hunkId
              ? { ...h, decision }
              : h,
          );
          return refreshFileBadge(s, { ...s.current, hunks });
        });
        return;
      }
      try {
        const doc = await invoke<MergeDocument>("set_decision", {
          hunkId,
          decision,
        });
        setSession((s) => (s.current ? refreshFileBadge(s, doc) : s));
      } catch (e) {
        console.error("set_decision failed", e);
      }
    },
    [],
  );

  /** 全部未解决块：取左或取右。顺序回写后端（浏览器环境则本地折叠）。 */
  const acceptAll = useCallback(
    async (side: "local" | "remote") => {
      if (!current) return;
      setManualOverride(null);
      const decision: Decision =
        side === "local" ? { kind: "TakeLocal" } : { kind: "TakeRemote" };
      const ids = current.hunks
        .filter(
          (h) =>
            h.kind === "Conflict" &&
            (h.decision.kind === "Unresolved" || h.decision.kind === "Manual"),
        )
        .map((h) => (h as Extract<Hunk, { kind: "Conflict" }>).id);

      if (inTauri()) {
        try {
          let doc = current;
          for (const id of ids) {
            doc = await invoke<MergeDocument>("set_decision", {
              hunkId: id,
              decision,
            });
          }
          setSession((s) => (s.current ? refreshFileBadge(s, doc) : s));
          return;
        } catch (e) {
          console.error("setDecision 全部接受失败", e);
          return;
        }
      }
      setSession((s) => {
        if (!s.current) return s;
        const idset = new Set(ids);
        const hunks = s.current.hunks.map((h) =>
          h.kind === "Conflict" && idset.has(h.id) ? { ...h, decision } : h,
        );
        return refreshFileBadge(s, { ...s.current, hunks });
      });
    },
    [current],
  );

  /** 用户手改了 Result 面板内容。 */
  const onManualEdit = useCallback((text: string) => {
    setManualOverride(text);
  }, []);

  // Ctrl+S 保存。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [save]);

  // Recalculate the Result pane's text whenever the document changes
  // (unless the user is hand-editing it).
  const resultText = current
    ? manualOverride ?? applyHunks(current.hunks)
    : "";

  return (
    <div className="app">
      <header className="app-header">
        <h1>MergeDrag{current ? ` — ${current.fileLabel}` : ""}</h1>
        <button
          type="button"
          className="save"
          onClick={save}
          disabled={!current}
          title="写入当前文件（Ctrl+S）"
        >
          保存
        </button>
        <button type="button" className="secondary" onClick={openSample}>
          打开示例冲突
        </button>
      </header>
      {mergetool && (
        <div
          className={
            mergetool.status === "done"
              ? "mergetool-banner ok"
              : "mergetool-banner err"
          }
        >
          <span>{mergetool.message}</span>
          <button
            type="button"
            className="secondary"
            onClick={forceRegister}
            title="强制写入全局 git config（merge.tool / mergetool.mergedrag.*）"
          >
            重新登记
          </button>
        </div>
      )}
      <div className="app-body">
        <FileList
          files={session.files}
          activePath={current?.fileLabel ?? null}
          onSelect={openFile}
        />
        <ThreePane
          document={current}
          resultText={resultText}
          onDecide={decide}
          onAcceptAll={acceptAll}
          onManualEdit={onManualEdit}
        />
      </div>
      <footer className="status-bar">
        <span>{current?.fileLabel ?? "未打开文件"}</span>
        <span>
          {current
            ? `未解决 ${unresolved} / 冲突 ${conflictCount(current.hunks)}`
            : session.message}
        </span>
        <span>{notice ?? "MergeDrag v0.2"}</span>
      </footer>
    </div>
  );
}