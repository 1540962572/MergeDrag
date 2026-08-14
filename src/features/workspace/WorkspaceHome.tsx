import type { RecentWorkspace } from "../../shared/types";

interface WorkspaceHomeProps {
  recent: RecentWorkspace[];
  busy: boolean;
  notice: string | null;
  onOpenFolder: () => void;
  onOpenRecent: (path: string) => void;
  onOpenSample: () => void;
}

function basename(p: string): string {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts.at(-1) ?? p;
}

function ago(seconds: number): string {
  if (!seconds) return "";
  const diff = Math.floor(Date.now() / 1000) - seconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  return `${Math.ceil(diff / (86400 * 30))} 个月前`;
}

/** 空壳启动（未通过 git mergetool 调用）时的首页。 */
export function WorkspaceHome({
  recent,
  busy,
  notice,
  onOpenFolder,
  onOpenRecent,
  onOpenSample,
}: WorkspaceHomeProps) {
  return (
    <div className="home-pane">
      <div className="home-card">
        <h2>打开一个 Git 工作区</h2>
        <p>
          选择包含 <code>.git</code> 的文件夹，即可查看分支状态、拉取、推送，
          并在冲突时用三栏视图逐块解决。
        </p>
        <div className="home-actions">
          <button type="button" onClick={onOpenFolder} disabled={busy}>
            {busy ? "打开中…" : "选择文件夹…"}
          </button>
          <button type="button" className="secondary" onClick={onOpenSample}>
            或打开示例冲突
          </button>
        </div>
        {notice && <div className="home-notice">{notice}</div>}
      </div>

      {recent.length > 0 && (
        <div className="home-recent">
          <h3>最近打开</h3>
          <ul>
            {recent.map((w) => (
              <li
                key={w.path}
                onClick={() => onOpenRecent(w.path)}
                title={w.path}
              >
                <span className="ws-folder">
                  📁 {basename(w.path)}
                  <span className="ws-recent-path">{w.path}</span>
                </span>
                <span className="home-recent-ago">{ago(w.lastOpened)}</span>
              </li>
            ))}
          </ul>
          <p className="home-hint">
            提示：在 Git 仓库里运行 <code>git mergetool</code> 也可以直接打开
            MergeDrag 解决当前合并冲突。
          </p>
        </div>
      )}
    </div>
  );
}