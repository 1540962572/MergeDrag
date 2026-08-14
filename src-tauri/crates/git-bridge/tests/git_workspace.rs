//! 工作区后端集成测试：本地 bare 远端做 pull/push，验证
//! `repo_status`（分支/上游/领先落后/合并中/冲突数）与 `pull`/`push`。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use git_bridge::{
    commit_all, create_branch, list_branches, list_stashes, pull, push, repo_status, stash_drop,
    stash_pop, stash_pop_index, stash_push, switch_branch,
};

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
    // 本地身份：commit_all 走不带 -c 的 `git commit`，需要仓库级身份可用。
    run_ok(&work, &["config", "user.name", "md-local"]);
    run_ok(&work, &["config", "user.email", "md-local@test"]);

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

/// 在 work 里落一个已跟踪的 data.txt 基线，供“改已跟踪文件”的场景使用。
fn seed_data(fx: &Fixture) {
    std::fs::write(fx.work.join("data.txt"), "a\nb\nc\n").unwrap();
    run_ok(&fx.work, &["add", "data.txt"]);
    commit(&fx.work, "加 data.txt");
}

#[test]
fn branch_create_switch_and_current_marker() {
    let fx = fixture();

    let branches = list_branches(&fx.work).unwrap();
    assert_eq!(branches.len(), 1, "初始只有 master");
    assert_eq!(branches[0].name, "master");
    assert!(branches[0].current);

    // 空名字直接拒绝。
    assert!(create_branch(&fx.work, "  ").is_err());

    create_branch(&fx.work, "feature").unwrap();
    let branches = list_branches(&fx.work).unwrap();
    assert_eq!(branches.len(), 2);
    let master = branches.iter().find(|b| b.name == "master").unwrap();
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(master.current && !feature.current, "master 仍是当前分支");

    switch_branch(&fx.work, "feature", false).unwrap();
    let branches = list_branches(&fx.work).unwrap();
    assert!(branches.iter().find(|b| b.name == "feature").unwrap().current);
    assert_eq!(
        repo_status(&fx.work).unwrap().branch.as_deref(),
        Some("feature")
    );

    switch_branch(&fx.work, "master", false).unwrap();
    assert_eq!(
        repo_status(&fx.work).unwrap().branch.as_deref(),
        Some("master")
    );
}

#[test]
fn switch_branch_blocks_and_forces_dirty_tree() {
    let fx = fixture();
    seed_data(&fx);

    create_branch(&fx.work, "feature").unwrap();
    switch_branch(&fx.work, "feature", false).unwrap();
    std::fs::write(fx.work.join("data.txt"), "feature 内容\n").unwrap();
    run_ok(&fx.work, &["add", "data.txt"]); // commit() 不带 -a，须先暂存
    commit(&fx.work, "feature 提交");
    switch_branch(&fx.work, "master", false).unwrap();

    // master 工作树有未提交改动，且两分支该文件内容不同 → 普通切换被 git 拒绝。
    std::fs::write(fx.work.join("data.txt"), "master 未提交改动\n").unwrap();
    assert!(
        switch_branch(&fx.work, "feature", false).is_err(),
        "有本地改动时应拒绝切换"
    );
    assert_eq!(
        repo_status(&fx.work).unwrap().branch.as_deref(),
        Some("master"),
        "失败的切换不应改分支"
    );
    assert_eq!(
        std::fs::read_to_string(fx.work.join("data.txt")).unwrap(),
        "master 未提交改动\n",
        "失败的切换不应动工作树"
    );

    // force 切换丢弃改动，工作树取 feature 的文件内容。
    switch_branch(&fx.work, "feature", true).unwrap();
    assert_eq!(
        repo_status(&fx.work).unwrap().branch.as_deref(),
        Some("feature")
    );
    assert_eq!(
        std::fs::read_to_string(fx.work.join("data.txt")).unwrap(),
        "feature 内容\n"
    );
}

#[test]
fn stash_push_list_pop_round_trip() {
    let fx = fixture();
    seed_data(&fx);
    assert!(list_stashes(&fx.work).unwrap().is_empty());

    std::fs::write(fx.work.join("data.txt"), "改动\n").unwrap();
    stash_push(&fx.work, "临时改动").unwrap();

    let stashes = list_stashes(&fx.work).unwrap();
    assert_eq!(stashes.len(), 1);
    assert_eq!(stashes[0].index, 0);
    assert!(
        stashes[0].message.contains("临时改动"),
        "message: {}",
        stashes[0].message
    );
    assert_eq!(
        std::fs::read_to_string(fx.work.join("data.txt")).unwrap(),
        "a\nb\nc\n",
        "stash 后工作树应还原基线"
    );

    // 干净工作树时 stash_push 应报可读错误而非 git 的原始 No local changes。
    std::fs::write(fx.work.join("untracked.txt"), "未跟踪\n").unwrap();
    let err = stash_push(&fx.work, "").unwrap_err();
    assert!(err.to_string().contains("没有可保存"), "err: {err}");

    // pop 恢复。
    stash_pop(&fx.work).unwrap();
    assert!(list_stashes(&fx.work).unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(fx.work.join("data.txt")).unwrap(),
        "改动\n",
        "pop 应恢复改动"
    );
}

#[test]
fn stash_index_pop_and_drop() {
    let fx = fixture();
    seed_data(&fx);

    std::fs::write(fx.work.join("data.txt"), "第一份\n").unwrap();
    stash_push(&fx.work, "第一份改动").unwrap();
    std::fs::write(fx.work.join("data.txt"), "第二份\n").unwrap();
    stash_push(&fx.work, "第二份改动").unwrap();

    let stashes = list_stashes(&fx.work).unwrap();
    assert_eq!(stashes.len(), 2);
    assert!(stashes[0].message.contains("第二份"), "最近的在前面");

    // 按序号恢复较旧一条（index=1）。
    stash_pop_index(&fx.work, 1).unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.work.join("data.txt")).unwrap(),
        "第一份\n"
    );
    let stashes = list_stashes(&fx.work).unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("第二份"));

    // 删除剩下的。
    stash_drop(&fx.work, 0).unwrap();
    assert!(list_stashes(&fx.work).unwrap().is_empty());
}

#[test]
fn commit_all_stages_tracked_and_untracked() {
    let fx = fixture();

    // 既有未跟踪新文件、又有已跟踪改动 → add -A + commit 一次收编。
    std::fs::write(fx.work.join("new.txt"), "新文件\n").unwrap();
    seed_data(&fx); // data.txt 已跟踪
    std::fs::write(fx.work.join("data.txt"), "改过\n").unwrap();
    let head_before = run_ok(&fx.work, &["rev-parse", "HEAD"]);

    let summary = commit_all(&fx.work, "feat: 提交全部").unwrap();
    assert!(summary.contains("feat: 提交全部"), "summary: {summary}");
    assert_ne!(run_ok(&fx.work, &["rev-parse", "HEAD"]), head_before, "HEAD 前进");
    let st = repo_status(&fx.work).unwrap();
    assert!(!st.dirty, "提交后工作树干净");

    // 已跟踪改动与未跟踪文件都进了提交。
    let files = run_ok(&fx.work, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(files.contains("data.txt") && files.contains("new.txt"), "files: {files}");

    // 干净工作树二次提交报错，空信息直接拒绝。
    assert!(commit_all(&fx.work, "没事可提交").is_err());
    let err = commit_all(&fx.work, "   ").unwrap_err();
    assert!(err.to_string().contains("不能为空"), "err: {err}");
}
