use serde::{Serialize, Serializer};

/// Every error that can reach the frontend. It serializes to its display
/// message, so the UI can show it as is.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),
    #[error("接口返回错误（HTTP {status}）：{message}")]
    Api { status: u16, message: String },
    #[error("接口返回错误：{0}")]
    Stream(String),
    #[error("数据库错误：{0}")]
    Db(#[from] rusqlite::Error),
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),
    #[error("密钥存储失败：{0}")]
    Secret(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

impl AppError {
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
