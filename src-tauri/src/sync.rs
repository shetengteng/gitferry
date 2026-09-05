//! 同步引擎：按设计文档 §4.3 对单个仓库做 fast-forward 同步与冲突检测。
//!
//! 安全边界（safety.md）：只推显式 refspec、自动路径不带 --force、不删 ref、
//! 不改工作区（唯一例外 `git pull --ff-only`）；分叉停下标记 conflict 等用户裁决。

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::{Direction, HostKind, RepoEntry};
use crate::error::{FerryError, FerryResult};

// ---------- serde 契约（与前端 src/lib/types.ts 对齐，字段 snake_case） ----------

/// 仓库同步状态：pending → syncing → synced / conflict / error / missing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStatus {
    #[default]
    Pending,
    Syncing,
    Synced,
    Conflict,
    Error,
    Missing,
}

/// 冲突 ref 类型：分支或 tag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Branch,
    Tag,
}

/// 一条未裁决的 ref 冲突（分叉分支 / 漂移 tag），sha 为 7 位短 sha
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefConflict {
    pub kind: RefKind,
    pub name: String,
    pub github_sha: String,
    pub gitee_sha: String,
}

/// 单仓库同步状态（运行时内存态，不落盘）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoSyncState {
    pub status: SyncStatus,
    pub last_synced: Option<i64>,
    pub pushed_refs: u32,
    /// 本次同步沿镜像模式删除的 ref 数
    #[serde(default)]
    pub deleted_refs: u32,
    pub error: Option<String>,
    pub conflicts: Vec<RefConflict>,
}

/// sync_all / get_sync_states 返回的单仓库结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSyncResult {
    pub path: String,
    pub state: RepoSyncState,
}

/// 冲突裁决的权威侧（用户三选一后的显式选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSide {
    Github,
    Gitee,
}

// ---------- 内部模型 ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Github,
    Gitee,
}

impl Side {
    fn label(self) -> &'static str {
        match self {
            Side::Github => "github",
            Side::Gitee => "gitee",
        }
    }
}

/// 一轮同步方向：src 领先时向 dst 传播
#[derive(Debug, Clone, Copy)]
struct Round {
    src: Side,
    dst: Side,
}

struct Remotes {
    github: String,
    gitee: String,
}

impl Remotes {
    fn of(&self, side: Side) -> &str {
        match side {
            Side::Github => &self.github,
            Side::Gitee => &self.gitee,
        }
    }
}

/// 同步结果累积器
#[derive(Default)]
struct Acc {
    pushed: u32,
    /// 镜像模式删除的 ref 数
    deleted: u32,
    conflicts: Vec<RefConflict>,
    errors: Vec<String>,
    /// 本地分支与远端分叉（只置状态，不进 conflicts：该分歧无法由两端对齐解决）
    diverged: bool,
}

/// 远端 tag 引用：ref_sha 为 refs/tags/<name> 直接指向（annotated tag 为 tag 对象），
/// peeled 为解引用后的提交 sha（轻量 tag 两者相同）。
struct TagRef {
    ref_sha: String,
    peeled: String,
}

/// push_explicit 的冲突记录上下文
struct RefCtx<'a> {
    kind: RefKind,
    name: &'a str,
    src_side: Side,
    dst_side: Side,
    /// 比较时已知的对端 sha；push 被拒（竞态）时用于记录冲突
    dst_sha: Option<&'a str>,
}

// ---------- 公共 API ----------

/// 对单个仓库执行一轮同步：fetch 两端 → 本地三方比较 → 逐方向远端 ref 对齐。
/// 全程只做 fast-forward，分叉记入 conflicts 等待用户裁决。
pub fn sync_repo(entry: &RepoEntry) -> RepoSyncState {
    let started = Instant::now();
    let root = entry.path.clone();
    if !root.is_dir() {
        return RepoSyncState {
            status: SyncStatus::Missing,
            ..Default::default()
        };
    }
    // 任何方向都必须同时具备 github 与 gitee remote
    let (github, gitee) = (
        remote_for(entry, Side::Github),
        remote_for(entry, Side::Gitee),
    );
    let remotes = match (github, gitee) {
        (Some(g), Some(t)) => Remotes {
            github: g.to_string(),
            gitee: t.to_string(),
        },
        (github, gitee) => {
            let mut missing: Vec<&str> = Vec::new();
            if github.is_none() {
                missing.push(Side::Github.label());
            }
            if gitee.is_none() {
                missing.push(Side::Gitee.label());
            }
            let msg = format!(
                "该仓库没有 {} remote，无法按当前方向同步",
                missing.join("、")
            );
            tracing::warn!(path = %root.display(), "{msg}");
            return RepoSyncState {
                status: SyncStatus::Error,
                error: Some(msg),
                ..Default::default()
            };
        }
    };
    let mut acc = Acc::default();
    // 1. fetch 所有涉及的 remote 更新 remote-tracking refs；失败则本轮终止，避免基于过期数据判定
    for side in [Side::Github, Side::Gitee] {
        let remote = remotes.of(side);
        match run_git(&root, &["fetch", remote, "--prune"]) {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                acc.errors
                    .push(classify_git_error(remote, &stderr_of(&out)));
                return finish(acc, started, &root);
            }
            Err(err) => {
                acc.errors.push(err.to_string());
                return finish(acc, started, &root);
            }
        }
    }
    let rounds = rounds_for(entry.direction);
    // 2. 本地三方比较（设计 §5「本地未推送提交」）：本地领先先 push 到其 origin，
    //    随后的远端轮次会把该提交继续传播到另一端，单次同步即完成端到端对齐。
    local_three_way(&root, &rounds, &remotes, &mut acc);
    // 3. 逐方向远端对齐：分支 fast-forward 同步 + tag 补推 / 漂移检测
    for round in rounds {
        round_refs(&root, round, &remotes, &mut acc, entry.mirror_delete);
    }
    finish(acc, started, &root)
}

/// 用户显式裁决冲突后，按权威侧 force 对齐两端 remote。
/// 这是全项目唯一允许 `--force` 的路径，调用前 UI 必须已取得用户确认。
pub fn resolve_conflicts(
    entry: &RepoEntry,
    conflicts: &[RefConflict],
    side: ConflictSide,
) -> FerryResult<u32> {
    let root = &entry.path;
    if !root.is_dir() {
        return Err(FerryError::msg(format!(
            "仓库路径不存在：{}，无法裁决冲突",
            root.display()
        )));
    }
    let github = remote_for(entry, Side::Github).ok_or_else(|| {
        FerryError::msg("该仓库没有 github remote，无法裁决冲突。请重新扫描后再试。")
    })?;
    let gitee = remote_for(entry, Side::Gitee).ok_or_else(|| {
        FerryError::msg("该仓库没有 gitee remote，无法裁决冲突。请重新扫描后再试。")
    })?;
    let mut pushed = 0u32;
    for conflict in conflicts {
        // side=Github → 以 github_sha 对齐到 gitee；side=Gitee → 反之
        let (target, sha, dst) = match (conflict.kind, side) {
            (RefKind::Branch, ConflictSide::Github) => (
                format!("refs/heads/{}", conflict.name),
                &conflict.github_sha,
                gitee,
            ),
            (RefKind::Branch, ConflictSide::Gitee) => (
                format!("refs/heads/{}", conflict.name),
                &conflict.gitee_sha,
                github,
            ),
            (RefKind::Tag, ConflictSide::Github) => (
                format!("refs/tags/{}", conflict.name),
                &conflict.github_sha,
                gitee,
            ),
            (RefKind::Tag, ConflictSide::Gitee) => (
                format!("refs/tags/{}", conflict.name),
                &conflict.gitee_sha,
                github,
            ),
        };
        tracing::warn!(path = %root.display(), target = %target, dst, "用户裁决冲突，强制对齐远端");
        let out = run_git(root, &["push", "--force", dst, &format!("{sha}:{target}")])?;
        if !out.status.success() {
            return Err(FerryError::msg(classify_git_error(dst, &stderr_of(&out))));
        }
        pushed += 1;
    }
    Ok(pushed)
}

// ---------- 引擎内部 ----------

fn rounds_for(direction: Direction) -> Vec<Round> {
    match direction {
        Direction::GithubToGitee => vec![Round {
            src: Side::Github,
            dst: Side::Gitee,
        }],
        Direction::GiteeToGithub => vec![Round {
            src: Side::Gitee,
            dst: Side::Github,
        }],
        Direction::Both => vec![
            Round {
                src: Side::Github,
                dst: Side::Gitee,
            },
            Round {
                src: Side::Gitee,
                dst: Side::Github,
            },
        ],
    }
}

/// 按平台归属找 remote 名（同平台多个 remote 取第一个）。
fn remote_for(entry: &RepoEntry, side: Side) -> Option<&str> {
    let host = match side {
        Side::Github => HostKind::Github,
        Side::Gitee => HostKind::Gitee,
    };
    entry
        .remotes
        .iter()
        .find(|r| r.host == host)
        .map(|r| r.name.as_str())
}

/// 本地 checkout 分支与各启用方向的 src remote 三方比较：
/// 相等 → 无操作；本地领先 → push；本地落后 → `git pull --ff-only`；分叉 → conflict 告警。
/// detached HEAD（空输出）或 unborn 分支跳过。
fn local_three_way(root: &Path, rounds: &[Round], remotes: &Remotes, acc: &mut Acc) {
    let branch = match git_stdout(root, &["branch", "--show-current"]) {
        Ok(b) if !b.is_empty() => b,
        _ => return,
    };
    let local_sha = match git_stdout(root, &["rev-parse", &format!("refs/heads/{branch}")]) {
        Ok(sha) => sha,
        Err(_) => return,
    };
    let mut sides: Vec<Side> = Vec::new();
    for round in rounds {
        if !sides.contains(&round.src) {
            sides.push(round.src);
        }
    }
    for side in sides {
        let remote = remotes.of(side);
        let remote_sha = git_stdout(
            root,
            &["rev-parse", &format!("refs/remotes/{remote}/{branch}")],
        )
        .ok();
        match remote_sha {
            // 远端没有该分支 → 本地领先，推送创建
            None => push_local_branch(root, remote, &branch, acc),
            Some(rt) if rt == local_sha => {}
            Some(rt) => {
                match (
                    is_ancestor(root, &rt, &local_sha),
                    is_ancestor(root, &local_sha, &rt),
                ) {
                    // 远端落后 → fast-forward push
                    (Some(true), _) => push_local_branch(root, remote, &branch, acc),
                    // 本地落后 → pull --ff-only（safety.md 唯一允许的改工作区操作）
                    (_, Some(true)) => pull_ff_only(root, remote, &branch, acc),
                    // 分叉 → 停下告警，不自动选边
                    (Some(false), Some(false)) => mark_local_diverged(remote, &branch, acc),
                    // merge-base 不确定：宁可不操作
                    _ => tracing::warn!(
                        path = %root.display(),
                        branch = %branch,
                        remote,
                        "merge-base 判定失败，跳过本地分支处理"
                    ),
                }
            }
        }
    }
}

/// 本地分支与远端分叉：置 conflict 状态并给出可操作提示（优先级高于领先/落后）。
fn mark_local_diverged(remote: &str, branch: &str, acc: &mut Acc) {
    acc.diverged = true;
    let msg = format!("本地分支 {branch} 与 {remote} 分叉，请手动合并后重试");
    if !acc.errors.contains(&msg) {
        acc.errors.push(msg);
    }
}

/// 推送本地分支到远端（fast-forward、显式 refspec、无 --force）。
fn push_local_branch(root: &Path, remote: &str, branch: &str, acc: &mut Acc) {
    let full = format!("refs/heads/{branch}");
    match run_git(root, &["push", remote, &format!("{full}:{full}")]) {
        Ok(out) if out.status.success() => {
            if !stderr_of(&out).contains("Everything up-to-date") {
                acc.pushed += 1;
            }
        }
        Ok(out) => {
            let stderr = stderr_of(&out);
            if stderr.contains("[rejected]") {
                mark_local_diverged(remote, branch, acc);
            } else {
                acc.errors.push(classify_git_error(remote, &stderr));
            }
        }
        Err(err) => acc.errors.push(err.to_string()),
    }
}

/// 本地落后：`git pull --ff-only`。若拉取时发现分叉（竞态）按 conflict 处理。
fn pull_ff_only(root: &Path, remote: &str, branch: &str, acc: &mut Acc) {
    match run_git(
        root,
        &["pull", "--ff-only", remote, &format!("refs/heads/{branch}")],
    ) {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = stderr_of(&out);
            if stderr.contains("Not possible to fast-forward") {
                mark_local_diverged(remote, branch, acc);
            } else {
                acc.errors.push(classify_git_error(remote, &stderr));
            }
        }
        Err(err) => acc.errors.push(err.to_string()),
    }
}

/// 单方向远端对齐：逐分支 fast-forward 同步 + tag 补推 / 漂移检测。
/// mirror_delete 为 true 时，额外删除 dst 侧 src 已不存在的分支/tag（用户显式开启的镜像模式）。
fn round_refs(root: &Path, round: Round, remotes: &Remotes, acc: &mut Acc, mirror_delete: bool) {
    let (src, dst) = (remotes.of(round.src), remotes.of(round.dst));
    let src_branches = match list_remote_branches(root, src) {
        Ok(m) => m,
        Err(msg) => {
            acc.errors.push(msg);
            return;
        }
    };
    let dst_branches = match list_remote_branches(root, dst) {
        Ok(m) => m,
        Err(msg) => {
            acc.errors.push(msg);
            return;
        }
    };
    for (branch, src_sha) in &src_branches {
        let target = format!("refs/heads/{branch}");
        match dst_branches.get(branch) {
            // dst 缺分支 → push 创建
            None => push_explicit(
                root,
                dst,
                src_sha,
                &target,
                RefCtx {
                    kind: RefKind::Branch,
                    name: branch,
                    src_side: round.src,
                    dst_side: round.dst,
                    dst_sha: None,
                },
                acc,
            ),
            Some(dst_sha) if dst_sha == src_sha => {}
            Some(dst_sha) => {
                match (
                    is_ancestor(root, dst_sha, src_sha),
                    is_ancestor(root, src_sha, dst_sha),
                ) {
                    // dst 是 src 祖先 → fast-forward push
                    (Some(true), _) => push_explicit(
                        root,
                        dst,
                        src_sha,
                        &target,
                        RefCtx {
                            kind: RefKind::Branch,
                            name: branch,
                            src_side: round.src,
                            dst_side: round.dst,
                            dst_sha: Some(dst_sha),
                        },
                        acc,
                    ),
                    // src 是 dst 祖先 → dst 已包含 src，无操作
                    (_, Some(true)) => {}
                    // 分叉 → 标记 conflict，跳过该分支
                    (Some(false), Some(false)) => {
                        add_conflict(acc, RefKind::Branch, branch, round.src, src_sha, dst_sha)
                    }
                    // merge-base 不确定：宁可不推
                    _ => tracing::warn!(
                        path = %root.display(),
                        branch = %branch,
                        "merge-base 判定失败，跳过该分支"
                    ),
                }
            }
        }
    }
    // 镜像模式：dst 侧存在而 src 确认不存在的分支 → 沿 round 方向单向删除。
    // src/dst 列表任一获取失败时已在上方提前 return，不会在 Err 路径做删除。
    if mirror_delete {
        for branch in dst_branches.keys() {
            if !src_branches.contains_key(branch) {
                push_delete(
                    root,
                    dst,
                    &format!("refs/heads/{branch}"),
                    RefKind::Branch,
                    branch,
                    acc,
                );
            }
        }
    }
    // tag：本地 refs/tags 命名空间无法区分来自哪端（两端 tag 混在同一命名空间，
    // auto-follow 也不覆盖已有 tag），故用 ls-remote 分别读取两端真实 tag refs。
    // 以 peeled sha 比较「指向」；补推时用 ref sha 以保留 annotated tag 对象。
    let src_tags = match ls_remote_tags(root, src) {
        Ok(m) => m,
        Err(msg) => {
            acc.errors.push(msg);
            return;
        }
    };
    let dst_tags = match ls_remote_tags(root, dst) {
        Ok(m) => m,
        Err(msg) => {
            acc.errors.push(msg);
            return;
        }
    };
    for (tag, tag_ref) in &src_tags {
        let target = format!("refs/tags/{tag}");
        match dst_tags.get(tag) {
            // dst 缺失 → 补推
            None => push_explicit(
                root,
                dst,
                &tag_ref.ref_sha,
                &target,
                RefCtx {
                    kind: RefKind::Tag,
                    name: tag,
                    src_side: round.src,
                    dst_side: round.dst,
                    dst_sha: None,
                },
                acc,
            ),
            // 指向相同 → 跳过
            Some(dst_tag) if dst_tag.peeled == tag_ref.peeled => {}
            // 指向不同 → 漂移冲突，不覆盖
            Some(dst_tag) => add_conflict(
                acc,
                RefKind::Tag,
                tag,
                round.src,
                &tag_ref.ref_sha,
                &dst_tag.ref_sha,
            ),
        }
    }
    // 镜像模式：dst 侧存在而 src 确认不存在的 tag → 单向删除
    if mirror_delete {
        for tag in dst_tags.keys() {
            if !src_tags.contains_key(tag) {
                push_delete(
                    root,
                    dst,
                    &format!("refs/tags/{tag}"),
                    RefKind::Tag,
                    tag,
                    acc,
                );
            }
        }
    }
}

/// 推送显式 refspec（`<sha>:<target>`，无 --force），按输出判定结果。
fn push_explicit(
    root: &Path,
    remote: &str,
    sha: &str,
    target: &str,
    ctx: RefCtx<'_>,
    acc: &mut Acc,
) {
    match run_git(root, &["push", remote, &format!("{sha}:{target}")]) {
        Ok(out) if out.status.success() => {
            if !stderr_of(&out).contains("Everything up-to-date") {
                acc.pushed += 1;
            }
        }
        Ok(out) => {
            let stderr = stderr_of(&out);
            // fetch 之后远端又变化（竞态）导致 non-fast-forward 被拒：按冲突处理，等用户裁决
            if stderr.contains("[rejected]") && ctx.kind == RefKind::Branch {
                if let Some(dst_sha) = ctx.dst_sha {
                    add_conflict(acc, ctx.kind, ctx.name, ctx.src_side, sha, dst_sha);
                } else {
                    acc.diverged = true;
                    acc.errors.push(format!(
                        "{} 的 {} push 被拒：远端已变化，请重新同步",
                        remote, ctx.name
                    ));
                }
            } else {
                acc.errors.push(classify_git_error(remote, &stderr));
            }
        }
        Err(err) => acc.errors.push(err.to_string()),
    }
}

/// 镜像模式：删除 dst 侧 src 已不存在的 ref（显式 refspec，绝不 --mirror 裸推）。
/// 删除失败按 error 处理，不静默。
fn push_delete(root: &Path, remote: &str, target: &str, kind: RefKind, name: &str, acc: &mut Acc) {
    match run_git(root, &["push", remote, "--delete", target]) {
        Ok(out) if out.status.success() => {
            acc.deleted += 1;
            tracing::info!(
                path = %root.display(),
                remote,
                kind = ?kind,
                name,
                "镜像模式删除远端 ref"
            );
        }
        Ok(out) => {
            acc.errors
                .push(classify_git_error(remote, &stderr_of(&out)));
        }
        Err(err) => acc.errors.push(err.to_string()),
    }
}

/// 记录一条分叉/漂移冲突（同 ref 去重），并把 src/dst sha 映射到平台侧短 sha。
fn add_conflict(
    acc: &mut Acc,
    kind: RefKind,
    name: &str,
    src_side: Side,
    src_sha: &str,
    dst_sha: &str,
) {
    if acc
        .conflicts
        .iter()
        .any(|c| c.kind == kind && c.name == name)
    {
        return;
    }
    let (github_sha, gitee_sha) = match src_side {
        Side::Github => (src_sha, dst_sha),
        Side::Gitee => (dst_sha, src_sha),
    };
    acc.conflicts.push(RefConflict {
        kind,
        name: name.to_string(),
        github_sha: short_sha(github_sha),
        gitee_sha: short_sha(gitee_sha),
    });
}

/// 汇总本轮结果：conflict 优先于 error，全部干净才记 synced + 时间戳。
fn finish(acc: Acc, started: Instant, root: &Path) -> RepoSyncState {
    let status = if !acc.conflicts.is_empty() || acc.diverged {
        SyncStatus::Conflict
    } else if !acc.errors.is_empty() {
        SyncStatus::Error
    } else {
        SyncStatus::Synced
    };
    let last_synced = if status == SyncStatus::Synced {
        Some(now_unix())
    } else {
        None
    };
    let error = if acc.errors.is_empty() {
        None
    } else {
        Some(acc.errors.join("；"))
    };
    let state = RepoSyncState {
        status,
        last_synced,
        pushed_refs: acc.pushed,
        deleted_refs: acc.deleted,
        error,
        conflicts: acc.conflicts,
    };
    tracing::info!(
        path = %root.display(),
        status = ?state.status,
        pushed = state.pushed_refs,
        deleted = state.deleted_refs,
        conflicts = state.conflicts.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "同步完成"
    );
    state
}

// ---------- git CLI 基础设施 ----------

/// 在仓库根执行 git 子命令。GIT_TERMINAL_PROMPT=0 避免无凭据时阻塞等待输入。
fn run_git(root: &Path, args: &[&str]) -> Result<Output, FerryError> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|err| FerryError::msg(format!("git 命令启动失败：{err}。请确认系统已安装 git。")))
}

/// 执行 git 子命令并返回 trim 后的 stdout；失败时返回带 stderr 摘录的错误。
fn git_stdout(root: &Path, args: &[&str]) -> FerryResult<String> {
    let out = run_git(root, args)?;
    if !out.status.success() {
        return Err(FerryError::msg(format!(
            "git {} 执行失败：{}",
            args.join(" "),
            stderr_tail(&stderr_of(&out))
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git merge-base --is-ancestor a b`：Some(true)=a 是 b 祖先（含相等）。
/// 退出码非 0/1 视为不确定，返回 None（调用方宁可不推）。
fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Option<bool> {
    match run_git(root, &["merge-base", "--is-ancestor", ancestor, descendant]) {
        Ok(out) => match out.status.code() {
            Some(0) => Some(true),
            Some(1) => Some(false),
            _ => None,
        },
        Err(_) => None,
    }
}

/// 列出远端分支（remote-tracking refs）→ { 分支名: sha }，跳过 `<remote>/HEAD`。
fn list_remote_branches(root: &Path, remote: &str) -> Result<HashMap<String, String>, String> {
    let out = run_git(
        root,
        &[
            "for-each-ref",
            &format!("refs/remotes/{remote}/"),
            "--format=%(refname:short) %(objectname)",
        ],
    )
    .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(classify_git_error(remote, &stderr_of(&out)));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let prefix = format!("{remote}/");
    let mut map = HashMap::new();
    for line in stdout.lines() {
        let line = line.trim();
        let Some((short, sha)) = line.rsplit_once(' ') else {
            continue;
        };
        let Some(name) = short.strip_prefix(prefix.as_str()) else {
            continue;
        };
        if name.is_empty() || name == "HEAD" || name.ends_with("/HEAD") {
            continue;
        }
        map.insert(name.to_string(), sha.to_string());
    }
    Ok(map)
}

/// `git ls-remote --tags` 读取远端 tag：peeled 行覆盖同名条目，保证
/// annotated tag 记录的是提交 sha；ref_sha 保留原始指向用于补推。
fn ls_remote_tags(root: &Path, remote: &str) -> Result<HashMap<String, TagRef>, String> {
    let out = run_git(root, &["ls-remote", "--tags", remote]).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(classify_git_error(remote, &stderr_of(&out)));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut map: HashMap<String, TagRef> = HashMap::new();
    for line in stdout.lines() {
        let mut parts = line.split_whitespace();
        let (Some(sha), Some(refname)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Some(name) = refname.strip_prefix("refs/tags/") else {
            continue;
        };
        if let Some(base) = name.strip_suffix("^{}") {
            map.entry(base.to_string())
                .and_modify(|t| t.peeled = sha.to_string())
                .or_insert_with(|| TagRef {
                    ref_sha: sha.to_string(),
                    peeled: sha.to_string(),
                });
        } else {
            map.entry(name.to_string()).or_insert_with(|| TagRef {
                ref_sha: sha.to_string(),
                peeled: sha.to_string(),
            });
        }
    }
    Ok(map)
}

/// git stderr → 用户可读中文错误（附原始信息摘录）。
fn classify_git_error(remote: &str, stderr: &str) -> String {
    const AUTH: [&str; 4] = [
        "Authentication failed",
        "could not read Username",
        "Permission denied (publickey)",
        "terminal prompts disabled",
    ];
    if AUTH.iter().any(|k| stderr.contains(k)) {
        return format!("{remote} 操作失败：git 凭据不可用。请在终端对该仓库执行一次 git fetch/push 保存凭据，或在设置页配置 PAT。");
    }
    const NETWORK: [&str; 3] = ["Could not resolve host", "timed out", "Connection"];
    if NETWORK.iter().any(|k| stderr.contains(k)) {
        return format!("{remote} 操作失败：网络不可达，请检查网络后重试。");
    }
    format!("{remote} 操作失败：{}", stderr_tail(stderr))
}

/// 取 stderr 最后 3 行非空内容作为原始信息摘录，单行超长截断。
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let start = lines.len().saturating_sub(3);
    lines[start..]
        .iter()
        .map(|l| {
            if l.chars().count() > 200 {
                format!("{}…", l.chars().take(200).collect::<String>())
            } else {
                (*l).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("；")
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------- 测试：tempfile + 本地 bare 仓库伪造两端 ----------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::config::{RemoteInfo, RepoKind};

    /// 两端 bare 仓库 + 工作仓库的测试环境
    struct TestEnv {
        dir: tempfile::TempDir,
        work: PathBuf,
        github: PathBuf,
        gitee: PathBuf,
    }

    fn git(cwd: &Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@test")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@test")
            .output()
            .unwrap()
    }

    fn git_ok(cwd: &Path, args: &[&str]) {
        let out = git(cwd, args);
        assert!(
            out.status.success(),
            "git {args:?} 执行失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn sha_at(repo: &Path, refname: &str) -> String {
        let out = git(repo, &["rev-parse", refname]);
        assert!(
            out.status.success(),
            "rev-parse {refname} 失败：{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// 读 bare 仓库中的 ref（不存在返回 None）
    fn bare_sha(bare: &Path, refname: &str) -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(bare)
            .args(["rev-parse", "--verify", refname])
            .output()
            .unwrap();
        if out.status.success() {
            Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            None
        }
    }

    fn push_main_to(repo: &Path, remote: &str) {
        git_ok(repo, &["push", remote, "refs/heads/main:refs/heads/main"]);
    }

    fn setup() -> TestEnv {
        let dir = tempfile::tempdir().unwrap();
        let github = dir.path().join("github.git");
        let gitee = dir.path().join("gitee.git");
        git_ok(dir.path(), &["init", "--bare", github.to_str().unwrap()]);
        git_ok(dir.path(), &["init", "--bare", gitee.to_str().unwrap()]);
        // 未配置 init.defaultBranch 的环境默认 master，统一指到 main
        git_ok(&github, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git_ok(&gitee, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        let work = dir.path().join("work");
        git_ok(dir.path(), &["init", "-b", "main", work.to_str().unwrap()]);
        git_ok(
            &work,
            &["remote", "add", "github", github.to_str().unwrap()],
        );
        git_ok(&work, &["remote", "add", "gitee", gitee.to_str().unwrap()]);
        git_ok(&work, &["commit", "--allow-empty", "-m", "c1"]);
        push_main_to(&work, "github");
        push_main_to(&work, "gitee");
        TestEnv {
            dir,
            work,
            github,
            gitee,
        }
    }

    fn entry(env: &TestEnv, direction: Direction) -> RepoEntry {
        RepoEntry {
            path: env.work.clone(),
            enabled: true,
            direction,
            remotes: vec![
                RemoteInfo {
                    name: "github".into(),
                    url: env.github.to_string_lossy().into_owned(),
                    host: HostKind::Github,
                },
                RemoteInfo {
                    name: "gitee".into(),
                    url: env.gitee.to_string_lossy().into_owned(),
                    host: HostKind::Gitee,
                },
            ],
            kind: RepoKind::Both,
            mirror_delete: false,
        }
    }

    /// 在远端 bare 仓库伪造第三方提交：clone → commit → push，返回新提交 sha。
    /// 用于构造「不经过本地工作仓库」的远端变化。
    fn push_foreign_commit(
        env: &TestEnv,
        bare: &Path,
        agent: &str,
        branch: &str,
        msg: &str,
    ) -> String {
        let agent_dir = env.dir.path().join(agent);
        git_ok(
            env.dir.path(),
            &["clone", bare.to_str().unwrap(), agent_dir.to_str().unwrap()],
        );
        if branch != "main" {
            git_ok(&agent_dir, &["switch", "-c", branch]);
        }
        git_ok(&agent_dir, &["commit", "--allow-empty", "-m", msg]);
        git_ok(
            &agent_dir,
            &[
                "push",
                "origin",
                &format!("refs/heads/{branch}:refs/heads/{branch}"),
            ],
        );
        sha_at(&agent_dir, "HEAD")
    }

    #[test]
    fn dst_missing_branch_gets_created() {
        let env = setup();
        git_ok(&env.work, &["switch", "-c", "feature"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "f1"]);
        let f1 = sha_at(&env.work, "HEAD");
        git_ok(
            &env.work,
            &["push", "github", "refs/heads/feature:refs/heads/feature"],
        );
        git_ok(&env.work, &["switch", "main"]);

        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 1);
        assert!(state.conflicts.is_empty());
        assert!(state.last_synced.is_some());
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/feature").as_deref(),
            Some(f1.as_str())
        );
    }

    #[test]
    fn dst_behind_fast_forwards() {
        let env = setup();
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "c2"]);
        let c2 = sha_at(&env.work, "HEAD");
        push_main_to(&env.work, "github");

        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 1);
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/main").as_deref(),
            Some(c2.as_str())
        );
    }

    #[test]
    fn src_ancestor_of_dst_skips_push() {
        let env = setup();
        let c1 = bare_sha(&env.github, "refs/heads/main").unwrap();
        // gitee 侧（非本地）前进一个提交：src(github) 是 dst(gitee) 祖先
        push_foreign_commit(&env, &env.gitee, "agent-gitee", "main", "c2");

        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 0);
        // github 端未被改写
        assert_eq!(
            bare_sha(&env.github, "refs/heads/main").as_deref(),
            Some(c1.as_str())
        );
    }

    #[test]
    fn diverged_branch_reports_conflict_without_touching_ends() {
        let env = setup();
        // github 侧 feature 前进（本地保持在 main）
        git_ok(&env.work, &["switch", "-c", "feature"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "f1"]);
        let f1 = sha_at(&env.work, "HEAD");
        git_ok(
            &env.work,
            &["push", "github", "refs/heads/feature:refs/heads/feature"],
        );
        git_ok(&env.work, &["switch", "main"]);
        // gitee 侧 feature 另行前进
        let f2 = push_foreign_commit(&env, &env.gitee, "agent-gitee", "feature", "f2");

        let state = sync_repo(&entry(&env, Direction::Both));
        assert_eq!(state.status, SyncStatus::Conflict);
        // 两轮方向检测到同一分叉，去重后只记一条
        assert_eq!(state.conflicts.len(), 1);
        let conflict = &state.conflicts[0];
        assert_eq!(conflict.kind, RefKind::Branch);
        assert_eq!(conflict.name, "feature");
        assert_eq!(conflict.github_sha, short_sha(&f1));
        assert_eq!(conflict.gitee_sha, short_sha(&f2));
        // 两端均未被改写
        assert_eq!(
            bare_sha(&env.github, "refs/heads/feature").as_deref(),
            Some(f1.as_str())
        );
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/feature").as_deref(),
            Some(f2.as_str())
        );
    }

    #[test]
    fn missing_tag_pushed_and_drifted_tag_rejected() {
        // tag 缺失 → 补推
        let env = setup();
        git_ok(&env.work, &["tag", "v1.0"]);
        git_ok(
            &env.work,
            &["push", "github", "refs/tags/v1.0:refs/tags/v1.0"],
        );
        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        let c1 = bare_sha(&env.github, "refs/tags/v1.0").unwrap();
        assert_eq!(
            bare_sha(&env.gitee, "refs/tags/v1.0").as_deref(),
            Some(c1.as_str())
        );

        // tag 漂移：gitee 侧 v1.0 指向不同提交 → 拒推记冲突
        let env2 = setup();
        git_ok(&env2.work, &["tag", "v1.0"]);
        git_ok(
            &env2.work,
            &["push", "github", "refs/tags/v1.0:refs/tags/v1.0"],
        );
        let c1 = bare_sha(&env2.github, "refs/tags/v1.0").unwrap();
        let agent = env2.dir.path().join("agent-gitee");
        git_ok(
            env2.dir.path(),
            &[
                "clone",
                env2.gitee.to_str().unwrap(),
                agent.to_str().unwrap(),
            ],
        );
        git_ok(&agent, &["commit", "--allow-empty", "-m", "c2"]);
        git_ok(&agent, &["tag", "v1.0"]);
        git_ok(
            &agent,
            &["push", "origin", "refs/heads/main:refs/heads/main"],
        );
        git_ok(&agent, &["push", "origin", "refs/tags/v1.0:refs/tags/v1.0"]);
        let c2 = sha_at(&agent, "HEAD");

        let state = sync_repo(&entry(&env2, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Conflict);
        assert_eq!(state.conflicts.len(), 1);
        assert_eq!(state.conflicts[0].kind, RefKind::Tag);
        assert_eq!(state.conflicts[0].name, "v1.0");
        assert_eq!(state.conflicts[0].github_sha, short_sha(&c1));
        assert_eq!(state.conflicts[0].gitee_sha, short_sha(&c2));
        // 两端 tag 均未被覆盖
        assert_eq!(
            bare_sha(&env2.github, "refs/tags/v1.0").as_deref(),
            Some(c1.as_str())
        );
        assert_eq!(
            bare_sha(&env2.gitee, "refs/tags/v1.0").as_deref(),
            Some(c2.as_str())
        );
    }

    #[test]
    fn missing_repo_path_reports_missing() {
        let dir = tempfile::tempdir().unwrap();
        let entry = RepoEntry {
            path: dir.path().join("gone"),
            enabled: true,
            direction: Direction::Both,
            remotes: Vec::new(),
            kind: RepoKind::None,
            mirror_delete: false,
        };
        let state = sync_repo(&entry);
        assert_eq!(state.status, SyncStatus::Missing);
        assert!(state.error.is_none());
    }

    #[test]
    fn local_ahead_pushes_and_local_behind_pulls_ff_only() {
        // 本地领先：提交未推送 → 先推 github，再由远端轮次传播到 gitee
        let env = setup();
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "c2"]);
        let c2 = sha_at(&env.work, "HEAD");
        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 2);
        assert_eq!(
            bare_sha(&env.github, "refs/heads/main").as_deref(),
            Some(c2.as_str())
        );
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/main").as_deref(),
            Some(c2.as_str())
        );

        // 本地落后：远端有新提交 → pull --ff-only 拉平
        let env = setup();
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "c2"]);
        push_main_to(&env.work, "github");
        push_main_to(&env.work, "gitee");
        let c2 = sha_at(&env.work, "HEAD");
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "c3"]);
        push_main_to(&env.work, "github");
        let c3 = sha_at(&env.work, "HEAD");
        // 测试侧构造「本地落后」（引擎自身不使用 reset）
        git_ok(&env.work, &["reset", "--hard", &c2]);

        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 1);
        assert_eq!(sha_at(&env.work, "refs/heads/main"), c3);
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/main").as_deref(),
            Some(c3.as_str())
        );
    }

    #[test]
    fn direction_github_to_gitee_never_pushes_back_to_github() {
        let env = setup();
        let c1 = bare_sha(&env.github, "refs/heads/main").unwrap();
        // gitee 侧（非本地）新提交
        push_foreign_commit(&env, &env.gitee, "agent-gitee", "main", "c2");

        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.pushed_refs, 0);
        // github 端未被回推
        assert_eq!(
            bare_sha(&env.github, "refs/heads/main").as_deref(),
            Some(c1.as_str())
        );
        // 本地也不从 gitee 拉取（gitee 不是该方向的 src）
        assert_eq!(sha_at(&env.work, "refs/heads/main"), c1);
    }

    #[test]
    fn resolve_conflict_force_aligns_chosen_side() {
        let env = setup();
        git_ok(&env.work, &["switch", "-c", "feature"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "f1"]);
        let f1 = sha_at(&env.work, "HEAD");
        git_ok(
            &env.work,
            &["push", "github", "refs/heads/feature:refs/heads/feature"],
        );
        git_ok(&env.work, &["switch", "main"]);
        push_foreign_commit(&env, &env.gitee, "agent-gitee", "feature", "f2");

        let state = sync_repo(&entry(&env, Direction::Both));
        assert_eq!(state.status, SyncStatus::Conflict);

        // 用户裁决：以 github 为准 → gitee 的 feature 被强制对齐到 f1
        let pushed = resolve_conflicts(
            &entry(&env, Direction::Both),
            &state.conflicts,
            ConflictSide::Github,
        )
        .unwrap();
        assert_eq!(pushed, 1);
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/feature").as_deref(),
            Some(f1.as_str())
        );

        // 复合同步：冲突消除
        let state = sync_repo(&entry(&env, Direction::Both));
        assert_eq!(state.status, SyncStatus::Synced);
        assert!(state.conflicts.is_empty());
        assert_eq!(
            bare_sha(&env.github, "refs/heads/feature").as_deref(),
            Some(f1.as_str())
        );
    }

    #[test]
    fn mirror_delete_propagates_branch_deletion() {
        let env = setup();
        git_ok(&env.work, &["switch", "-c", "feature"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "f1"]);
        git_ok(
            &env.work,
            &["push", "github", "refs/heads/feature:refs/heads/feature"],
        );
        git_ok(&env.work, &["switch", "main"]);
        // 先同步一次，让 gitee 也有 feature
        let state = sync_repo(&entry(&env, Direction::Both));
        assert_eq!(state.status, SyncStatus::Synced);
        assert!(bare_sha(&env.gitee, "refs/heads/feature").is_some());

        // 从 github 删除 feature（模拟上游删分支）
        git_ok(&env.github, &["update-ref", "-d", "refs/heads/feature"]);

        let mut e = entry(&env, Direction::Both);
        e.mirror_delete = true;
        let state = sync_repo(&e);
        assert_eq!(state.status, SyncStatus::Synced);
        assert!(state.conflicts.is_empty());
        assert!(state.deleted_refs >= 1);
        assert!(bare_sha(&env.github, "refs/heads/feature").is_none());
        assert!(bare_sha(&env.gitee, "refs/heads/feature").is_none());
    }

    #[test]
    fn mirror_delete_propagates_tag_deletion() {
        let env = setup();
        git_ok(&env.work, &["tag", "v1"]);
        git_ok(&env.work, &["push", "github", "refs/tags/v1:refs/tags/v1"]);
        // 先同步一次，让 gitee 也有 v1
        let state = sync_repo(&entry(&env, Direction::GithubToGitee));
        assert_eq!(state.status, SyncStatus::Synced);
        assert!(bare_sha(&env.gitee, "refs/tags/v1").is_some());

        // 从 github 删除 v1（模拟上游删 tag）
        git_ok(&env.github, &["update-ref", "-d", "refs/tags/v1"]);

        let mut e = entry(&env, Direction::Both);
        e.mirror_delete = true;
        let state = sync_repo(&e);
        assert_eq!(state.status, SyncStatus::Synced);
        assert!(state.deleted_refs >= 1);
        assert!(bare_sha(&env.github, "refs/tags/v1").is_none());
        assert!(bare_sha(&env.gitee, "refs/tags/v1").is_none());
    }

    #[test]
    fn mirror_delete_disabled_keeps_refs() {
        let env = setup();
        git_ok(&env.work, &["switch", "-c", "feature"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "f1"]);
        git_ok(
            &env.work,
            &["push", "github", "refs/heads/feature:refs/heads/feature"],
        );
        git_ok(&env.work, &["switch", "main"]);
        let state = sync_repo(&entry(&env, Direction::Both));
        assert_eq!(state.status, SyncStatus::Synced);

        git_ok(&env.github, &["update-ref", "-d", "refs/heads/feature"]);

        // mirror_delete 默认 false：gitee 侧 feature 保留
        let e = entry(&env, Direction::Both);
        assert!(!e.mirror_delete);
        let state = sync_repo(&e);
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.deleted_refs, 0);
        assert!(bare_sha(&env.gitee, "refs/heads/feature").is_some());
    }

    #[test]
    fn mirror_delete_both_directions_symmetric() {
        let env = setup();
        // github 独有分支 a，gitee 独有分支 b
        git_ok(&env.work, &["switch", "-c", "a"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "a1"]);
        git_ok(&env.work, &["push", "github", "refs/heads/a:refs/heads/a"]);
        git_ok(&env.work, &["switch", "-c", "b"]);
        git_ok(&env.work, &["commit", "--allow-empty", "-m", "b1"]);
        git_ok(&env.work, &["push", "gitee", "refs/heads/b:refs/heads/b"]);
        git_ok(&env.work, &["switch", "main"]);

        let mut e = entry(&env, Direction::Both);
        e.mirror_delete = true;

        // 第一次同步：github→gitee 轮传播 a、删除 gitee 独有的 b。
        // git push 会同步更新 remote-tracking ref，反向轮基于最新状态、无需再删。
        let state = sync_repo(&e);
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.deleted_refs, 1);
        assert!(bare_sha(&env.gitee, "refs/heads/b").is_none());
        assert_eq!(
            bare_sha(&env.gitee, "refs/heads/a"),
            bare_sha(&env.github, "refs/heads/a")
        );

        // 从 github 删除 a → 第二次同步沿 github→gitee 方向把删除传播到 gitee
        git_ok(&env.github, &["update-ref", "-d", "refs/heads/a"]);
        let state = sync_repo(&e);
        assert_eq!(state.status, SyncStatus::Synced);
        assert_eq!(state.deleted_refs, 1);
        // 对称收敛：两端 bare 均只剩 main
        assert!(bare_sha(&env.github, "refs/heads/a").is_none());
        assert!(bare_sha(&env.gitee, "refs/heads/a").is_none());
        assert!(bare_sha(&env.github, "refs/heads/main").is_some());
        assert!(bare_sha(&env.gitee, "refs/heads/main").is_some());
    }

    #[test]
    fn classifies_git_errors_to_actionable_messages() {
        let auth = classify_git_error(
            "github",
            "fatal: Authentication failed for 'https://github.com/owner/repo.git/'",
        );
        assert!(auth.contains("凭据不可用"));
        assert!(auth.contains("github"));
        assert!(auth.contains("PAT"));

        let net = classify_git_error(
            "gitee",
            "fatal: unable to access 'https://gitee.com/': Could not resolve host: gitee.com",
        );
        assert!(net.contains("网络不可达"));

        let other = classify_git_error("gitee", "warning: a\nfatal: bad object refs/heads/x");
        assert!(other.contains("gitee 操作失败"));
        assert!(other.contains("bad object"));
    }

    #[test]
    fn serde_snake_case_roundtrip() {
        assert_eq!(
            serde_json::to_string(&SyncStatus::Synced).unwrap(),
            "\"synced\""
        );
        assert_eq!(
            serde_json::to_string(&SyncStatus::Missing).unwrap(),
            "\"missing\""
        );
        assert_eq!(
            serde_json::to_string(&RefKind::Branch).unwrap(),
            "\"branch\""
        );
        assert_eq!(
            serde_json::to_string(&ConflictSide::Gitee).unwrap(),
            "\"gitee\""
        );
        let state: RepoSyncState = serde_json::from_str(
            r#"{"status":"conflict","last_synced":123,"pushed_refs":2,"error":null,
                "conflicts":[{"kind":"tag","name":"v1","github_sha":"aaaaaaa","gitee_sha":"bbbbbbb"}]}"#,
        )
        .unwrap();
        assert_eq!(state.status, SyncStatus::Conflict);
        assert_eq!(state.pushed_refs, 2);
        assert_eq!(state.deleted_refs, 0);
        assert_eq!(state.conflicts[0].kind, RefKind::Tag);
        assert_eq!(state.conflicts[0].github_sha, "aaaaaaa");
    }
}
