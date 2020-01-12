use nix;
use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ErrorKind {
    #[error("io error")]
    IoError(#[from] io::Error),
    #[error("nix error")]
    NixError(#[from] nix::Error),
    // #[error("tokio error")]
    // TokioError(#[from] tokio::io::Error),
    #[error("an error occured: {0}")]
    ErrorMsg(String),
    #[error("an error occured: {0}")]
    ErrorStr(&'static str),
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct Error(ErrorKind);

impl Error {
    fn from_kind(kind: ErrorKind) -> Self {
        Self(kind)
    }
}

impl<E> From<E> for Error
where
    E: Into<ErrorKind>,
{
    fn from(err: E) -> Self {
        Self::from_kind(err.into())
    }
}

impl From<&'static str> for Error {
    fn from(err: &'static str) -> Self {
        Self::from_kind(ErrorKind::ErrorStr(err))
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::from_kind(ErrorKind::ErrorMsg(err))
    }
}
