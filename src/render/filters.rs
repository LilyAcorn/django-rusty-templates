use std::borrow::Cow;
use std::sync::LazyLock;

use html_escape::encode_quoted_attribute_to_string;
use num_bigint::{BigInt, ToBigInt};
use num_traits::ToPrimitive;
use pyo3::exceptions::PyIndexError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyType;

use crate::error::RenderError;
use crate::filters::{
    AddFilter, AddSlashesFilter, CapfirstFilter, CenterFilter, DefaultFilter, EscapeFilter,
    ExternalFilter, FilterType, FirstFilter, LowerFilter, SafeFilter, SlugifyFilter, UpperFilter,
};
use crate::parse::Filter;
use crate::render::types::{AsBorrowedContent, Content, ContentString, Context, IntoOwnedContent};
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

static SAFEDATA: PyOnceLock<Py<PyType>> = PyOnceLock::new();

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
            FilterType::Center(filter) => filter.resolve(left, py, template, context),
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
        Ok(Some(content))
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
                    None => return Ok(Some("".as_content())),
                };
                let string: String = first_char.chain(chars).collect();
                string.into_content()
            }
            None => "".as_content(),
        };
        Ok(Some(content))
    }
}

fn resolve_bigint(bigint: BigInt, at: (usize, usize)) -> Result<usize, RenderError> {
    match bigint.to_isize() {
        Some(n) => Ok(n.max(0) as usize),
        None => Err(RenderError::OverflowError {
            argument: bigint.to_string(),
            argument_at: at.into(),
        }),
    }
}

impl ResolveFilter for CenterFilter {
    fn resolve<'t, 'py>(
        &self,
        variable: Option<Content<'t, 'py>>,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> ResolveResult<'t, 'py> {
        let left: usize;
        let right: usize;
        let content = match variable {
            Some(content) => content.render(context)?,
            None => return Ok(Some("".as_content())),
        };
        let arg = self
            .argument
            .resolve(py, template, context, ResolveFailures::Raise)?
            .expect("missing argument in context should already have raised");

        let size = match arg {
            Content::Int(left) => resolve_bigint(left, self.argument.at)?,
            Content::String(left) => match left.as_raw().parse::<BigInt>() {
                Ok(n) => resolve_bigint(n, self.argument.at)?,
                Err(_) => {
                    return Err(RenderError::InvalidArgumentInteger {
                        argument: format!("'{}'", left.as_raw()),
                        argument_at: self.argument.at.into(),
                    }
                    .into());
                }
            },
            Content::Float(left) => match left.trunc().to_bigint() {
                Some(n) => resolve_bigint(n, self.argument.at)?,
                None => {
                    return Err(RenderError::InvalidArgumentFloat {
                        argument: left.to_string(),
                        argument_at: self.argument.at.into(),
                    }
                    .into());
                }
            },
            Content::Py(left) => match left.extract::<BigInt>() {
                Ok(left) => resolve_bigint(left, self.argument.at)?,
                Err(_) => {
                    let argument = left.to_string();
                    let argument_at = self.argument.at.into();
                    let err = match left.extract::<f64>() {
                        Ok(_) => RenderError::InvalidArgumentFloat {
                            argument,
                            argument_at,
                        },
                        Err(_) => RenderError::InvalidArgumentInteger {
                            argument,
                            argument_at,
                        },
                    };
                    return Err(err.into());
                }
            },
            Content::Bool(true) if content.is_empty() => return Ok(Some(" ".as_content())),
            Content::Bool(_) => return Ok(Some(content.into_content())),
        };

        if size <= content.len() {
            return Ok(Some(content.into_content()));
        }
        if size % 2 == 0 && content.len() % 2 != 0 {
            // If the size is even and the content length is odd, we need to adjust the centering
            right = (size - content.len()).div_ceil(2);
            left = size - content.len() - right;
        } else {
            right = (size - content.len()) / 2;
            left = size - content.len() - right;
        }
        let mut centered = String::with_capacity(size);

        centered.push_str(&" ".repeat(left));
        centered.push_str(&content);
        centered.push_str(&" ".repeat(right));

        Ok(Some(centered.into_content()))
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
                    Content::Bool(true) => Cow::Borrowed("True"),
                    Content::Bool(false) => Cow::Borrowed("False"),
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
            Some(content) => content,
            None => return Ok(Some("".as_content())),
        };

        match content {
            Content::Py(obj) => {
                // Try to get the first item using Python's indexing
                // Django only catches IndexError, not TypeError
                match obj.get_item(0) {
                    Ok(first) => Ok(Some(Content::Py(first))),
                    Err(e) if e.is_instance_of::<PyIndexError>(py) => Ok(Some("".as_content())),
                    Err(e) if e.is_instance_of::<pyo3::exceptions::PyTypeError>(py) => {
                        // Check if this is the standard "'type' object is not subscriptable" error
                        let error_msg = e.to_string();
                        if error_msg.contains("is not subscriptable") {
                            // Standard not subscriptable error - provide better formatting
                            let type_name = obj
                                .get_type()
                                .name()
                                .map(|n| n.to_string())
                                .unwrap_or_else(|_| "unknown".to_string());
                            Err(RenderError::NotSubscriptable {
                                type_name,
                                at: self.at,
                            }
                            .into())
                        } else {
                            // Custom TypeError - preserve the original message
                            Err(e.into())
                        }
                    }
                    Err(e) => Err(e.into()),
                }
            }
            Content::String(s) => {
                // For strings, get the first character
                // Skip the empty string always present at the start after splitting
                let mut chars = s.as_raw().split("").skip(1);
                match chars.next() {
                    None => Ok(Some("".as_content())),
                    Some(c) => Ok(Some(c.to_string().into_content())),
                }
            }
            Content::Int(_) => {
                // Numbers are not sequences, should raise TypeError
                // Match Django's behavior exactly
                Err(RenderError::NotSubscriptable {
                    type_name: "int".to_string(),
                    at: self.at,
                }
                .into())
            }
            Content::Float(_) => {
                // Floats are not sequences, should raise TypeError
                // Match Django's behavior exactly
                Err(RenderError::NotSubscriptable {
                    type_name: "float".to_string(),
                    at: self.at,
                }
                .into())
            }
            Content::Bool(_) => {
                // Booleans are not sequences, should raise TypeError
                // Match Django's behavior exactly
                Err(RenderError::NotSubscriptable {
                    type_name: "bool".to_string(),
                    at: self.at,
                }
                .into())
            }
        }
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
            Some(content) => content
                .resolve_string(context)?
                .map_content(|content| Cow::Owned(content.to_lowercase())),
            None => "".as_content(),
        };
        Ok(Some(content))
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
                    Content::Bool(true) => Cow::Borrowed("True"),
                    Content::Bool(false) => Cow::Borrowed("False"),
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
                        true => Content::String(ContentString::HtmlSafe(slug)),
                        false => Content::String(ContentString::HtmlUnsafe(slug)),
                    }
                }
                // Int and Float requires no slugify, we only need to turn it into a string.
                Content::Int(content) => content.to_string().into_content(),
                Content::Float(content) => content.to_string().into_content(),
                Content::String(content) => content.map_content(slugify),
                Content::Bool(true) => "true".as_content(),
                Content::Bool(false) => "false".as_content(),
            },
            None => "".as_content(),
        };
        Ok(Some(content))
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
                content.map_content(|content| Cow::Owned(content.to_uppercase()))
            }
            None => "".as_content(),
        };
        Ok(Some(content))
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
    static MARK_SAFE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

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
        Python::initialize();

        Python::attach(|py| {
            let name = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
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
        Python::initialize();

        Python::attach(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|slugify:invalid }}".to_string();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");
            assert!(error_string.contains("slugify filter does not take an argument"));
        })
    }

    #[test]
    fn test_render_filter_addslashes_single() {
        Python::initialize();

        Python::attach(|py| {
            let name = PyString::new(py, "'hello'").into_any();
            let context = HashMap::from([("quotes".to_string(), name.unbind())]);
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
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
    fn test_render_filter_center() {
        Python::initialize();

        Python::attach(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|center:'11' }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "hello").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "   hello   ");

            let context = PyDict::new(py);
            context.set_item("var", "django").unwrap();
            let template_string = "{{ var|center:'15' }}".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "     django    ");

            let context = PyDict::new(py);
            context.set_item("var", "django").unwrap();
            let template_string = "{{ var|center:1 }}".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "django");
        })
    }

    #[test]
    fn test_render_filter_center_no_argument_return_err() {
        Python::initialize();

        Python::attach(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|center }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "hello").unwrap();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");

            assert!(error_string.contains("Expected an argument"));
        })
    }

    #[test]
    fn test_render_filter_center_no_variable() {
        Python::initialize();

        Python::attach(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|center:'11' }}".to_string();
            let context = PyDict::new(py);
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");
        })
    }

    #[test]
    fn test_render_filter_center_on_0() {
        Python::initialize();

        Python::attach(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ var|center:0 }}".to_string();
            let context = PyDict::new(py);
            context.set_item("var", "hello").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "hello");
        })
    }

    #[test]
    fn test_render_filter_default() {
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let me = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("me".to_string(), me.unbind())]);
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let name = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let name = PyString::new(py, "Foo").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context::new(context, None, false);
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
        Python::initialize();

        Python::attach(|py| {
            let context = HashMap::new();
            let mut context = Context::new(context, None, false);
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
