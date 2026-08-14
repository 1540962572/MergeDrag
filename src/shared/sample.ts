import type { ConflictFile, MergeDocument } from "./types";

/** Sample conflict usable in dev mode (in a plain browser, outside Tauri). */
export const SAMPLE_DOCUMENT: MergeDocument = {
  fileLabel: "src/example.rs",
  hunks: [
    { kind: "Clean", text: "fn greet(name: &str) {\n    println!(\"hello\");\n" },
    {
      kind: "Conflict",
      id: 1,
      local: "    println!(\"local: {name}\");\n",
      remote: "    println!(\"remote: {name}\");\n",
      base: "    println!(\"{name}\");\n",
      decision: { kind: "Unresolved" },
    },
    { kind: "Clean", text: "}\n" },
  ],
};

export const SAMPLE_FILES: ConflictFile[] = [
  { path: "src/example.rs", isBinary: false, unresolvedCount: 1 },
];