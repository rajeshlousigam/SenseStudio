#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("required tool `{0}` was not found; install FFmpeg (includes ffprobe) or point SenseStudio at it")]
    ToolMissing(String),
    #[error("{tool} failed: {message}")]
    Tool { tool: &'static str, message: String },
    #[error("could not parse {what}: {message}")]
    Parse { what: &'static str, message: String },
    #[error("file has no video stream")]
    NoVideo,
    #[error("no usable H.264 encoder found in this FFmpeg build")]
    NoEncoder,
    #[error("cancelled")]
    Cancelled,
    #[error("verification failed: {0}")]
    Verify(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
