//! 工作区后端集成测试：本地 bare 远端做 pull/push，验证
//! `repo_status`（分支/上游/领先落后/合并中/冲突数）与 `pull`/`push`。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use git_bridge::{pull, push, repo_status};

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "init.defaultBranch=master",
            "-c",
            "core.autocrlf=false",
            "-C",
        ])
        .arg(dir)
        .args(args)
        .output()
        .expect("需要 git 可执行文件在 PATH 上")
}

fn run_ok(dir: &Path, args: &[&str]) -> String {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, msg: &str) {
    run_ok(
        dir,
        &[
            "-c",
            "user.name=md",
            "-c",
            "user.email=md@test",
            "commit",
            "--allow-empty",
            "-m",
            msg,
        ],
    );
}

/// origin（bare 远端） + master 分支克隆 work -> (tmpdir, origin, work)
struct Fixture {
    _tmp: tempfile::TempDir,
    origin: PathBuf,
    work: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    let origin = root.join("origin.git");
    run_ok(root, &["init", "--bare", &origin.to_string_lossy()]);

    let work = root.join("work");
    run_ok(
        root,
        &["clone", &origin.to_string_lossy(), &work.to_string_lossy()],
    );
    commit(&work, "初始提交");
    run_ok(&work, &["push", "origin", "master"]);

    Fixture {
        _tmp: tmp,
        origin,
        work,
    }
}

#[test]
fn repo_status_reports_synced_branch() {
    let fx = fixture();

    let st = repo_status(&fx.work).unwrap();
    assert_eq!(&st.root, &fx.work, "根应为工作树目录");
    assert_eq!(st.branch.as_deref(), Some("master"));
    assert!(!st.merging, "初始状态不应在合并中");
    assert_eq!(st.unmerged_count, 0);
    let up = st.upstream.expect("clone 后应有上游");
    assert_eq!(up.name, "origin/master");
    assert_eq!(up.ahead, 0);
    assert_eq!(up.behind, 0);
}

#[test]
fn ahead_after_local_commit_untracked_pull_ok() {
    let fx = fixture();

    commit(&fx.work, "本地领先提交");
    let st = repo_status(&fx.work).unwrap();
    let up = st.upstream.unwrap();
    assert_eq!(up.ahead, 1, "提交未推送应领先 1");
    assert_eq!(up.behind, 0);

    // 推送后回同步。
    let msg = push(&fx.work).unwrap();
    assert!(msg.contains("master"), "push 输出应包含分支：{msg}");
    let st2 = repo_status(&fx.work).unwrap();
    assert_eq!(st2.upstream.unwrap().ahead, 0, "推送后不再领先");
}

#[test]
fn behind_and_pull_merges_remote_commits() {
    let fx = fixture();

    // 另一个克隆提交并推送 → work 落后 1。
    let other = fx._tmp.path().join("other");
    run_ok(
        fx._tmp.path(),
        &[
            "clone",
            &fx.origin.to_string_lossy(),
            &other.to_string_lossy(),
        ],
    );
    commit(&other, "远端提交");
    run_ok(&other, &["push", "origin", "master"]);

    // 落后数相对「上次 fetch 到的远端」而言，先 fetch 刷新远端跟踪引用。
    git_bridge::fetch(&fx.work).unwrap();

    let st = repo_status(&fx.work).unwrap();
    let up = st.upstream.unwrap();
    assert_eq!(up.behind, 1, "远端有新提交未拉取");
    assert_eq!(up.ahead, 0);

    // pull 拉取远端提交，回到同步。（不断言 "Fast-forward" 文本，避免受 git 本地化影响）
    pull(&fx.work).unwrap();
    let st2 = repo_status(&fx.work).unwrap();
    let up2 = st2.upstream.unwrap();
    assert_eq!(up2.behind, 0, "pull 后不再落后");
    assert!(
        run_ok(&fx.work, &["log", "-1", "--format=%s"]).contains("远端提交"),
        "pull 应带入远端提交"
    );
}

#[test]
fn merging_and_conflict_count_detected() {
    let fx = fixture();

    // 双方各改一个文件后再合并 → 冲突。
    std::fs::write(fx.work.join("data.txt"), "a\nWORK\nc\n").unwrap();
    run_ok(&fx.work, &["add", "data.txt"]);
    commit(&fx.work, "work 改 data");

    let other = fx._tmp.path().join("other");
    run_ok(
        fx._tmp.path(),
        &[
            "clone",
            &fx.origin.to_string_lossy(),
            &other.to_string_lossy(),
        ],
    );
    std::fs::write(other.join("data.txt"), "a\nOTHER\nc\n").unwrap();
    run_ok(&other, &["add", "data.txt"]);
    commit(&other, "other 改 data");
    run_ok(&other, &["push", "origin", "master"]);

    run_ok(&fx.work, &["fetch", "origin"]);
    let merged = git(&fx.work, &["merge", "origin/master"]);
    assert!(!merged.status.success(), "双边改同一行应冲突");

    let st = repo_status(&fx.work).unwrap();
    assert!(st.merging, "MERGE_HEAD 存在即合并中");
    assert_eq!(st.unmerged_count, 1);
    assert!(st.summary.contains("合并中"), "summary: {}", st.summary);
    assert!(st.summary.contains("1 个冲突"), "summary: {}", st.summary);
}
