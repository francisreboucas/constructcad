use serde::Serialize;
use std::fmt;

/// What kind of failure a command met.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// No command has this name.
    UnknownCommand,
    /// The arguments do not match the command's schema.
    InvalidArguments,
    /// A referenced element, type, level, view or material does not exist.
    NotFound,
    /// The edit would leave the model invalid, or is not allowed now.
    Rejected,
    /// Reading or writing a file failed.
    Io,
    /// An internal error (a bug); the session is kept as it was.
    Internal,
    /// A Lua script or plugin failed: a syntax or run-time error, or one of
    /// its limits (instructions, memory) reached.
    Script,
}

/// A command failure: a code for programs and a message for people.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, EngineError>;

impl EngineError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidArguments, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Rejected, message)
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Io, message)
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EngineError {}

impl From<cc_core::PersistError> for EngineError {
    fn from(err: cc_core::PersistError) -> Self {
        use crate::t;
        use cc_core::PersistError as E;
        Self::io(match err {
            E::Io(e) => t!("err-io", "detail" => e),
            E::Parse(e) => t!("err-parse", "detail" => e),
            E::Serialize(e) => t!("err-serialize", "detail" => e),
            E::Version(v) => t!("err-version", "version" => v, "max" => cc_core::FORMAT_VERSION),
        })
    }
}

impl From<cc_core::component_file::ComponentFileError> for EngineError {
    fn from(err: cc_core::component_file::ComponentFileError) -> Self {
        use crate::t;
        use cc_core::component_file::ComponentFileError as E;
        let not_found = matches!(err, E::NotFound(_));
        let message = match err {
            E::Io(e) => t!("err-io", "detail" => e),
            E::Parse(e) => t!("err-component-parse", "detail" => e),
            E::Serialize(e) => t!("err-serialize", "detail" => e),
            E::Version(v) => {
                t!("err-version", "version" => v, "max" => cc_core::component_file::COMPONENT_FORMAT_VERSION)
            }
            E::Invalid(detail) | E::NotFound(detail) => t!("err-component-invalid", "detail" => detail),
        };
        if not_found { Self::not_found(message) } else { Self::io(message) }
    }
}

impl From<cc_ifc::ImportError> for EngineError {
    fn from(err: cc_ifc::ImportError) -> Self {
        use crate::t;
        Self::io(match err {
            cc_ifc::ImportError::Io(e) => t!("err-io", "detail" => e),
            cc_ifc::ImportError::Parse(e) => t!("err-ifc-parse", "detail" => e),
            cc_ifc::ImportError::NotIfc => t!("err-not-ifc"),
        })
    }
}

impl From<std::io::Error> for EngineError {
    fn from(e: std::io::Error) -> Self {
        Self::io(crate::t!("err-io", "detail" => e))
    }
}

impl From<cc_core::wall_ops::EditError> for EngineError {
    fn from(e: cc_core::wall_ops::EditError) -> Self {
        use cc_core::wall_ops::EditError as E;
        Self::rejected(match e {
            E::NotLinear => crate::t!("msg-pick-linear"),
            E::Parallel => crate::t!("msg-parallel"),
            E::Degenerate => crate::t!("msg-degenerate"),
            E::Curved => crate::t!("msg-curved-wall"),
        })
    }
}
