/// Error returned to the UI by commands. Serialized as a plain message.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Store(#[from] cominspect_store::StoreError),
    #[error("{0}")]
    Message(String),
}

impl CommandError {
    pub fn msg(message: impl Into<String>) -> Self {
        CommandError::Message(message.into())
    }
}

impl From<tauri::Error> for CommandError {
    fn from(e: tauri::Error) -> Self {
        CommandError::Message(e.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(e: std::io::Error) -> Self {
        CommandError::Message(e.to_string())
    }
}

impl serde::Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type CmdResult<T> = Result<T, CommandError>;
