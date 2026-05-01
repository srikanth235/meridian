use thiserror::Error;

#[derive(Debug, Error)]
pub enum TrackerError {
    #[error("unsupported tracker kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("missing tracker api_key")]
    MissingApiKey,
    #[error("missing tracker project_slug")]
    MissingProjectSlug,
    #[error("linear request failed: {0}")]
    LinearApiRequest(String),
    #[error("linear non-2xx status {status}: {body}")]
    LinearApiStatus { status: u16, body: String },
    #[error("linear graphql errors: {0}")]
    LinearGraphqlErrors(String),
    #[error("linear payload missing data")]
    LinearUnknownPayload,
    #[error("linear pagination missing endCursor")]
    LinearMissingEndCursor,
}
