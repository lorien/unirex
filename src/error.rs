//! Compile and usage errors.

use regex_syntax::ast;

/// An error produced while compiling a pattern.
#[derive(Debug)]
pub enum Error {
    /// `regex-syntax` could not parse the pattern.
    Parse(Box<ast::Error>),
    /// The pattern uses syntax this milestone does not support.
    Unsupported {
        /// A short description of the unsupported construct.
        what: &'static str,
        /// Byte offset in the pattern.
        offset: usize,
    },
    /// The pattern exceeds a compile limit.
    TooLarge {
        /// A short description of the exceeded limit.
        what: &'static str,
        /// Byte offset in the pattern, or zero when not tied to a token.
        offset: usize,
    },
}

impl Error {
    /// The byte offset in the pattern, when known.
    pub fn offset(&self) -> Option<usize> {
        match self {
            Error::Parse(_) => None,
            Error::Unsupported { offset, .. } | Error::TooLarge { offset, .. } => Some(*offset),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(err) => write!(f, "regex parse error: {err}"),
            Error::Unsupported { what, offset } => write!(f, "{what} at position {offset}"),
            Error::TooLarge { what, offset } => write!(f, "{what} at position {offset}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Parse(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

impl From<ast::Error> for Error {
    fn from(err: ast::Error) -> Error {
        Error::Parse(Box::new(err))
    }
}
