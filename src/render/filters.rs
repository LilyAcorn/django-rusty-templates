use std::borrow::Cow;
use std::sync::LazyLock;

use html_escape::encode_quoted_attribute_to_string;
use pyo3::prelude::*;
use pyo3::sync::GILOnceCell;
use pyo3::types::PyType;

use crate::filters::{
    AddFilter, AddSlashesFilter, CapfirstFilter, DefaultFilter, EscapeFilter, ExternalFilter,
    FilterType, FirstFilter, LowerFilter, SafeFilter, SlugifyFilter, UpperFilter,
};
use crate::parse::Filter;
use crate::render::types::{Content, ContentString, Context};
use crate::render::{Resolve, ResolveFailures, ResolveResult};
use crate::types::TemplateString;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

// Used for replacing all non-word and non-spaces with an empty string
static NON_WORD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\w\s-]").expect("Static string will never panic"));

// regex for whitespaces and hyphen, used for replacing with hyphen only
static WHITESPACE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-\s]+").expect("Static string will never panic"));

static SAFEDATA: GILOnceCell<Py<PyType>> = GILOnceCell::new();

trait IntoOwnedContent<'t, 'py> {
    fn into_content(self) -> Option<Content<'t, 'py>>;
}

trait AsBorrowedContent<'a, 't, 'py>
where
    'a: 't,
{
    fn as_content(&'a self) -> Option<Content<'t, 'py>>;
}

impl<'a, 't, 'py> AsBorrowedContent<'a, 't, 'py> for str
where
    'a: 't,
{
    fn as_content(&'a self) -> Option<Content<'t, 'py>> {
        Some(Content::String(ContentString::String(Cow::Borrowed(self))))
    }
}

impl<'t, 'py> IntoOwnedContent<'t, 'py> for String {
    fn into_content(self) -> Option<Content<'t, 'py>> {
        Some(Content::String(ContentString::String(Cow::Owned(self))))
    }
}

impl Resolve for Filter {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        let left = self.left.resolve(py, template, context, failures)?;
        match &self.filter {
            FilterType::Add(filter) => filter.resolve(left, py, template, context),
            FilterType::AddSlashes(filter) => filter.resolve(left, py, template, context),
            FilterType::Capfirst(filter) => filter.resolve(left, py, template, context),
            FilterType::Default(filter) => filter.resolve(left, py, template, context),
            FilterType::Escape(filter) => filter.resolve(left, py, template, context),
            FilterType::External(filter) => filter.resolve(left, py, template, context),
            FilterType::First(filter) => filter.resolve(left, py, template, context),
            FilterType::Lower(filter) => filter.resolve(left, py, template, context),
            FilterType::Safe(filter) => filter.resolve(left, py, template, context),
            FilterType::Slugify(filter) => filter.resolve(left, py, template, context),
            FilterType::Upper(filter) => filter.resolve(left, py, template, context),
        }
    }
}

pub trait ResolveFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py>;
}

impl ResolveFilter for AddSlashesFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => content
                .render(context)?
                .replace(r"\", r"\\")
                .replace("\"", "\\\"")
                .replace("'", r"\'")
                .into_content(),
            None => "".as_content(),
        };
        Ok(content)
    }
}

impl ResolveFilter for AddFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let variable = match variable {
            Some(left) => left,
            None => return Ok(None),
        };
        let right = self
            .argument
            .resolve(py, template, context, ResolveFailures::Raise)?
            .expect("missing argument in context should already have raised");
        match (variable.to_bigint(), right.to_bigint()) {
            (Some(variable), Some(right)) => Ok(Some(Content::Int(variable + right))),
            _ => {
                let variable = variable.to_py(py)?;
                let right = right.to_py(py)?;
                match variable.add(right) {
                    Ok(sum) => Ok(Some(Content::Py(sum))),
                    Err(_) => Ok(None),
                }
            }
        }
    }
}

impl ResolveFilter for CapfirstFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => {
                let content_string = content.render(context)?.into_owned();
                let mut chars = content_string.chars();
                let first_char = match chars.next() {
                    Some(c) => c.to_uppercase(),
                    None => return Ok("".as_content()),
                };
                let string: String = first_char.chain(chars).collect();
                string.into_content()
            }
            None => "".as_content(),
        };
        Ok(content)
    }
}

impl ResolveFilter for DefaultFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(left) => Some(left),
            None => self
                .argument
                .resolve(py, template, context, ResolveFailures::Raise)?,
        };
        Ok(content)
    }
}

impl ResolveFilter for EscapeFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        _context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        Ok(Some(Content::String(ContentString::HtmlSafe(
            match variable {
                Some(content) => match content {
                    Content::String(ContentString::HtmlSafe(content)) => content,
                    Content::String(content) => {
                        let mut encoded = String::new();
                        encode_quoted_attribute_to_string(content.as_raw(), &mut encoded);
                        Cow::Owned(encoded)
                    }
                    Content::Int(n) => Cow::Owned(n.to_string()),
                    Content::Float(n) => Cow::Owned(n.to_string()),
                    Content::Py(object) => {
                        let content = object.str()?.extract::<String>()?;
                        let mut encoded = String::new();
                        encode_quoted_attribute_to_string(&content, &mut encoded);
                        Cow::Owned(encoded)
                    }
                },
                None => Cow::Borrowed(""),
            },
        ))))
    }
}

impl ResolveFilter for ExternalFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let arg = match &self.argument {
            Some(arg) => arg.resolve(py, template, context, ResolveFailures::Raise)?,
            None => None,
        };
        let filter = self.filter.bind(py);
        let value = match arg {
            Some(arg) => filter.call1((variable, arg))?,
            None => filter.call1((variable,))?,
        };
        Ok(Some(Content::Py(value)))
    }
}

impl ResolveFilter for FirstFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        _template: TemplateString<'t>,
        _context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => {
                // Try to get the first item from the content
                match content {
                    Content::Py(obj) => {
                        // Try to get the first item using Python's indexing
                        // Django only catches IndexError, not TypeError
                        match obj.get_item(0) {
                            Ok(first) => Some(Content::Py(first)),
                            Err(e) => {
                                // Check if it's an IndexError
                                if e.is_instance_of::<pyo3::exceptions::PyIndexError>(py) {
                                    // IndexError -> return empty string
                                    "".as_content()
                                } else {
                                    // Other errors (like TypeError) -> propagate
                                    return Err(e.into());
                                }
                            }
                        }
                    }
                    Content::String(s) => {
                        // For strings, get the first character
                        let s_raw = s.as_raw();
                        if s_raw.is_empty() {
                            "".as_content()
                        } else {
                            // Get the first character
                            let first_char = s_raw.chars().next().unwrap();
                            first_char.to_string().into_content()
                        }
                    }
                    Content::Int(_) => {
                        // Numbers are not sequences, should raise TypeError
                        // Match Django's behavior exactly
                        return Err(pyo3::exceptions::PyTypeError::new_err(
                            "'int' object is not subscriptable",
                        )
                        .into());
                    }
                    Content::Float(_) => {
                        // Floats are not sequences, should raise TypeError
                        // Match Django's behavior exactly
                        return Err(pyo3::exceptions::PyTypeError::new_err(
                            "'float' object is not subscriptable",
                        )
                        .into());
                    }
                }
            }
            None => "".as_content(),
        };
        Ok(content)
    }
}

impl ResolveFilter for LowerFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => Some(
                content
                    .resolve_string(context)?
                    .map_content(|content| Cow::Owned(content.to_lowercase())),
            ),
            None => "".as_content(),
        };
        Ok(content)
    }
}

impl ResolveFilter for SafeFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        _context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        Ok(Some(Content::String(ContentString::HtmlSafe(
            match variable {
                Some(content) => match content {
                    Content::String(content) => content.into_raw(),
                    Content::Int(n) => Cow::Owned(n.to_string()),
                    Content::Float(n) => Cow::Owned(n.to_string()),
                    Content::Py(object) => {
                        let content = object.str()?.extract::<String>()?;
                        Cow::Owned(content)
                    }
                },
                None => Cow::Borrowed(""),
            },
        ))))
    }
}

fn slugify(content: Cow<str>) -> Cow<str> {
    let content = content
        .nfkd()
        // first decomposing characters, then only keeping
        // the ascii ones, filtering out diacritics for example.
        .filter(|c| c.is_ascii())
        .collect::<String>()
        .to_lowercase();
    let content = NON_WORD_RE.replace_all(&content, "");
    let content = content.trim();
    let content = WHITESPACE_RE.replace_all(content, "-");
    Cow::Owned(content.to_string())
}

impl ResolveFilter for SlugifyFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        _template: TemplateString<'t>,
        _context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => match content {
                Content::Py(content) => {
                    let slug = slugify(Cow::Owned(content.str()?.extract::<String>()?));
                    #[allow(non_snake_case)]
                    let SafeData = SAFEDATA.import(py, "django.utils.safestring", "SafeData")?;
                    match content.is_instance(SafeData)? {
                        true => Some(Content::String(ContentString::HtmlSafe(slug))),
                        false => Some(Content::String(ContentString::HtmlUnsafe(slug))),
                    }
                }
                // Int and Float requires no slugify, we only need to turn it into a string.
                Content::Int(content) => Some(Content::String(ContentString::String(Cow::Owned(
                    content.to_string(),
                )))),
                Content::Float(content) => Some(Content::String(ContentString::String(
                    Cow::Owned(content.to_string()),
                ))),
                Content::String(content) => Some(content.map_content(slugify)),
            },
            None => "".as_content(),
        };
        Ok(content)
    }
}

impl ResolveFilter for UpperFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        _py: Python<'py>,
        _template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let content = match variable {
            Some(content) => {
                let content = content.resolve_string(context)?;
                Some(content.map_content(|content| Cow::Owned(content.to_uppercase())))
            }
            None => "".as_content(),
        };
        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filters::{AddSlashesFilter, DefaultFilter, LowerFilter, UpperFilter};
    use crate::parse::TagElement;
    use crate::render::Render;
    use crate::template::django_rusty_templates::{EngineData, Template};
    use crate::types::{Argument, ArgumentType, Text, Variable};

    use pyo3::types::{PyDict, PyString};
    static MARK_SAFE: GILOnceCell<Py<PyAny>> = GILOnceCell::new();

    fn mark_safe(py: Python<'_>, string: String) -> Result<Py<PyAny>, PyErr> {
        let mark_safe = match MARK_SAFE.get(py) {
            Some(mark_safe) => mark_safe,
            None => {
                let py_mark_safe = py.import("django.utils.safestring")?;
                let py_mark_safe = py_mark_safe.getattr("mark_safe")?;
                MARK_SAFE.set(py, py_mark_safe.into()).unwrap();
                MARK_SAFE.get(py).unwrap()
            }
        };
        let safe_string = mark_safe.call1(py, (string,))?;
        Ok(safe_string)
    }

    use std::collections::HashMap;

    #[test]
    fn test_render_filter() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|default:'Bryony' }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (16, 8),
                    argument_type: ArgumentType::Text(Text::new((17, 6))),
                })),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_filter_slugify_happy_path() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "hello world").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "hello-world");
        })
    }

    #[test]
    fn test_render_filter_slugify_spaces_omitted() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", " hello world").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "hello-world");
        })
    }

    #[test]
    fn test_render_filter_slugify_special_characters_omitted() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "a&€%").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "a");
        })
    }

    #[test]
    fn test_render_filter_slugify_multiple_spaces_inside_becomes_single() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "a & b").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "a-b");
        })
    }

    #[test]
    fn test_render_filter_slugify_integer() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|default:1|slugify }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "1");
        })
    }

    #[test]
    fn test_render_filter_slugify_float() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|default:1.3|slugify }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "1.3");
        })
    }

    #[test]
    fn test_render_filter_slugify_rust_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|default:'hello world'|slugify }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "hello-world");
        })
    }

    #[test]
    fn test_render_filter_slugify_safe_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|default:'hello world'|safe|slugify }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "hello-world");
        })
    }

    #[test]
    fn test_render_filter_slugify_safe_string_from_rust_treated_as_py() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify }}".to_string();
            let context = PyDict::new(py);
            let safe_string = mark_safe(py, "a &amp; b".to_string()).unwrap();
            context.set_item("var", safe_string).unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "a-amp-b");
        })
    }

    #[test]
    fn test_render_filter_slugify_non_existing_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ not_there|slugify }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");
        })
    }

    #[test]
    fn test_render_filter_slugify_invalid() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify:invalid }}".to_string();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");
            assert!(error_string.contains("slugify filter does not take an argument"));
        })
    }

    #[test]
    fn test_render_filter_addslashes_single() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "'hello'").into_any();
            let context = HashMap::from([("quotes".to_string(), name.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ quotes|addslashes }}");
            let variable = Variable::new((3, 6));
            let filter = Filter {
                at: (10, 10),
                left: TagElement::Variable(variable),
                filter: FilterType::AddSlashes(AddSlashesFilter),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, r"\'hello\'");
        })
    }

    #[test]
    fn test_render_filter_capfirst() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|capfirst }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "hello world").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "Hello world");

            let context = PyDict::new(py);
            context.set_item("var", "").unwrap();
            let template_string = "{{ var|capfirst }}".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");

            let context = PyDict::new(py);
            context.set_item("bar", "").unwrap();
            let template_string = "{{ var|capfirst }}".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");

            let template_string = "{{ var|capfirst:invalid }}".to_string();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");
            assert!(error_string.contains("capfirst filter does not take an argument"));
        })
    }

    #[test]
    fn test_render_filter_default() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|default:'Bryony' }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (16, 8),
                    argument_type: ArgumentType::Text(Text::new((17, 6))),
                })),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Bryony");
        })
    }

    #[test]
    fn test_render_filter_default_integer() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ count|default:12}}");
            let variable = Variable::new((3, 5));
            let filter = Filter {
                at: (9, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (17, 2),
                    argument_type: ArgumentType::Int(12.into()),
                })),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "12");
        })
    }

    #[test]
    fn test_render_filter_default_float() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ count|default:3.5}}");
            let variable = Variable::new((3, 5));
            let filter = Filter {
                at: (9, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (17, 3),
                    argument_type: ArgumentType::Float(3.5),
                })),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "3.5");
        })
    }

    #[test]
    fn test_render_filter_default_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let me = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("me".to_string(), me.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|default:me}}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (16, 2),
                    argument_type: ArgumentType::Variable(Variable::new((16, 2))),
                })),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_filter_lower() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|lower }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 5),
                left: TagElement::Variable(variable),
                filter: FilterType::Lower(LowerFilter),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "lily");
        })
    }

    #[test]
    fn test_render_filter_lower_missing_left() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|lower }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 5),
                left: TagElement::Variable(variable),
                filter: FilterType::Lower(LowerFilter),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "");
        })
    }

    #[test]
    fn test_render_chained_filters() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|default:'Bryony'|lower }}");
            let variable = Variable::new((3, 4));
            let default = Filter {
                at: (8, 7),
                left: TagElement::Variable(variable),
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (16, 8),
                    argument_type: ArgumentType::Text(Text::new((17, 6))),
                })),
            };
            let lower = Filter {
                at: (25, 5),
                left: TagElement::Filter(Box::new(default)),
                filter: FilterType::Lower(LowerFilter),
            };

            let rendered = lower.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "bryony");
        })
    }

    #[test]
    fn test_render_filter_upper() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "Foo").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|upper }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 5),
                left: TagElement::Variable(variable),
                filter: FilterType::Upper(UpperFilter),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "FOO");
        })
    }

    #[test]
    fn test_render_filter_upper_missing_left() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let context = HashMap::new();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name|upper }}");
            let variable = Variable::new((3, 4));
            let filter = Filter {
                at: (8, 5),
                left: TagElement::Variable(variable),
                filter: FilterType::Upper(UpperFilter),
            };

            let rendered = filter.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "");
        })
    }

    #[test]
    fn test_render_filter_first_list() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ items|first }}".to_string();
            let context = PyDict::new(py);
            let items = vec!["a", "b", "c"];
            context.set_item("items", items).unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "a");
        })
    }

    #[test]
    fn test_render_filter_first_empty_list() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ items|first }}".to_string();
            let context = PyDict::new(py);
            let items: Vec<&str> = vec![];
            context.set_item("items", items).unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");
        })
    }

    #[test]
    fn test_render_filter_first_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ text|first }}".to_string();
            let context = PyDict::new(py);
            context.set_item("text", "hello").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "h");
        })
    }

    #[test]
    fn test_render_filter_first_empty_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ text|first }}".to_string();
            let context = PyDict::new(py);
            context.set_item("text", "").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");
        })
    }

    #[test]
    fn test_render_filter_first_no_argument() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ items|first:'arg' }}".to_string();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");
            assert!(error_string.contains("first filter does not take an argument"));
        })
    }
}
