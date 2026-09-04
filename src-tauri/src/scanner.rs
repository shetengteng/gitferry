use std::collections::HashSet;
use std::path::{Path, PathBuf};

const EXCLUDED_DIRS: [&str; 2] = ["node_modules", "vendor"];

/// 在 roots 下发现 git 仓库根（含 `.git` 目录或 worktree `.git` 文件）。
/// 隐藏目录与 node_modules/vendor 整棵跳过；`.git` 命中后不深入其内部；
/// 权限错误等只记日志并跳过；结果去重且保持扫描顺序。
pub fn scan(roots: &[PathBuf], max_depth: u32) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut repos = Vec::new();
    fn push_repo(path: &Path, seen: &mut HashSet<PathBuf>, repos: &mut Vec<PathBuf>) {
        let Some(repo_root) = path.parent().map(Path::to_path_buf) else {
            return;
        };
        if seen.insert(repo_root.clone()) {
            repos.push(repo_root);
        }
    }
    for root in roots {
        if !root.is_dir() {
            tracing::warn!(root = %root.display(), "扫描根不存在，跳过");
            continue;
        }
        let mut it = walkdir::WalkDir::new(root)
            // max_depth 语义：仓库根在扫描根下 N 层目录内；.git 本身再深一层
            .max_depth(max_depth as usize + 1)
            .follow_links(false)
            .into_iter();
        while let Some(next) = it.next() {
            let Ok(entry) = next else { continue };
            let name = entry.file_name().to_string_lossy();
            let is_dir = entry.file_type().is_dir();
            if name == ".git" {
                push_repo(entry.path(), &mut seen, &mut repos);
                if is_dir {
                    it.skip_current_dir();
                }
                continue;
            }
            if is_dir
                && entry.depth() > 0
                && (name.starts_with('.') || EXCLUDED_DIRS.contains(&name.as_ref()))
            {
                it.skip_current_dir();
            }
        }
    }
    repos
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkdir(p: &Path) {
        std::fs::create_dir_all(p).unwrap();
    }

    #[test]
    fn finds_repos_at_various_shapes() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path();
        let plain = base.join("a/repo1");
        mkdir(&plain.join(".git"));
        let worktree = base.join("b/repo2");
        mkdir(&worktree);
        std::fs::write(worktree.join(".git"), "gitdir: /elsewhere\n").unwrap();
        let nested = base.join("c/owner/deep/repo3");
        mkdir(&nested.join(".git"));
        mkdir(&base.join("d/node_modules/pkg/.git"));
        mkdir(&base.join("d/.hidden/repo4/.git"));
        mkdir(&base.join("d/vendor/lib/.git"));

        let mut found = scan(&[base.to_path_buf()], 4);
        found.sort();
        let mut expected = vec![plain, worktree, nested];
        expected.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn respects_max_depth() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path();
        let deep = base.join("l1/l2/l3/repo");
        mkdir(&deep.join(".git"));
        assert!(scan(&[base.to_path_buf()], 3).is_empty());
        assert_eq!(scan(&[base.to_path_buf()], 4), vec![deep]);
    }

    #[test]
    fn dedupes_and_skips_missing_roots() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("repo");
        mkdir(&repo.join(".git"));
        let repo_path = repo.clone();
        let found = scan(
            &[
                repo_path.clone(),
                repo_path,
                root.path().join("does-not-exist"),
            ],
            4,
        );
        assert_eq!(found, vec![repo]);
    }
}
