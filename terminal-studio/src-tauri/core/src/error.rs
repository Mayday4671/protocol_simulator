use serde::{Serialize, Serializer};

/// 统一错误类型：实现 Serialize 后可直接作为 Tauri command 的 Err 返回给前端
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),

    #[error("迁移错误: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Msg(String),
}

impl AppError {
    pub fn msg<S: Into<String>>(s: S) -> Self {
        AppError::Msg(s.into())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Msg(s)
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
