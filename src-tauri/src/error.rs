use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum FerryError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("配置读写失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("钥匙串访问失败: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("{0}")]
    Message(String),
}

impl FerryError {
    pub fn msg(m: impl Into<String>) -> Self {
        Self::Message(m.into())
    }
}

impl Serialize for FerryError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type FerryResult<T> = Result<T, FerryError>;
