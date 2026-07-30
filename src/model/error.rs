use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    InvalidRepository(PathBuf),
    Parse {
        language: &'static str,
        path: String,
        message: String,
    },
    Graph(weavatrix_graph::GraphError),
    Json(blazingly_json::Error),
    Scan(weavatrix_scan::Error),
    Analysis(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "I/O error at {}: {source}", path.display())
            }
            Self::InvalidRepository(path) => {
                write!(
                    formatter,
                    "repository root is not a readable directory: {}",
                    path.display()
                )
            }
            Self::Parse {
                language,
                path,
                message,
            } => write!(formatter, "{language} parse failed for {path}: {message}"),
            Self::Graph(source) => write!(formatter, "invalid graph: {source}"),
            Self::Json(source) => write!(formatter, "JSON serialization failed: {source}"),
            Self::Scan(source) => write!(formatter, "repository scan failed: {source}"),
            Self::Analysis(message) => write!(formatter, "repository analysis failed: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Graph(source) => Some(source),
            Self::Json(source) => Some(source),
            Self::Scan(source) => Some(source),
            _ => None,
        }
    }
}

impl From<blazingly_json::Error> for Error {
    fn from(value: blazingly_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<weavatrix_graph::GraphError> for Error {
    fn from(value: weavatrix_graph::GraphError) -> Self {
        Self::Graph(value)
    }
}

impl From<weavatrix_scan::Error> for Error {
    fn from(value: weavatrix_scan::Error) -> Self {
        Self::Scan(value)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
