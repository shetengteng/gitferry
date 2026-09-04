use std::path::{Path, PathBuf};

use crate::config::{HostKind, RemoteInfo, RepoKind};

/// `.git` 可能是目录（普通仓库）或文件（worktree：`gitdir: <path>`）。
pub fn gitdir_for(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let content = std::fs::read_to_string(&dot_git).ok()?;
    let target = content.trim().strip_prefix("gitdir:")?.trim();
    let gitdir = PathBuf::from(target);
    if gitdir.is_absolute() {
        Some(gitdir)
    } else {
        Some(repo_root.join(gitdir))
    }
}

/// 解析 `<gitdir>/config` 中 `[remote "<name>"] url = ...` 条目。
/// 每个 url 生成一条 RemoteInfo（多 url remote 生成多条，同名）。
pub fn parse_git_config(path: &Path) -> Vec<RemoteInfo> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut remotes = Vec::new();
    let mut current_remote: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') {
            current_remote = parse_section_remote_name(line);
            continue;
        }
        let (Some(name), Some((key, value))) = (current_remote.as_ref(), line.split_once('='))
        else {
            continue;
        };
        if key.trim() == "url" {
            let url = value.trim();
            remotes.push(RemoteInfo {
                name: name.clone(),
                url: url.to_string(),
                host: classify_url(url),
            });
        }
    }
    remotes
}

/// `[remote "origin"]` → Some("origin")；其他节 → None。
fn parse_section_remote_name(line: &str) -> Option<String> {
    let rest = line.trim_start_matches('[').trim_end_matches(']').trim();
    let name = rest
        .strip_prefix("remote")?
        .trim()
        .trim_matches('"')
        .trim_matches('\'');
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// 从 remote url 提取 host：支持 `https://[user@]host[:port]/...`、`ssh://...` 与 scp-like `git@host:path`。
pub fn url_host(url: &str) -> Option<&str> {
    const SCHEMES: [&str; 4] = ["https://", "http://", "ssh://", "git://"];
    for scheme in SCHEMES {
        if let Some(rest) = url.strip_prefix(scheme) {
            let authority = rest.split(['/', '?', '#']).next()?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            return Some(host.split(':').next()?);
        }
    }
    // scp-like：git@github.com:owner/repo.git
    if let Some((_, host_and_path)) = url.split_once('@') {
        let host = host_and_path.split(':').next()?;
        if !host.is_empty() {
            return Some(host);
        }
    }
    None
}

pub fn classify_url(url: &str) -> HostKind {
    match url_host(url) {
        Some(host) if host == "github.com" || host.ends_with(".github.com") => HostKind::Github,
        Some(host) if host == "gitee.com" || host.ends_with(".gitee.com") => HostKind::Gitee,
        Some(_) => HostKind::Other,
        None => HostKind::Other,
    }
}

pub fn kind_for(remotes: &[RemoteInfo]) -> RepoKind {
    let has_github = remotes.iter().any(|r| r.host == HostKind::Github);
    let has_gitee = remotes.iter().any(|r| r.host == HostKind::Gitee);
    match (has_github, has_gitee) {
        (true, true) => RepoKind::Both,
        (true, false) => RepoKind::GithubOnly,
        (false, true) => RepoKind::GiteeOnly,
        (false, false) => RepoKind::None,
    }
}

pub fn inspect_repo(repo_root: &Path) -> Vec<RemoteInfo> {
    match gitdir_for(repo_root) {
        Some(gitdir) => parse_git_config(&gitdir.join("config")),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(url: &str) -> HostKind {
        classify_url(url)
    }

    #[test]
    fn classifies_common_url_forms() {
        assert_eq!(info("git@github.com:owner/repo.git"), HostKind::Github);
        assert_eq!(info("https://github.com/owner/repo.git"), HostKind::Github);
        assert_eq!(info("https://user@github.com/owner/repo"), HostKind::Github);
        assert_eq!(
            info("ssh://git@ssh.github.com:443/owner/repo.git"),
            HostKind::Github
        );
        assert_eq!(info("git@gitee.com:owner/repo.git"), HostKind::Gitee);
        assert_eq!(info("https://gitee.com/owner/repo"), HostKind::Gitee);
        assert_eq!(info("https://gitlab.com/owner/repo.git"), HostKind::Other);
        assert_eq!(info("/local/path/only"), HostKind::Other);
        assert_eq!(info(""), HostKind::Other);
    }

    #[test]
    fn url_host_handles_ports_and_users() {
        assert_eq!(url_host("https://github.com/a/b"), Some("github.com"));
        assert_eq!(
            url_host("https://user@github.com:8443/a/b"),
            Some("github.com")
        );
        assert_eq!(url_host("ssh://git@github.com/a/b"), Some("github.com"));
        assert_eq!(
            url_host("git@gitee.com:22/owner/repo.git"),
            Some("gitee.com")
        );
        assert_eq!(url_host("file:///tmp/x"), None);
    }

    #[test]
    fn parses_config_with_remotes_and_ignores_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config");
        std::fs::write(
            &path,
            r#"
[core]
    repositoryformatversion = 0
[remote "origin"]
    url = git@github.com:owner/repo.git
    fetch = +refs/heads/*:refs/remotes/origin/*
[remote "gitee"]
    url = https://gitee.com/owner/repo.git ; trailing comment
[branch "main"]
    remote = origin
"#,
        )
        .unwrap();
        let remotes = parse_git_config(&path);
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].host, HostKind::Github);
        assert_eq!(remotes[1].name, "gitee");
        assert_eq!(remotes[1].host, HostKind::Gitee);
        assert_eq!(kind_for(&remotes), RepoKind::Both);
    }

    #[test]
    fn gitdir_supports_worktree_file() {
        let dir = tempfile::tempdir().unwrap();
        let worktree_root = dir.path().join("wt");
        std::fs::create_dir_all(&worktree_root).unwrap();
        let real_gitdir = dir.path().join("main/.git/worktrees/wt");
        std::fs::create_dir_all(&real_gitdir).unwrap();
        std::fs::write(
            worktree_root.join(".git"),
            format!("gitdir: {}\n", real_gitdir.display()),
        )
        .unwrap();
        assert_eq!(gitdir_for(&worktree_root), Some(real_gitdir));
    }

    #[test]
    fn missing_git_dir_yields_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(inspect_repo(dir.path()).is_empty());
        assert_eq!(kind_for(&[]), RepoKind::None);
    }
}
