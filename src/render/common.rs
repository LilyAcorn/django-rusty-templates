use std::borrow::Cow;
use std::collections::BTreeMap;

use pyo3::prelude::*;

use super::types::{Content, ContentString, Context};
use super::{Evaluate, Render, RenderResult, Resolve, ResolveFailures, ResolveResult};
use crate::error::RenderError;
use crate::parse::{TagElement, TokenTree};
use crate::types::Argument;
use crate::types::ArgumentType;
use crate::types::TemplateString;
use crate::types::Text;
use crate::types::TranslatedText;
use crate::types::Variable;

impl Resolve for Variable {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        let mut parts = self.parts(template);
        let (first, mut object_at) = parts.next().expect("Variable names cannot be empty");
        let mut variable = match context.context.get(first) {
            Some(variable) => variable.bind(py).clone(),
            None => return Ok(None),
        };

        for (part, key_at) in parts {
            variable = match variable.get_item(part) {
                Ok(variable) => variable,
                Err(_) => match variable.getattr(part) {
                    Ok(variable) => variable,
                    Err(_) => {
                        let int = match part.parse::<usize>() {
                            Ok(int) => int,
                            Err(_) => {
                                return match failures {
                                    ResolveFailures::Raise => {
                                        Err(RenderError::VariableDoesNotExist {
                                            key: part.to_string(),
                                            object: variable.str()?.to_string(),
                                            key_at: key_at.into(),
                                            object_at: Some(object_at.into()),
                                        }
                                        .into())
                                    }
                                    ResolveFailures::IgnoreVariableDoesNotExist => Ok(None),
                                };
                            }
                        };
                        match variable.get_item(int) {
                            Ok(variable) => variable,
                            Err(_) => todo!(),
                        }
                    }
                },
            };
            object_at.1 += key_at.1 + 1;
        }
        Ok(Some(Content::Py(variable)))
    }
}

impl Resolve for Text {
    fn resolve<'t, 'py>(
        &self,
        _py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        _failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        let resolved = Cow::Borrowed(template.content(self.at));
        Ok(Some(Content::String(match context.autoescape {
            false => ContentString::String(resolved),
            true => ContentString::HtmlSafe(resolved),
        })))
    }
}

impl Resolve for TranslatedText {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        _failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        let resolved = Cow::Borrowed(template.content(self.at));
        let django_translation = py.import("django.utils.translation")?;
        let get_text = django_translation.getattr("gettext")?;
        let resolved = get_text.call1((resolved,))?.extract::<String>()?;
        Ok(Some(Content::String(match context.autoescape {
            false => ContentString::String(Cow::Owned(resolved)),
            true => ContentString::HtmlSafe(Cow::Owned(resolved)),
        })))
    }
}

impl Resolve for Argument {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        Ok(Some(match &self.argument_type {
            ArgumentType::Text(text) => return text.resolve(py, template, context, failures),
            ArgumentType::TranslatedText(text) => {
                return text.resolve(py, template, context, failures);
            }
            ArgumentType::Variable(variable) => {
                match variable.resolve(py, template, context, failures)? {
                    Some(content) => content,
                    None => {
                        let key = template.content(variable.at).to_string();
                        let context: BTreeMap<&String, &Bound<'py, PyAny>> = context
                            .context
                            .iter()
                            .map(|(k, v)| (k, v.bind(py)))
                            .collect();
                        let object = format!("{:?}", context);
                        return Err(RenderError::ArgumentDoesNotExist {
                            key,
                            object,
                            key_at: variable.at.into(),
                            object_at: None,
                        }
                        .into());
                    }
                }
            }
            ArgumentType::Float(number) => Content::Float(*number),
            ArgumentType::Int(number) => Content::Int(number.clone()),
        }))
    }
}

impl Resolve for TagElement {
    fn resolve<'t, 'py>(
        &self,
        py: Python<'py>,
        template: TemplateString<'t>,
        context: &mut Context,
        failures: ResolveFailures,
    ) -> ResolveResult<'t, 'py> {
        match self {
            Self::Text(text) => text.resolve(py, template, context, failures),
            Self::TranslatedText(text) => text.resolve(py, template, context, failures),
            Self::Variable(variable) => variable.resolve(py, template, context, failures),
            Self::Filter(filter) => filter.resolve(py, template, context, failures),
            Self::Int(int) => Ok(Some(Content::Int(int.clone()))),
            Self::Float(float) => Ok(Some(Content::Float(*float))),
        }
    }
}

impl Evaluate for TagElement {
    fn evaluate(
        &self,
        py: Python<'_>,
        template: TemplateString<'_>,
        context: &mut Context,
    ) -> Option<bool> {
        match self.resolve(
            py,
            template,
            context,
            ResolveFailures::IgnoreVariableDoesNotExist,
        ) {
            Ok(inner) => inner.evaluate(py, template, context),
            Err(_) => None,
        }
    }
}

impl Render for TokenTree {
    fn render<'t>(
        &self,
        py: Python<'_>,
        template: TemplateString<'t>,
        context: &mut Context,
    ) -> RenderResult<'t> {
        match self {
            Self::Text(text) => text.render(py, template, context),
            Self::TranslatedText(_text) => todo!(),
            Self::Tag(tag) => tag.render(py, template, context),
            Self::Variable(variable) => variable.render(py, template, context),
            Self::Filter(filter) => filter.render(py, template, context),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use pyo3::types::{PyDict, PyList, PyString};

    #[test]
    fn test_render_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "Lily").into_any();
            let context = HashMap::from([("name".to_string(), name.unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ name }}");
            let variable = Variable::new((3, 4));

            let rendered = variable.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_dict_lookup() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let data = PyDict::new(py);
            let name = PyString::new(py, "Lily");
            data.set_item("name", name).unwrap();
            let context = HashMap::from([("data".to_string(), data.into_any().unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ data.name }}");
            let variable = Variable::new((3, 9));

            let rendered = variable.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_list_lookup() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let name = PyString::new(py, "Lily");
            let names = PyList::new(py, [name]).unwrap();
            let context = HashMap::from([("names".to_string(), names.into_any().unbind())]);
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ names.0 }}");
            let variable = Variable::new((3, 7));

            let rendered = variable.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_attribute_lookup() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let locals = PyDict::new(py);
            py.run(
                c"
class User:
    def __init__(self, name):
        self.name = name

user = User('Lily')
",
                None,
                Some(&locals),
            )
            .unwrap();

            let context = locals.extract().unwrap();
            let mut context = Context {
                context,
                request: None,
                autoescape: false,
            };
            let template = TemplateString("{{ user.name }}");
            let variable = Variable::new((3, 9));

            let rendered = variable.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "Lily");
        })
    }

    #[test]
    fn test_render_html_autoescape() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let html = PyString::new(py, "<p>Hello World!</p>").into_any().unbind();
            let context = HashMap::from([("html".to_string(), html)]);
            let mut context = Context {
                context,
                request: None,
                autoescape: true,
            };
            let template = TemplateString("{{ html }}");
            let html = Variable::new((3, 4));

            let rendered = html.render(py, template, &mut context).unwrap();
            assert_eq!(rendered, "&lt;p&gt;Hello World!&lt;/p&gt;");
        })
    }
}
