import type { ConflictFile, MergeDocument } from "./types";

/**
 * 示例冲突（3 处相互独立的冲突，用于演示「不同块取左/取右各不同」）：
 * 一个 README 里，简介/安装/使用三个段落两边各改各的。
 */
export function buildSampleDocument(): MergeDocument {
  return {
    fileLabel: "README.md",
    hunks: [
      { kind: "Clean", text: "# MergeDrag\n\n## 简介\n\n" },
      {
        kind: "Conflict",
        id: 1,
        local: "这是一个三方合并工具（本地版）。\n",
        remote: "这是一个三方合并工具（远端版）。\n",
        base: "这是一个 IDEA 风格的三方合并工具。\n",
        decision: { kind: "Unresolved" },
      },
      { kind: "Clean", text: "\n## 安装\n\n" },
      {
        kind: "Conflict",
        id: 2,
        local: "使用 NSIS 安装包。\n",
        remote: "使用 MSI 安装包。\n",
        base: "使用安装包。\n",
        decision: { kind: "Unresolved" },
      },
      { kind: "Clean", text: "\n## 使用\n\n" },
      {
        kind: "Conflict",
        id: 3,
        local: "用 git mergetool 启动（本地）。\n",
        remote: "用 git mergetool 启动（远端）。\n",
        base: "用 git mergetool 启动。\n",
        decision: { kind: "Unresolved" },
      },
      { kind: "Clean", text: "\n## 反馈\n\n欢迎反馈问题。\n" },
    ],
  };
}

export const SAMPLE_DOCUMENT: MergeDocument = buildSampleDocument();

export const SAMPLE_FILES: ConflictFile[] = [
  { path: "README.md", isBinary: false, unresolvedCount: 3 },
];