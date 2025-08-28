use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Arc;

use num_bigint::{BigInt, Sign};
use num_traits::cast::ToPrimitive;
use pyo3::exceptions::PyAttributeError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyNone, PyString, PyTuple};

use super::types::{AsBorrowedContent, Content, Context, PyContext};
use super::{Evaluate, Render, RenderResult, Resolve, ResolveFailures, ResolveResult};
use crate::error::{AnnotatePyErr, PyRenderError, RenderError};
use crate::parse::{For, IfCondition, SimpleTag, Tag, Url};
use crate::template::django_rusty_templates::NoReverseMatch;
use crate::types::TemplateString;
use crate::utils::PyResultMethods;

fn current_app(py: Python, request: &Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
    let none = py.None();
    let request = match request {
        None => return Ok(none),
        Some(request) => request,
    };
    if let Ok(current_app) = request
        .getattr(py, "current_app")
        .ok_or_isinstance_of::<PyAttributeError>(py)?
    {
        return Ok(current_app);
    }
    match request
        .getattr(py, "resolver_match")
        .ok_or_isinstance_of::<PyAttributeError>(py)?
    {
        Ok(resolver_match) if !resolver_match.is_none(py) => {
            resolver_match.getattr(py, "namespace")
        }
        _ => Ok(none),
    }
}

impl Resolve for Url {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        let view_name = match self.view_name.resolve(py, template, context, failures)? {
            Some(view_name) => view_name,
            None => "".as_content(),
        };
        let urls = py.import("django.urls")?;
        let reverse = urls.getattr("reverse")?;

        let current_app = current_app(py, &context.request)?;
        let url = if self.kwargs.is_empty() {
            let py_args = PyList::empty(py);
            for arg in &self.args {
                py_args.append(arg.resolve(py, template, context, failures)?)?;
            }
            reverse.call1((
                view_name,
                py.None(),
                py_args.to_tuple(),
                py.None(),
                current_app,
            ))
        } else {
            let kwargs = PyDict::new(py);
            for (key, value) in &self.kwargs {
                kwargs.set_item(key, value.resolve(py, template, context, failures)?)?;
            }
            reverse.call1((view_name, py.None(), py.None(), kwargs, current_app))
        };
        match &self.variable {
            None => Ok(Some(Content::Py(url?))),
            Some(variable) => match url.ok_or_isinstance_of::<NoReverseMatch>(py)? {
                Ok(url) => {
                    context.insert(variable.clone(), url);
                    Ok(None)
                }
                Err(_) => Ok(None),
            },
        }
    }
}

impl Evaluate for Content<'_, '_> {
    fn evaluate(
        &self,
        _py: Python<'_>,
        _template: TemplateString<'_>,
        _context: &mut Context,
    ) -> Option<bool> {
        Some(match self {
            Self::Py(obj) => obj.is_truthy().unwrap_or(false),
            Self::String(s) => !s.as_raw().is_empty(),
            Self::Float(f) => *f != 0.0,
            Self::Int(n) => *n != BigInt::ZERO,
            Self::Bool(b) => *b,
        })
    }
}

trait PyCmp<T> {
    fn eq(&self, other: &T) -> bool;

    fn ne(&self, other: &T) -> bool {
        !self.eq(other)
    }

    fn lt(&self, other: &T) -> bool;

    fn gt(&self, other: &T) -> bool;

    fn lte(&self, other: &T) -> bool;

    fn gte(&self, other: &T) -> bool;
}

impl PyCmp<Content<'_, '_>> for Content<'_, '_> {
    fn eq(&self, other: &Content<'_, '_>) -> bool {
        match (self, other) {
            (Self::Py(obj), Content::Py(other)) => obj.eq(other).unwrap_or(false),
            (Self::Py(obj), Content::Float(other)) => obj.eq(other).unwrap_or(false),
            (Self::Py(obj), Content::Int(other)) => obj.eq(other).unwrap_or(false),
            (Self::Py(obj), Content::Bool(other)) => obj.eq(other).unwrap_or(false),
            (Self::Py(obj), Content::String(other)) => obj.eq(other.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Py(other)) => other.eq(obj).unwrap_or(false),
            (Self::Int(obj), Content::Py(other)) => other.eq(obj).unwrap_or(false),
            (Self::String(obj), Content::Py(other)) => other.eq(obj.as_raw()).unwrap_or(false),
            (Self::Bool(obj), Content::Py(other)) => other.eq(obj).unwrap_or(false),
            (Self::Float(obj), Content::Float(other)) => obj == other,
            (Self::Int(obj), Content::Int(other)) => obj == other,
            (Self::Int(obj), Content::Bool(other)) => u8::try_from(obj)
                .map(|o| o == *other as u8)
                .unwrap_or(false),
            (Self::Bool(obj), Content::Int(other)) => u8::try_from(other)
                .map(|o| o == *obj as u8)
                .unwrap_or(false),
            (Self::Float(obj), Content::Int(other)) => {
                match other.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => false,
                    f64::NEG_INFINITY => false,
                    other => *obj == other,
                }
            }
            (Self::Int(obj), Content::Float(other)) => {
                match obj.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => false,
                    f64::NEG_INFINITY => false,
                    obj => obj == *other,
                }
            }
            (Self::Float(obj), Content::Bool(other)) => match other {
                true => *obj == 1.0,
                false => *obj == 0.0,
            },
            (Self::Bool(obj), Content::Float(other)) => match obj {
                true => *other == 1.0,
                false => *other == 0.0,
            },
            (Self::String(obj), Content::String(other)) => obj.as_raw() == other.as_raw(),
            (Self::Bool(obj), Content::Bool(other)) => obj == other,
            _ => false,
        }
    }

    fn lt(&self, other: &Content<'_, '_>) -> bool {
        match (self, other) {
            (Self::Py(obj), Content::Py(other)) => obj.lt(other).unwrap_or(false),
            (Self::Py(obj), Content::Float(other)) => obj.lt(other).unwrap_or(false),
            (Self::Py(obj), Content::Int(other)) => obj.lt(other).unwrap_or(false),
            (Self::Py(obj), Content::Bool(other)) => obj.lt(other).unwrap_or(false),
            (Self::Py(obj), Content::String(other)) => obj.lt(other.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Py(other)) => other.gt(obj).unwrap_or(false),
            (Self::Int(obj), Content::Py(other)) => other.gt(obj).unwrap_or(false),
            (Self::String(obj), Content::Py(other)) => other.gt(obj.as_raw()).unwrap_or(false),
            (Self::Bool(obj), Content::Py(other)) => other.gt(obj).unwrap_or(false),
            (Self::Float(obj), Content::Float(other)) => obj < other,
            (Self::Int(obj), Content::Int(other)) => obj < other,
            (Self::Int(obj), Content::Bool(other)) => match obj.sign() {
                Sign::Minus => true,
                _ => u8::try_from(obj).map(|o| o < *other as u8).unwrap_or(false),
            },
            (Self::Bool(obj), Content::Int(other)) => match other.sign() {
                Sign::Minus => false,
                _ => u8::try_from(other).map(|o| o > *obj as u8).unwrap_or(true),
            },
            (Self::Float(obj), Content::Int(other)) => {
                match other.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => obj.is_finite() || *obj == f64::NEG_INFINITY,
                    f64::NEG_INFINITY => *obj == f64::NEG_INFINITY,
                    other => *obj < other,
                }
            }
            (Self::Int(obj), Content::Float(other)) => {
                match obj.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => *other == f64::INFINITY,
                    f64::NEG_INFINITY => other.is_finite() || *other == f64::INFINITY,
                    obj => obj < *other,
                }
            }
            (Self::Float(obj), Content::Bool(other)) => match other {
                true => *obj < 1.0,
                false => *obj < 0.0,
            },
            (Self::Bool(obj), Content::Float(other)) => match obj {
                true => *other > 1.0,
                false => *other > 0.0,
            },
            (Self::String(obj), Content::String(other)) => obj.as_raw() < other.as_raw(),
            (Self::Bool(obj), Content::Bool(other)) => obj < other,
            _ => false,
        }
    }

    fn gt(&self, other: &Content<'_, '_>) -> bool {
        match (self, other) {
            (Self::Py(obj), Content::Py(other)) => obj.gt(other).unwrap_or(false),
            (Self::Py(obj), Content::Float(other)) => obj.gt(other).unwrap_or(false),
            (Self::Py(obj), Content::Int(other)) => obj.gt(other).unwrap_or(false),
            (Self::Py(obj), Content::Bool(other)) => obj.gt(other).unwrap_or(false),
            (Self::Py(obj), Content::String(other)) => obj.gt(other.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Py(other)) => other.lt(obj).unwrap_or(false),
            (Self::Int(obj), Content::Py(other)) => other.lt(obj).unwrap_or(false),
            (Self::String(obj), Content::Py(other)) => other.lt(obj.as_raw()).unwrap_or(false),
            (Self::Bool(obj), Content::Py(other)) => other.lt(obj).unwrap_or(false),
            (Self::Float(obj), Content::Float(other)) => obj > other,
            (Self::Int(obj), Content::Int(other)) => obj > other,
            (Self::Int(obj), Content::Bool(other)) => match obj.sign() {
                Sign::Minus => false,
                _ => u8::try_from(obj).map(|o| o > *other as u8).unwrap_or(true),
            },
            (Self::Bool(obj), Content::Int(other)) => match other.sign() {
                Sign::Minus => true,
                _ => u8::try_from(other).map(|o| o < *obj as u8).unwrap_or(false),
            },
            (Self::Float(obj), Content::Int(other)) => {
                match other.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => *obj == f64::INFINITY,
                    f64::NEG_INFINITY => obj.is_finite() || *obj == f64::INFINITY,
                    other => *obj > other,
                }
            }
            (Self::Int(obj), Content::Float(other)) => {
                match obj.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => other.is_finite() || *other == f64::NEG_INFINITY,
                    f64::NEG_INFINITY => *other == f64::NEG_INFINITY,
                    obj => obj > *other,
                }
            }
            (Self::Float(obj), Content::Bool(other)) => match other {
                true => *obj > 1.0,
                false => *obj > 0.0,
            },
            (Self::Bool(obj), Content::Float(other)) => match obj {
                true => *other < 1.0,
                false => *other < 0.0,
            },
            (Self::String(obj), Content::String(other)) => obj.as_raw() > other.as_raw(),
            (Self::Bool(obj), Content::Bool(other)) => obj > other,
            _ => false,
        }
    }

    fn lte(&self, other: &Content<'_, '_>) -> bool {
        match (self, other) {
            (Self::Py(obj), Content::Py(other)) => obj.le(other).unwrap_or(false),
            (Self::Py(obj), Content::Float(other)) => obj.le(other).unwrap_or(false),
            (Self::Py(obj), Content::Int(other)) => obj.le(other).unwrap_or(false),
            (Self::Py(obj), Content::Bool(other)) => obj.le(other).unwrap_or(false),
            (Self::Py(obj), Content::String(other)) => obj.le(other.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Py(other)) => other.ge(obj).unwrap_or(false),
            (Self::Int(obj), Content::Py(other)) => other.ge(obj).unwrap_or(false),
            (Self::Bool(obj), Content::Py(other)) => other.ge(obj).unwrap_or(false),
            (Self::String(obj), Content::Py(other)) => other.ge(obj.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Float(other)) => obj <= other,
            (Self::Int(obj), Content::Int(other)) => obj <= other,
            (Self::Int(obj), Content::Bool(other)) => match obj.sign() {
                Sign::Minus => true,
                _ => u8::try_from(obj)
                    .map(|o| o <= *other as u8)
                    .unwrap_or(false),
            },
            (Self::Bool(obj), Content::Int(other)) => match other.sign() {
                Sign::Minus => false,
                _ => u8::try_from(other).map(|o| o >= *obj as u8).unwrap_or(true),
            },
            (Self::Float(obj), Content::Int(other)) => {
                match other.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => obj.is_finite() || *obj == f64::NEG_INFINITY,
                    f64::NEG_INFINITY => *obj == f64::NEG_INFINITY,
                    other => *obj <= other,
                }
            }
            (Self::Int(obj), Content::Float(other)) => {
                match obj.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => *other == f64::INFINITY,
                    f64::NEG_INFINITY => other.is_finite() || *other == f64::INFINITY,
                    obj => obj <= *other,
                }
            }
            (Self::Float(obj), Content::Bool(other)) => match other {
                true => *obj <= 1.0,
                false => *obj <= 0.0,
            },
            (Self::Bool(obj), Content::Float(other)) => match obj {
                true => *other >= 1.0,
                false => *other >= 0.0,
            },
            (Self::String(obj), Content::String(other)) => obj.as_raw() <= other.as_raw(),
            (Self::Bool(obj), Content::Bool(other)) => obj <= other,
            _ => false,
        }
    }

    fn gte(&self, other: &Content<'_, '_>) -> bool {
        match (self, other) {
            (Self::Py(obj), Content::Py(other)) => obj.ge(other).unwrap_or(false),
            (Self::Py(obj), Content::Float(other)) => obj.ge(other).unwrap_or(false),
            (Self::Py(obj), Content::Int(other)) => obj.ge(other).unwrap_or(false),
            (Self::Py(obj), Content::Bool(other)) => obj.ge(other).unwrap_or(false),
            (Self::Py(obj), Content::String(other)) => obj.ge(other.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Py(other)) => other.le(obj).unwrap_or(false),
            (Self::Int(obj), Content::Py(other)) => other.le(obj).unwrap_or(false),
            (Self::Bool(obj), Content::Py(other)) => other.le(obj).unwrap_or(false),
            (Self::String(obj), Content::Py(other)) => other.le(obj.as_raw()).unwrap_or(false),
            (Self::Float(obj), Content::Float(other)) => obj >= other,
            (Self::Int(obj), Content::Int(other)) => obj >= other,
            (Self::Int(obj), Content::Bool(other)) => match obj.sign() {
                Sign::Minus => false,
                _ => u8::try_from(obj).map(|o| o >= *other as u8).unwrap_or(true),
            },
            (Self::Bool(obj), Content::Int(other)) => match other.sign() {
                Sign::Minus => true,
                _ => u8::try_from(other)
                    .map(|o| o <= *obj as u8)
                    .unwrap_or(false),
            },
            (Self::Float(obj), Content::Int(other)) => {
                match other.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => *obj == f64::INFINITY,
                    f64::NEG_INFINITY => obj.is_finite() || *obj == f64::INFINITY,
                    other => *obj >= other,
                }
            }
            (Self::Int(obj), Content::Float(other)) => {
                match obj.to_f64().expect("BigInt to f64 is always possible") {
                    f64::INFINITY => other.is_finite() || *other == f64::NEG_INFINITY,
                    f64::NEG_INFINITY => *other == f64::NEG_INFINITY,
                    obj => obj >= *other,
                }
            }
            (Self::Float(obj), Content::Bool(other)) => match other {
                true => *obj >= 1.0,
                false => *obj >= 0.0,
            },
            (Self::Bool(obj), Content::Float(other)) => match obj {
                true => *other <= 1.0,
                false => *other <= 0.0,
            },
            (Self::String(obj), Content::String(other)) => obj.as_raw() >= other.as_raw(),
            (Self::Bool(obj), Content::Bool(other)) => obj >= other,
            _ => false,
        }
    }
}

impl PyCmp<Option<Content<'_, '_>>> for Option<Content<'_, '_>> {
    fn eq(&self, other: &Option<Content<'_, '_>>) -> bool {
        match (self, other) {
            (None, None) => true,
            (Some(obj), Some(other)) => obj.eq(other),
            (Some(obj), None) | (None, Some(obj)) => match obj {
                Content::Py(obj) => obj.eq(PyNone::get(obj.py())).unwrap_or(false),
                _ => false,
            },
        }
    }

    fn lt(&self, other: &Option<Content<'_, '_>>) -> bool {
        match (self, other) {
            (Some(obj), Some(other)) => obj.lt(other),
            _ => false,
        }
    }

    fn gt(&self, other: &Option<Content<'_, '_>>) -> bool {
        match (self, other) {
            (Some(obj), Some(other)) => obj.gt(other),
            _ => false,
        }
    }

    fn lte(&self, other: &Option<Content<'_, '_>>) -> bool {
        match (self, other) {
            (Some(obj), Some(other)) => obj.lte(other),
            _ => false,
        }
    }

    fn gte(&self, other: &Option<Content<'_, '_>>) -> bool {
        match (self, other) {
            (Some(obj), Some(other)) => obj.gte(other),
            _ => false,
        }
    }
}

trait Contains<T> {
    fn contains(&self, other: T) -> Option<bool>;
}

impl Contains<Option<Content<'_, '_>>> for Content<'_, '_> {
    fn contains(&self, other: Option<Content<'_, '_>>) -> Option<bool> {
        match other {
            None => match self {
                Self::Py(obj) => obj.contains(PyNone::get(obj.py())).ok(),
                _ => None,
            },
            Some(Content::Py(other)) => {
                let obj = self.to_py(other.py()).ok()?;
                obj.contains(other).ok()
            }
            Some(Content::String(other)) => match self {
                Self::String(obj) => Some(obj.as_raw().contains(other.as_raw().as_ref())),
                Self::Int(_) | Self::Float(_) | Self::Bool(_) => None,
                Self::Py(obj) => obj.contains(other).ok(),
            },
            Some(Content::Int(n)) => match self {
                Self::Py(obj) => obj.contains(n).ok(),
                _ => None,
            },
            Some(Content::Float(f)) => match self {
                Self::Py(obj) => obj.contains(f).ok(),
                _ => None,
            },
            Some(Content::Bool(b)) => match self {
                Self::Py(obj) => obj.contains(b).ok(),
                _ => None,
            },
        }
    }
}

trait ResolveTuple<'t, 'py> {
    fn resolve(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> Result<(Option<Content<'t, 'py>>, Option<Content<'t, 'py>>), PyRenderError>;
}

impl<'t, 'py> ResolveTuple<'t, 'py> for (IfCondition, IfCondition) {
    fn resolve(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> Result<(Option<Content<'t, 'py>>, Option<Content<'t, 'py>>), PyRenderError> {
        const IGNORE: ResolveFailures = ResolveFailures::IgnoreVariableDoesNotExist;
        Ok(match self {
            (IfCondition::Variable(l), IfCondition::Variable(r)) => {
                let left = l.resolve(py, template, context, IGNORE)?;
                let right = r.resolve(py, template, context, IGNORE)?;
                (left, right)
            }
            (IfCondition::Variable(l), r) => {
                let left = l.resolve(py, template, context, IGNORE)?;
                let right = r
                    .evaluate(py, template, context)
                    .expect("Right cannot be an expression that evaluates to None");
                (left, Some(Content::Bool(right)))
            }
            (l, IfCondition::Variable(r)) => {
                let left = l
                    .evaluate(py, template, context)
                    .expect("Left cannot be an expression that evaluates to None");
                let right = r.resolve(py, template, context, IGNORE)?;
                (Some(Content::Bool(left)), right)
            }
            (l, r) => {
                let left = l
                    .evaluate(py, template, context)
                    .expect("Left cannot be an expression that evaluates to None");
                let right = r
                    .evaluate(py, template, context)
                    .expect("Right cannot be an expression that evaluates to None");
                (Some(Content::Bool(left)), Some(Content::Bool(right)))
            }
        })
    }
}

impl Evaluate for IfCondition {
    fn evaluate(
        &self,
        py: Python<'_>,
        template: TemplateString<'_>,
        context: &mut Context,
    ) -> Option<bool> {
        Some(match self {
            Self::Variable(v) => v.evaluate(py, template, context)?,
            Self::And(inner) => {
                let left = inner.0.evaluate(py, template, context).unwrap_or(false);
                let right = inner.1.evaluate(py, template, context).unwrap_or(false);
                if !left { false } else { right }
            }
            Self::Or(inner) => {
                let left = inner.0.evaluate(py, template, context);
                let right = inner.1.evaluate(py, template, context);
                match left {
                    None => false,
                    Some(left) => {
                        if left {
                            true
                        } else {
                            right.unwrap_or(false)
                        }
                    }
                }
            }
            Self::Not(inner) => match inner.evaluate(py, template, context) {
                None => false,
                Some(true) => false,
                Some(false) => true,
            },
            Self::Equal(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.eq(&r),
                Err(_) => false,
            },
            Self::NotEqual(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.ne(&r),
                Err(_) => false,
            },
            Self::LessThan(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.lt(&r),
                Err(_) => false,
            },
            Self::GreaterThan(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.gt(&r),
                Err(_) => false,
            },
            Self::LessThanEqual(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.lte(&r),
                Err(_) => false,
            },
            Self::GreaterThanEqual(inner) => match inner.resolve(py, template, context) {
                Ok((l, r)) => l.gte(&r),
                Err(_) => false,
            },
            Self::In(inner) => {
                let inner = match inner.resolve(py, template, context) {
                    Ok(inner) => inner,
                    Err(_) => return Some(false),
                };
                match inner {
                    (l, Some(r)) => r.contains(l).unwrap_or(false),
                    _ => false,
                }
            }
            Self::NotIn(inner) => {
                let inner = match inner.resolve(py, template, context) {
                    Ok(inner) => inner,
                    Err(_) => return Some(false),
                };
                match inner {
                    (l, Some(r)) => !(r.contains(l).unwrap_or(true)),
                    _ => false,
                }
            }
            Self::Is(inner) => {
                let inner = match inner.resolve(py, template, context) {
                    Ok(inner) => inner,
                    Err(_) => return Some(false),
                };
                match inner {
                    (Some(Content::Py(left)), Some(Content::Py(right))) => left.is(&right),
                    (Some(Content::Py(obj)), None) | (None, Some(Content::Py(obj))) => {
                        obj.is(PyNone::get(py).as_any())
                    }
                    (Some(Content::Bool(_)), None) => false,
                    (Some(Content::Bool(left)), Some(Content::Py(right))) => {
                        right.is(PyBool::new(py, left).as_any())
                    }
                    (None, None) => true,
                    _ => false,
                }
            }
            Self::IsNot(inner) => {
                let inner = match inner.resolve(py, template, context) {
                    Ok(inner) => inner,
                    Err(_) => return Some(false),
                };
                match inner {
                    (Some(Content::Py(left)), Some(Content::Py(right))) => !left.is(&right),
                    (Some(Content::Bool(left)), Some(Content::Bool(right))) => left != right,
                    (Some(Content::Py(obj)), None) | (None, Some(Content::Py(obj))) => {
                        !obj.is(PyNone::get(py).as_any())
                    }
                    (Some(Content::Bool(left)), Some(Content::Py(right))) => {
                        !right.is(PyBool::new(py, left).as_any())
                    }
                    (Some(Content::Bool(_)), _) => true,
                    (Some(Content::Py(left)), Some(Content::Bool(right))) => {
                        !left.is(PyBool::new(py, right).as_any())
                    }
                    (_, Some(Content::Bool(_))) => true,
                    (None, None) => false,
                    _ => true,
                }
            }
        })
    }
}

impl Render for Tag {
    fn render<'t>(
        &self,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        Ok(match self {
            Self::Autoescape { enabled, nodes } => {
                let autoescape = context.autoescape;
                context.autoescape = enabled.into();

                let mut rendered = vec![];
                for node in nodes {
                    rendered.push(node.render(py, template, context)?)
                }

                context.autoescape = autoescape;
                Cow::Owned(rendered.join(""))
            }
            Self::If {
                condition,
                truthy,
                falsey,
            } => {
                if condition.evaluate(py, template, context).unwrap_or(false) {
                    truthy.render(py, template, context)?
                } else {
                    falsey.render(py, template, context)?
                }
            }
            Self::For(for_tag) => for_tag.render(py, template, context)?,
            Self::Load => Cow::Borrowed(""),
            Self::SimpleTag(simple_tag) => simple_tag.render(py, template, context)?,
            Self::Url(url) => url.render(py, template, context)?,
        })
    }
}

impl For {
    fn render_python<'t>(
        &self,
        iterable: &Bound<'_, PyAny>,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        let mut parts = Vec::new();
        let mut list: Vec<_> = match iterable.try_iter() {
            Ok(iterator) => iterator.collect(),
            Err(error) => {
                let error = error.annotate(py, self.iterable.at, "here", template);
                return Err(error.into());
            }
        };
        if self.reversed {
            list.reverse();
        }
        context.push_for_loop(list.len());
        for (index, values) in list.into_iter().enumerate() {
            let values = match values {
                Ok(values) => values,
                Err(error) => {
                    let error =
                        error.annotate(py, self.iterable.at, "while iterating this", template);
                    return Err(error.into());
                }
            };
            context.push_variables(
                &self.variables.names,
                self.variables.at,
                values,
                self.iterable.at,
                index,
                template,
            )?;
            parts.push(self.body.render(py, template, context)?);
            context.increment_for_loop();
        }
        context.pop_variables(&self.variables.names);
        context.pop_for_loop();
        Ok(Cow::Owned(parts.join("")))
    }

    fn render_string<'t>(
        &self,
        string: &str,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        if self.variables.names.len() > 1 {
            return Err(RenderError::TupleUnpackError {
                expected_count: self.variables.names.len(),
                actual_count: 1,
                expected_at: self.variables.at.into(),
                actual_at: self.iterable.at.into(),
            }
            .into());
        }
        let mut parts = Vec::new();
        let mut chars: Vec<_> = string.chars().collect();
        if self.reversed {
            chars.reverse()
        }

        let variable = &self.variables.names[0];
        context.push_for_loop(chars.len());
        for (index, c) in chars.into_iter().enumerate() {
            let c = PyString::new(py, &c.to_string());
            context.push_variable(variable.clone(), c.into_any(), index);
            parts.push(self.body.render(py, template, context)?);
            context.increment_for_loop();
        }
        context.pop_variable(variable);
        context.pop_for_loop();
        Ok(Cow::Owned(parts.join("")))
    }
}

impl Render for For {
    fn render<'t>(
        &self,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        let iterable =
            match self
                .iterable
                .iterable
                .resolve(py, template, context, ResolveFailures::Raise)?
            {
                Some(iterable) => iterable,
                None => return self.empty.render(py, template, context),
            };
        match iterable {
            Content::Py(iterable) => self.render_python(&iterable, py, template, context),
            Content::String(s) => self.render_string(s.as_raw(), py, template, context),
            Content::Float(_) | Content::Int(_) | Content::Bool(_) => {
                unreachable!("float, int and bool literals are not iterable")
            }
        }
    }
}

impl SimpleTag {
    fn call_tag<'t>(
        &self,
        py: Python<'_>,
        template: TemplateString<'t>,
        args: VecDeque<Bound<'_, PyAny>>,
        kwargs: Bound<'_, PyDict>,
    ) -> RenderResult<'t> {
        let func = self.func.bind(py);
        match func.call(PyTuple::new(py, args)?, Some(&kwargs)) {
            Ok(content) => Ok(Cow::Owned(content.to_string())),
            Err(error) => Err(error.annotate(py, self.at, "here", template).into()),
        }
    }
}

impl Render for SimpleTag {
    fn render<'t>(
        &self,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        let mut args = VecDeque::new();
        for arg in &self.args {
            match arg.resolve(py, template, context, ResolveFailures::Raise)? {
                None => args.push_back(py.None().into_bound(py)),
                Some(arg) => args.push_back(arg.to_py(py)?),
            }
        }
        let kwargs = PyDict::new(py);
        for (key, value) in &self.kwargs {
            let value = value.resolve(py, template, context, ResolveFailures::Raise)?;
            kwargs.set_item(key, value)?;
        }
        if self.takes_context {
            // Take ownership of `context` so we can pass it to Python.
            // The `context` variable now points to an empty `Context` instance which will not be
            // used except as a placeholder.
            let swapped_context = std::mem::replace(context, Context::empty());

            // Wrap the context as a Python object and add it to the call args
            let py_context = Bound::new(py, PyContext::new(swapped_context))?.into_any();
            args.push_front(py_context.clone());

            // Actually call the tag
            let result = self.call_tag(py, template, args, kwargs);

            // Retrieve the PyContext wrapper from Python
            let extracted_context: PyContext = py_context
                .extract()
                .expect("The type of py_context should not have changed");
            // Ensure we only hold one reference in Rust by dropping the Python object.
            drop(py_context);

            // Try to remove the Context from the PyContext
            let inner_context = match Arc::try_unwrap(extracted_context.context) {
                // Fast path when we have the only reference in the Arc.
                Ok(inner_context) => inner_context,
                // Slow path when Python has held on to the context for some reason.
                // We can still do the right thing by cloning.
                Err(inner_context) => inner_context.clone_ref(py),
            };
            // Put the Context back in `context`
            let _ = std::mem::replace(context, inner_context);

            // Return the result of calling the tag
            result
        } else {
            self.call_tag(py, template, args, kwargs)
        }
    }
}
