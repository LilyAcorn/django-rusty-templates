use miette::{Diagnostic, LabeledSpan, SourceSpan, miette};
use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;
use thiserror::Error;

use crate::types::TemplateString;

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
    #[error("Couldn't convert argument ({argument}) to integer")]
    InvalidArgumentInteger {
        argument: String,
        #[label("argument")]
        argument_at: SourceSpan,
    },
    #[error("Couldn't convert float ({argument}) to integer")]
    InvalidArgumentFloat {
        argument: String,
        #[label("here")]
        argument_at: SourceSpan,
    },
    #[error("Integer {argument} is too large")]
    OverflowError {
        argument: String,
        #[label("here")]
        argument_at: SourceSpan,
    },
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
    #[error("'{type_name}' object is not subscriptable")]
    NotSubscriptable {
        type_name: String,
        #[label("filter applied here")]
        at: SourceSpan,
    },
}

#[pyclass]
struct KeyErrorMessage {
    message: String,
}

#[pymethods]
impl KeyErrorMessage {
    fn __repr__(&self) -> &str {
        &self.message
    }
}

pub trait AnnotatePyErr {
    fn annotate(
        self,
        py: Python<'_>,
        at: (usize, usize),
        label: &str,
        template: TemplateString<'_>,
    ) -> Self;
}

impl AnnotatePyErr for PyErr {
    fn annotate(
        self,
        py: Python<'_>,
        at: (usize, usize),
        label: &str,
        template: TemplateString<'_>,
    ) -> Self {
        let message = miette!(
            labels = vec![LabeledSpan::at(at, label)],
            "{}",
            self.value(py),
        )
        .with_source_code(template.0.to_string());
        if self.is_instance_of::<PyKeyError>(py) {
            let message = format!("{message:?}");
            // Python converts the message to `repr(message)` for KeyError.
            // When annotating, this is unhelpful, so we work around this by defining a custom
            // `__repr__` that returns the message exactly as we want it.
            // https://github.com/python/cpython/blob/43573028c6ae21c66c118b8bae866c8968b87b68/Objects/exceptions.c#L2946-L2954
            let message = KeyErrorMessage { message };
            PyKeyError::new_err((message,))
        } else {
            let err_type = self.get_type(py);
            PyErr::from_type(err_type, format!("{message:?}"))
        }
    }
}
