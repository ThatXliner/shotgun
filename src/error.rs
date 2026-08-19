use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShotgunError {
    #[error("failed to read spec at {path}: {source}")]
    SpecRead {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse spec at {path}: {source}")]
    SpecParse {
        path: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("unsupported OpenAPI version: {0}")]
    UnsupportedVersion(String),

    #[error("failed to parse mapping file at {path}: {source}")]
    MappingParse {
        path: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("failed to write mapping file at {path}: {source}")]
    MappingWrite {
        path: String,
        #[source]
        source: anyhow::Error,
    },

    #[error("no target configured for endpoint {0}")]
    UnmappedEndpoint(String),

    #[error("upstream request failed: {0}")]
    Upstream(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, ShotgunError>;
