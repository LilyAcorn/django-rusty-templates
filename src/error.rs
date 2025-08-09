use miette::{Diagnostic, SourceSpan};
use pyo3::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PyRenderError {
    #[error(transparent)]
    PyErr(#[from] PyErr),
    #[error(transparent)]
    RenderError(#[from] RenderError),
}

impl PyRenderError {
    pub fn try_into_render_error(self) -> Result<RenderError, PyErr> {
        match self {
            Self::RenderError(err) => Ok(err),
            Self::PyErr(err) => Err(err),
        }
    }
}

#[derive(Error, Debug, Diagnostic, PartialEq, Eq)]
pub enum RenderError {
    #[error("Failed lookup for key [{key}] in {object}")]
    ArgumentDoesNotExist {
        key: String,
        object: String,
        #[label("key")]
        key_at: SourceSpan,
        #[label("{object}")]
        object_at: Option<SourceSpan>,
    },
    #[error("Need {expected_count} values to unpack; got {actual_count}.")]
    TupleUnpackError {
        expected_count: usize,
        actual_count: usize,
        #[label("unpacked here")]
        expected_at: SourceSpan,
        #[label("from here")]
        actual_at: SourceSpan,
    },
    #[error("Failed lookup for key [{key}] in {object}")]
    VariableDoesNotExist {
        key: String,
        object: String,
        #[label("key")]
        key_at: SourceSpan,
        #[label("{object}")]
        object_at: Option<SourceSpan>,
    },
}
