use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::FerryResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    GithubToGitee,
    GiteeToGithub,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    Github,
    Gitee,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoKind {
    GithubOnly,
    GiteeOnly,
    Both,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
    pub host: HostKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub enabled: bool,
    pub direction: Direction,
    pub remotes: Vec<RemoteInfo>,
    pub kind: RepoKind,
    /// 镜像模式：沿同步方向删除 dst 侧 src 已不存在的分支/tag（用户显式开启）
    #[serde(default)]
    pub mirror_delete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub scan_roots: Vec<PathBuf>,
    pub max_depth: u32,
    /// 自动同步轮询间隔（分钟），调度器（M3）消费
    #[serde(default = "default_sync_interval_mins")]
    pub sync_interval_mins: u32,
    /// 并发同步仓库数上限，调度器（M3）消费
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
}

fn default_sync_interval_mins() -> u32 {
    10
}

fn default_concurrency() -> u32 {
    2
}

impl Default for Settings {
    fn default() -> Self {
        let home = dirs::home_dir();
        let candidates: &[&str] = &["Projects", "dev", "code", "Documents"];
        let scan_roots = candidates
            .iter()
            .filter_map(|c| home.as_ref().map(|h| h.join(c)))
            .filter(|p| p.is_dir())
            .collect();
        Self {
            scan_roots,
            max_depth: 4,
            sync_interval_mins: default_sync_interval_mins(),
            concurrency: default_concurrency(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Unconfigured,
    Connected,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub status: AccountStatus,
    pub login: Option<String>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            status: AccountStatus::Unconfigured,
            login: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Accounts {
    pub github: Account,
    pub gitee: Account,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub settings: Settings,
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
    #[serde(default)]
    pub accounts: Accounts,
    #[serde(default)]
    pub setup_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            repos: Vec::new(),
            accounts: Accounts::default(),
            setup_done: false,
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join("gitferry/config.json")
}

impl Config {
    pub fn load() -> FerryResult<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read(&path)?;
        let config: Self = serde_json::from_slice(&raw)?;
        Ok(config)
    }

    pub fn save(&self) -> FerryResult<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_serde_roundtrip() {
        for d in [
            Direction::GithubToGitee,
            Direction::GiteeToGithub,
            Direction::Both,
        ] {
            let json = serde_json::to_string(&d).unwrap();
            let parsed: Direction = serde_json::from_str(&json).unwrap();
            assert_eq!(d, parsed);
        }
        assert_eq!(serde_json::to_string(&Direction::Both).unwrap(), "\"both\"");
    }

    #[test]
    fn config_roundtrip_keeps_unknown_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = Config {
            repos: vec![RepoEntry {
                path: PathBuf::from("/tmp/repo"),
                enabled: true,
                direction: Direction::GithubToGitee,
                remotes: vec![RemoteInfo {
                    name: "origin".into(),
                    url: "git@github.com:a/b.git".into(),
                    host: HostKind::Github,
                }],
                kind: RepoKind::GithubOnly,
                mirror_delete: false,
            }],
            accounts: Accounts::default(),
            setup_done: true,
            settings: Settings::default(),
        };
        std::fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        let raw = std::fs::read(&path).unwrap();
        let parsed: Config = serde_json::from_slice(&raw).unwrap();
        assert!(parsed.setup_done);
        assert_eq!(parsed.repos.len(), 1);
        assert_eq!(parsed.repos[0].direction, Direction::GithubToGitee);
    }

    #[test]
    fn mirror_delete_serde_default() {
        // 旧配置 JSON 无 mirror_delete 字段 → 反序列化为 false
        let legacy = r#"{
            "path": "/tmp/repo",
            "enabled": true,
            "direction": "both",
            "remotes": [],
            "kind": "both"
        }"#;
        let parsed: RepoEntry = serde_json::from_str(legacy).unwrap();
        assert!(!parsed.mirror_delete);

        // 含 true 的 roundtrip 保留 true
        let entry = RepoEntry {
            path: PathBuf::from("/tmp/repo"),
            enabled: true,
            direction: Direction::Both,
            remotes: Vec::new(),
            kind: RepoKind::Both,
            mirror_delete: true,
        };
        let raw = serde_json::to_string(&entry).unwrap();
        let roundtrip: RepoEntry = serde_json::from_str(&raw).unwrap();
        assert!(roundtrip.mirror_delete);
    }

    #[test]
    fn default_settings_skip_missing_roots() {
        let settings = Settings::default();
        assert!(settings.scan_roots.iter().all(|p| p.is_dir()));
        assert_eq!(settings.max_depth, 4);
    }

    #[test]
    fn config_path_contains_app_dir() {
        assert!(config_path().ends_with("gitferry/config.json"));
    }
}
