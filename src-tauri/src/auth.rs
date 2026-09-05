use serde::{Deserialize, Serialize};

use crate::config::AccountStatus;
use crate::error::{FerryError, FerryResult};

const SERVICE: &str = "gitferry";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Github,
    Gitee,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitee => "gitee",
        }
    }
}

pub fn set_token(platform: Platform, token: &str) -> FerryResult<()> {
    let entry = keyring::Entry::new(SERVICE, platform.as_str())?;
    entry.set_password(token)?;
    Ok(())
}

/// 从钥匙串读取平台令牌；未设置返回 None。令牌仅瞬时存在，不缓存、不落日志。
pub fn get_token(platform: Platform) -> FerryResult<Option<String>> {
    match keyring::Entry::new(SERVICE, platform.as_str())?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// 平台返回的登录名
    Connected(String),
    Unauthorized,
    Unexpected(String),
}

/// 对平台 API 做一次轻量校验，返回账号登录名。
/// HTTP 细节独立于解析逻辑，便于离线测试。
pub async fn verify(platform: Platform, token: &str) -> VerifyOutcome {
    let (url, auth) = match platform {
        Platform::Github => (
            "https://api.github.com/user".to_string(),
            format!("Bearer {token}"),
        ),
        Platform::Gitee => (
            format!("https://gitee.com/api/v5/user?access_token={token}"),
            String::new(),
        ),
    };
    let client = reqwest::Client::new();
    let mut req = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, "gitferry");
    if !auth.is_empty() {
        req = req.header(reqwest::header::AUTHORIZATION, auth);
    }
    match req.send().await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            parse_verify_response(status, &body)
        }
        // without_url：reqwest 错误会附带完整 URL，Gitee 令牌在 query 中，绝不外泄
        Err(err) => VerifyOutcome::Unexpected(err.without_url().to_string()),
    }
}

pub fn parse_verify_response(status: u16, body: &str) -> VerifyOutcome {
    match status {
        200 => {
            #[derive(Deserialize)]
            struct User {
                login: Option<String>,
            }
            match serde_json::from_str::<User>(body) {
                Ok(user) if user.login.is_some() => VerifyOutcome::Connected(user.login.unwrap()),
                _ => VerifyOutcome::Unexpected("响应缺少 login 字段".to_string()),
            }
        }
        401 => VerifyOutcome::Unauthorized,
        _ => VerifyOutcome::Unexpected(format!("HTTP {status}")),
    }
}

/// 配置账号：写钥匙串 → 验证 → 返回最终状态。验证失败时令牌仍保留，
/// 便于令牌临时失效（如网络）后重试，状态如实反馈。
pub async fn configure(platform: Platform, token: &str) -> FerryResult<crate::config::Account> {
    set_token(platform, token)?;
    Ok(match verify(platform, token).await {
        VerifyOutcome::Connected(login) => crate::config::Account {
            status: AccountStatus::Connected,
            login: Some(login),
        },
        VerifyOutcome::Unauthorized => crate::config::Account {
            status: AccountStatus::Invalid,
            login: None,
        },
        VerifyOutcome::Unexpected(detail) => {
            return Err(FerryError::msg(format!("平台请求失败：{detail}")));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_responses_by_status() {
        assert_eq!(
            parse_verify_response(200, r#"{"login":"shetengteng","id":1}"#),
            VerifyOutcome::Connected("shetengteng".into())
        );
        assert_eq!(
            parse_verify_response(401, r#"{"message":"Bad credentials"}"#),
            VerifyOutcome::Unauthorized
        );
        assert!(matches!(
            parse_verify_response(500, "boom"),
            VerifyOutcome::Unexpected(_)
        ));
        assert!(matches!(
            parse_verify_response(200, r#"{"unexpected":true}"#),
            VerifyOutcome::Unexpected(_)
        ));
    }
}
