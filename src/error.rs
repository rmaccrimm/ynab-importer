use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImportError {
    #[error("something went wrong parsing the event path '{0}'")]
    PathParsingError(String),

    #[error("failed to parse QFX file")]
    FileParsingError(#[from] sgmlish::Error),

    #[error("no paths provided with event")]
    NoPathError,
}
