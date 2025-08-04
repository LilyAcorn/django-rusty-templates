use std::borrow::Cow;
use std::collections::HashMap;
use std::iter::zip;

use html_escape::encode_quoted_attribute;
use num_bigint::{BigInt, ToBigInt};
use pyo3::exceptions::PyAttributeError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyString, PyType};

use crate::error::{PyRenderError, RenderError};
use crate::utils::PyResultMethods;

#[derive(Debug)]
pub struct ForLoop {
    count: usize,
    len: usize,
}

impl ForLoop {
    pub fn counter0(&self) -> usize {
        self.count
    }

    pub fn counter(&self) -> usize {
        self.count + 1
    }

    pub fn rev_counter(&self) -> usize {
        self.len - self.count
    }

    pub fn rev_counter0(&self) -> usize {
        self.len - self.count - 1
    }

    pub fn first(&self) -> bool {
        self.count == 0
    }

    pub fn last(&self) -> bool {
        self.count + 1 == self.len
    }
}

#[derive(Debug)]
pub struct Context {
    pub request: Option<Py<PyAny>>,
    pub context: HashMap<String, Py<PyAny>>,
    pub autoescape: bool,
    pub loops: Vec<ForLoop>,
}

impl Context {
    pub fn new(
        context: HashMap<String, Py<PyAny>>,
        request: Option<Py<PyAny>>,
        autoescape: bool,
    ) -> Self {
        Self {
            request,
            context,
            autoescape,
            loops: Vec::new(),
        }
    }

    pub fn push_variable(&mut self, name: String, value: Bound<'_, PyAny>) {
        self.context.insert(name, value.unbind());
    }

    pub fn push_variables(
        &mut self,
        names: &Vec<String>,
        names_at: (usize, usize),
        values: Bound<'_, PyAny>,
        values_at: (usize, usize),
    ) -> Result<(), PyRenderError> {
        if names.len() == 1 {
            self.push_variable(names[0].clone(), values);
        } else {
            let values: Vec<_> = values.try_iter()?.collect();
            if names.len() == values.len() {
                for (name, value) in zip(names, values) {
                    self.context.insert(name.clone(), value?.unbind());
                }
            } else {
                return Err(RenderError::TupleUnpackError {
                    expected_count: names.len(),
                    actual_count: values.len(),
                    expected_at: names_at.into(),
                    actual_at: values_at.into(),
                }
                .into());
            }
        }
        Ok(())
    }

    pub fn push_for_loop(&mut self, len: usize) {
        self.loops.push(ForLoop { count: 0, len })
    }

    pub fn increment_for_loop(&mut self) {
        let for_loop = self
            .loops
            .last_mut()
            .expect("Called within an active for loop");
        for_loop.count += 1
    }

    pub fn pop_for_loop(&mut self) {
        self.loops
            .pop()
            .expect("Called when exiting an active for loop");
    }

    pub fn get_for_loop(&self, depth: usize) -> Option<&ForLoop> {
        match self.loops.len().checked_sub(depth + 1) {
            Some(index) => self.loops.get(index),
            None => None,
        }
    }
}

#[derive(Debug, IntoPyObject)]
pub enum ContentString<'t> {
    String(Cow<'t, str>),
    HtmlSafe(Cow<'t, str>),
    HtmlUnsafe(Cow<'t, str>),
}

#[allow(clippy::needless_lifetimes)] // https://github.com/rust-lang/rust-clippy/issues/13923
impl<'t, 'py> ContentString<'t> {
    pub fn content(self) -> Cow<'t, str> {
        match self {
            Self::String(content) => content,
            Self::HtmlSafe(content) => content,
            Self::HtmlUnsafe(content) => Cow::Owned(encode_quoted_attribute(&content).to_string()),
        }
    }

    pub fn as_raw(&self) -> &Cow<'t, str> {
        match self {
            Self::String(content) => content,
            Self::HtmlSafe(content) => content,
            Self::HtmlUnsafe(content) => content,
        }
    }

    pub fn into_raw(self) -> Cow<'t, str> {
        match self {
            Self::String(content) => content,
            Self::HtmlSafe(content) => content,
            Self::HtmlUnsafe(content) => content,
        }
    }

    pub fn map_content(self, f: impl FnOnce(Cow<'t, str>) -> Cow<'t, str>) -> Content<'t, 'py> {
        Content::String(match self {
            Self::String(content) => Self::String(f(content)),
            Self::HtmlSafe(content) => Self::HtmlSafe(f(content)),
            Self::HtmlUnsafe(content) => Self::HtmlUnsafe(f(content)),
        })
    }
}

fn resolve_python<'t>(value: Bound<'_, PyAny>, context: &Context) -> PyResult<ContentString<'t>> {
    if !context.autoescape {
        return Ok(ContentString::String(
            value.str()?.extract::<String>()?.into(),
        ));
    };
    let py = value.py();

    let value = match value.is_instance_of::<PyString>() {
        true => value,
        false => value.str()?.into_any(),
    };
    Ok(
        match value
            .getattr(intern!(py, "__html__"))
            .ok_or_isinstance_of::<PyAttributeError>(py)?
        {
            Ok(html) => ContentString::HtmlSafe(html.call0()?.extract::<String>()?.into()),
            Err(_) => ContentString::HtmlUnsafe(value.str()?.extract::<String>()?.into()),
        },
    )
}

#[derive(Debug, IntoPyObject)]
pub enum Content<'t, 'py> {
    Py(Bound<'py, PyAny>),
    String(ContentString<'t>),
    Float(f64),
    Int(BigInt),
    Bool(bool),
}

impl<'t, 'py> Content<'t, 'py> {
    pub fn render(self, context: &Context) -> PyResult<Cow<'t, str>> {
        Ok(match self {
            Self::Py(content) => resolve_python(content, context)?.content(),
            Self::String(content) => content.content(),
            Self::Float(content) => content.to_string().into(),
            Self::Int(content) => content.to_string().into(),
            Self::Bool(true) => "True".into(),
            Self::Bool(false) => "False".into(),
        })
    }

    pub fn resolve_string(self, context: &Context) -> PyResult<ContentString<'t>> {
        Ok(match self {
            Self::String(content) => content,
            Self::Float(content) => ContentString::String(content.to_string().into()),
            Self::Int(content) => ContentString::String(content.to_string().into()),
            Self::Py(content) => return resolve_python(content, context),
            Self::Bool(_content) => todo!(),
        })
    }

    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            Self::Int(left) => Some(left.clone()),
            Self::String(left) => left.as_raw().parse::<BigInt>().ok(),
            Self::Float(left) => left.trunc().to_bigint(),
            Self::Py(left) => match left.extract::<BigInt>() {
                Ok(left) => Some(left),
                Err(_) => {
                    let int = PyType::new::<PyInt>(left.py());
                    match int.call1((left,)) {
                        Ok(left) => Some(
                            left.extract::<BigInt>()
                                .expect("Python integers are BigInt compatible"),
                        ),
                        Err(_) => None,
                    }
                }
            },
            Self::Bool(_content) => todo!(),
        }
    }

    pub fn to_py(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        Ok(match self {
            Self::Py(object) => object.clone(),
            Self::Int(i) => i
                .into_pyobject(py)
                .expect("A BigInt can always be converted to a Python int.")
                .into_any(),
            Self::Float(f) => f
                .into_pyobject(py)
                .expect("An f64 can always be converted to a Python float.")
                .into_any(),
            Self::String(s) => match s {
                ContentString::String(s) => s
                    .into_pyobject(py)
                    .expect("A string can always be converted to a Python str.")
                    .into_any(),
                ContentString::HtmlUnsafe(s) => s
                    .into_pyobject(py)
                    .expect("A string can always be converted to a Python str.")
                    .into_any(),
                ContentString::HtmlSafe(s) => {
                    let string = s
                        .into_pyobject(py)
                        .expect("A string can always be converted to a Python str.");
                    let safestring = py.import(intern!(py, "django.utils.safestring"))?;
                    let mark_safe = safestring.getattr(intern!(py, "mark_safe"))?;
                    mark_safe.call1((string,))?
                }
            },
            Self::Bool(_content) => todo!(),
        })
    }
}
