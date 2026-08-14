import type { WorkspaceStatus } from "../../shared/types";

interface WorkspaceBarProps {
  status: WorkspaceStatus;
  busy: "idle" | "opening" | "pulling" | "pushing" | "refreshing";
  toast: string | null;
  onRefresh: () => void;
  onPull: () => void;
  onPush: () => void;
  onResolveConflicts: () => void;
}

/** 取路径最后一段作为显示名（Windows/Linux/macOS 分隔符都兼容）。 */
function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? p;
}

export function WorkspaceBar({
  status,
  busy,
  toast,
  onRefresh,
  onPull,
  onPush,
  onResolveConflicts,
}: WorkspaceBarProps) {
  const busyFlag = busy !== "idle";
  const url = status.upstream?.remoteUrl;

  return (
    <div className="workspace-card">
      <div className="ws-row">
        <span className="ws-folder" title={status.root}>
          📁 {basename(status.root)}
        </span>
        <span className="ws-branch" title={`分支 ${status.branch ?? "(detached)"}`}>
          {status.branch ?? "(detached)"}
        </span>
      </div>
      <div className="ws-summary" title={status.root}>
        {status.summary}
      </div>
      {url && (
        <div className="ws-url" title={url}>
          {url.replace(/^https?:\/\//, "")}
        </div>
      )}
      <div className="ws-ops">
        <button
          type="button"
          className="secondary"
          disabled={busyFlag}
          onClick={onRefresh}
          title="fetch 后再读一次状态，更新背后落后数"
        >
          {busy === "refreshing" ? "刷新中…" : "刷新"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={busyFlag}
          onClick={onPull}
          title="git pull --no-edit；有冲突时转入解决冲突"
        >
          {busy === "pulling" ? "拉取中…" : "拉取"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={busyFlag}
          onClick={onPush}
          title="git push"
        >
          {busy === "pushing" ? "推送中…" : "推送"}
        </button>
        {(status.merging || status.unmergedCount > 0) && (
          <button
            type="button"
            onClick={onResolveConflicts}
            title="列出并打开仓库中的冲突文件"
          >
            解决冲突
            {status.unmergedCount > 0 ? ` (${status.unmergedCount})` : ""}
          </button>
        )}
      </div>
      {toast && <div className="ws-toast">{toast}</div>}
    </div>
  );
}