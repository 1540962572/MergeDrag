import type { ConflictFile } from "../../shared/types";

interface FileListProps {
  files: ConflictFile[];
  activePath: string | null;
  onSelect: (path: string) => void;
}

export function FileList({ files, activePath, onSelect }: FileListProps) {
  return (
    <aside className="file-list">
      <h2>冲突文件</h2>
      {files.length === 0 ? (
        <p className="empty-state">
          暂无冲突文件。通过 git mergetool 启动，或打开示例。
        </p>
      ) : (
        <ul>
          {files.map((file) => (
            <li
              key={file.path}
              className={file.path === activePath ? "active" : undefined}
              onClick={() => onSelect(file.path)}
            >
              {file.path}
              {file.isBinary ? (
                <span className="status binary">二进制</span>
              ) : (
                <span
                  className={`status ${
                    file.unresolvedCount === 0 ? "resolved" : "unresolved"
                  }`}
                >
                  {file.unresolvedCount === 0
                    ? "已解决"
                    : `未解决 ${file.unresolvedCount}`}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}