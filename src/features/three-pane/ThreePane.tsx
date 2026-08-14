import Editor, { type OnMount } from "@monaco-editor/react";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  paneText,
  type Decision,
  type MergeDocument,
  type Hunk,
} from "../../shared/types";

interface ThreePaneProps {
  document: MergeDocument | null;
  resultText: string;
  onDecide: (hunkId: number, decision: Decision) => void;
  onAcceptAll: (side: "local" | "remote") => void;
  onManualEdit: (text: string) => void;
}

type MonacoEditor = Parameters<OnMount>[0];
type MonacoApi = Parameters<OnMount>[1];

const monacoOptions = {
  minimap: { enabled: false },
  fontSize: 13,
  wordWrap: "on" as const,
  scrollBeyondLastLine: false,
  automaticLayout: true,
  renderLineHighlight: "all" as const,
};

/** Count lines in a hunk text (trailing `\n` does not add a line). */
function lineCount(text: string): number {
  if (text === "") return 0;
  return text.endsWith("\n")
    ? text.split("\n").length - 1
    : text.split("\n").length;
}

/**
 * Line ranges (0-based) of unresolved conflict blocks in the Result pane.
 * Unresolved blocks render three marker lines + both sides' content.
 */
function unresolvedBlocks(
  hunks: Hunk[],
): { id: number; start: number; end: number }[] {
  const blocks: { id: number; start: number; end: number }[] = [];
  let line = 0;
  for (const h of hunks) {
    if (h.kind === "Clean") {
      line += lineCount(h.text);
    } else {
      const blockLines = lineCount(h.local) + lineCount(h.remote) + 3;
      const resolved =
        h.decision.kind !== "Unresolved" && h.decision.kind !== "Manual";
      if (!resolved) {
        blocks.push({ id: h.id, start: line, end: line + blockLines - 1 });
      }
      line += blockLines;
    }
  }
  return blocks;
}

/** Base 面板文本：Clean 段取原文本，冲突段取 base（无 base 则标注）。 */
function basePaneText(hunks: Hunk[]): string {
  return hunks
    .map((h) => {
      if (h.kind === "Clean") return h.text;
      return h.base ?? "(无共同祖先，无法显示 Base)\n";
    })
    .join("");
}

export function ThreePane({
  document,
  resultText,
  onDecide,
  onAcceptAll,
  onManualEdit,
}: ThreePaneProps) {
  const [baseVisible, setBaseVisible] = useState(false);
  const [activeIdx, setActiveIdx] = useState(0);
  const resultRef = useRef<MonacoEditor | null>(null);
  const monacoRef = useRef<MonacoApi | null>(null);
  const decoRef = useRef<string[]>([]);

  const blocks = useMemo(
    () => (document ? unresolvedBlocks(document.hunks) : []),
    [document],
  );
  const nUnresolved = blocks.length;

  const goTo = (index: number) => {
    if (nUnresolved === 0) return;
    const idx = ((index % nUnresolved) + nUnresolved) % nUnresolved;
    setActiveIdx(idx);
    const ed = resultRef.current;
    const b = blocks[idx];
    if (ed && b) {
      ed.revealLineInCenter(b.start + 1);
      ed.focus();
    }
  };

  // 切换文件/会话时回到第一块。
  useEffect(() => {
    setActiveIdx(0);
  }, [document]);

  // Alt+↑ / Alt+↓：跳上一处/下一处冲突。
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.altKey) return;
      if (e.key === "ArrowUp") {
        e.preventDefault();
        goTo(activeIdx - 1);
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        goTo(activeIdx + 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // 未解决块高亮：红底靠边块 + 活动块下划线。
  useEffect(() => {
    const ed = resultRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco || blocks.length === 0) {
      if (decoRef.current.length > 0) {
        ed?.deltaDecorations(decoRef.current, []);
        decoRef.current = [];
      }
      return;
    }
    const decorations = blocks.map((b, i) => ({
      range: new monaco.Range(b.start + 1, 1, b.end + 1, 1),
      options: {
        isWholeLine: true,
        className:
          i === activeIdx % blocks.length
            ? "md-block-active"
            : "md-block-unresolved",
      },
    }));
    decoRef.current = ed.deltaDecorations(decoRef.current, decorations);
  }, [blocks, activeIdx, resultText, document]);

  if (!document) {
    return (
      <div className="empty-state">
        尚未打开冲突文件。
        <br />
        请通过 <code>git mergetool</code> 启动，或点击右上角「打开示例冲突」。
      </div>
    );
  }

  const local = paneText(document.hunks, "local");
  const remote = paneText(document.hunks, "remote");
  const base = basePaneText(document.hunks);
  const currentBlock = nUnresolved > 0 ? blocks[activeIdx % nUnresolved] : null;

  return (
    <div className="three-pane">
      <div className="merge-toolbar">
        <button
          type="button"
          className={baseVisible ? "toolbar-btn active" : "toolbar-btn"}
          onClick={() => setBaseVisible((v) => !v)}
          title="显示/隐藏 Base 面板"
        >
          显示 Base
        </button>
        <span className="toolbar-sep" />
        <button
          type="button"
          className="toolbar-btn"
          disabled={nUnresolved === 0}
          onClick={() => goTo(activeIdx - 1)}
          title="上一处冲突 (Alt+↑)"
        >
          ↑ 上一块
        </button>
        <button
          type="button"
          className="toolbar-btn"
          disabled={nUnresolved === 0}
          onClick={() => goTo(activeIdx + 1)}
          title="下一处冲突 (Alt+↓)"
        >
          ↓ 下一块
        </button>
        <span className="block-counter">
          {nUnresolved > 0
            ? `${(activeIdx % nUnresolved) + 1} / ${nUnresolved}`
            : "无冲突"}
        </span>
        <span className="toolbar-sep" />
        <button
          type="button"
          className="toolbar-btn"
          disabled={nUnresolved === 0}
          onClick={() => onAcceptAll("local")}
          title="所有未解决块都取左侧"
        >
          全部取左
        </button>
        <button
          type="button"
          className="toolbar-btn"
          disabled={nUnresolved === 0}
          onClick={() => onAcceptAll("remote")}
          title="所有未解决块都取右侧"
        >
          全部取右
        </button>
      </div>

      {currentBlock && (
        <div className="block-bar">
          当前未解决块：第 {currentBlock.start + 1}–{currentBlock.end + 1} 行
          <span className="block-actions">
            <button
              type="button"
              className="mini"
              onClick={() => onDecide(currentBlock.id, { kind: "TakeLocal" })}
              title="接受左侧"
            >
              ← 取左
            </button>
            <button
              type="button"
              className="mini"
              onClick={() => onDecide(currentBlock.id, { kind: "TakeRemote" })}
              title="接受右侧"
            >
              取右 →
            </button>
            <button
              type="button"
              className="mini"
              onClick={() =>
                onDecide(currentBlock.id, {
                  kind: "TakeBoth",
                  payload: { localFirst: true },
                })
              }
              title="双方都取（先左后右）"
            >
              双方
            </button>
            <button
              type="button"
              className="mini"
              onClick={() => onDecide(currentBlock.id, { kind: "Ignore" })}
              title="忽略该块"
            >
              ✕ 忽略
            </button>
          </span>
        </div>
      )}

      <div className={baseVisible ? "pane-grid four" : "pane-grid three"}>
        <section className="pane">
          <div className="pane-title">Local</div>
          <div className="pane-body">
            <Editor
              language="plaintext"
              theme="vs-dark"
              value={local}
              options={{ ...monacoOptions, readOnly: true }}
            />
          </div>
        </section>
        {baseVisible && (
          <section className="pane">
            <div className="pane-title">Base</div>
            <div className="pane-body">
              <Editor
                language="plaintext"
                theme="vs-dark"
                value={base}
                options={{ ...monacoOptions, readOnly: true }}
              />
            </div>
          </section>
        )}
        <section className="pane">
          <div className="pane-title">Result</div>
          <div className="pane-body">
            <Editor
              language="plaintext"
              theme="vs-dark"
              value={resultText}
              onMount={(ed, monaco) => {
                resultRef.current = ed;
                monacoRef.current = monaco;
              }}
              onChange={(v) => {
                if (v !== undefined) onManualEdit(v);
              }}
              options={{ ...monacoOptions, readOnly: false }}
            />
          </div>
        </section>
        <section className="pane">
          <div className="pane-title">Remote</div>
          <div className="pane-body">
            <Editor
              language="plaintext"
              theme="vs-dark"
              value={remote}
              options={{ ...monacoOptions, readOnly: true }}
            />
          </div>
        </section>
      </div>
    </div>
  );
}