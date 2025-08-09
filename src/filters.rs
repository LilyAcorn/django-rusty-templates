use std::sync::Arc;

use pyo3::prelude::*;

use crate::types::Argument;

#[derive(Clone, Debug, PartialEq)]
pub enum FilterType {
    Add(AddFilter),
    AddSlashes(AddSlashesFilter),
    Capfirst(CapfirstFilter),
    Center(CenterFilter),
    Default(DefaultFilter),
    Escape(EscapeFilter),
    External(ExternalFilter),
    Lower(LowerFilter),
    Safe(SafeFilter),
    Slugify(SlugifyFilter),
    Upper(UpperFilter),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddSlashesFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct AddFilter {
    pub argument: Argument,
}

impl AddFilter {
    pub fn new(argument: Argument) -> Self {
        Self { argument }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapfirstFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct CenterFilter {
    pub argument: Argument,
}

impl CenterFilter {
    pub fn new(argument: Argument) -> Self {
        Self { argument }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefaultFilter {
    pub argument: Argument,
}

impl DefaultFilter {
    pub fn new(argument: Argument) -> Self {
        Self { argument }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EscapeFilter;

#[derive(Clone, Debug)]
pub struct ExternalFilter {
    pub filter: Arc<Py<PyAny>>,
    pub argument: Option<Argument>,
}

impl ExternalFilter {
    pub fn new(filter: Py<PyAny>, argument: Option<Argument>) -> Self {
        Self {
            filter: Arc::new(filter),
            argument,
        }
    }
}

impl PartialEq for ExternalFilter {
    fn eq(&self, other: &Self) -> bool {
        // We use `Arc::ptr_eq` here to avoid needing the `py` token for true
        // equality comparison between two `Py` smart pointers.
        //
        // We only use `eq` in tests, so this concession is acceptable here.
        self.argument.eq(&other.argument) && Arc::ptr_eq(&self.filter, &other.filter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LowerFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct SafeFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct SlugifyFilter;

#[derive(Clone, Debug, PartialEq)]
pub struct UpperFilter;
