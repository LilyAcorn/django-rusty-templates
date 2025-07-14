use pyo3::prelude::*;

#[pymodule]
pub mod django_rusty_templates {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use encoding_rs::Encoding;
    use pyo3::exceptions::{PyAttributeError, PyImportError};
    use pyo3::import_exception_bound;
    use pyo3::intern;
    use pyo3::prelude::*;
    use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple};

    use crate::loaders::{AppDirsLoader, CachedLoader, FileSystemLoader, Loader, LocMemLoader};
    use crate::parse::{Parser, TokenTree};
    use crate::render::Render;
    use crate::render::types::Context;
    use crate::types::TemplateString;
    use crate::utils::PyResultMethods;

    import_exception_bound!(django.core.exceptions, ImproperlyConfigured);
    import_exception_bound!(django.template.base, VariableDoesNotExist);
    import_exception_bound!(django.template.exceptions, TemplateDoesNotExist);
    import_exception_bound!(django.template.exceptions, TemplateSyntaxError);
    import_exception_bound!(django.template.library, InvalidTemplateLibrary);
    import_exception_bound!(django.urls, NoReverseMatch);

    impl TemplateSyntaxError {
        fn with_source_code(
            err: miette::Report,
            source: impl miette::SourceCode + 'static,
        ) -> PyErr {
            let miette_err = err.with_source_code(source);
            Self::new_err(format!("{miette_err:?}"))
        }
    }

    impl VariableDoesNotExist {
        fn with_source_code(
            err: miette::Report,
            source: impl miette::SourceCode + 'static,
        ) -> PyErr {
            let miette_err = err.with_source_code(source);
            let report = format!("{miette_err:?}");
            // Work around old-style Python formatting in VariableDoesNotExist.__str__
            let report = report.replace("%", "%%");
            Self::new_err(report)
        }
    }

    pub struct EngineData {
        autoescape: bool,
        libraries: HashMap<String, Py<PyAny>>,
    }

    impl EngineData {
        #[cfg(test)]
        pub fn empty() -> Self {
            Self {
                autoescape: false,
                libraries: HashMap::new(),
            }
        }
    }

    fn import_libraries(libraries: Bound<'_, PyAny>) -> PyResult<HashMap<String, Py<PyAny>>> {
        let py = libraries.py();
        let libraries: HashMap<String, String> = libraries.extract()?;
        let mut libs = HashMap::with_capacity(libraries.len());
        for (name, path) in libraries {
            let library = match py.import(&path).ok_or_isinstance_of::<PyImportError>(py)? {
                Ok(library) => library,
                Err(e) => {
                    let error = format!(
                        "Invalid template library specified. ImportError raised when trying to load '{}': {}",
                        path,
                        e.value(py)
                    );
                    return Err(InvalidTemplateLibrary::new_err(error));
                }
            };
            let library = match library
                .getattr(intern!(py, "register"))
                .ok_or_isinstance_of::<PyAttributeError>(py)?
            {
                Ok(library) => library,
                Err(_) => {
                    let error = format!(
                        "Module '{}' does not have a variable named 'register'",
                        path
                    );
                    return Err(InvalidTemplateLibrary::new_err(error));
                }
            };
            libs.insert(name, library.unbind());
        }
        Ok(libs)
    }

    #[pyclass]
    pub struct Engine {
        dirs: Vec<PathBuf>,
        app_dirs: bool,
        context_processors: Vec<String>,
        debug: bool,
        string_if_invalid: String,
        encoding: &'static Encoding,
        builtins: Vec<String>,
        template_loaders: Vec<Loader>,
        data: EngineData,
    }

    impl Engine {
        fn get_template_loaders<'py>(
            py: Python<'py>,
            template_loaders: &Bound<'_, PyList>,
        ) -> Result<Vec<Loader>, PyErr> {
            template_loaders
                .iter()
                .map(|template_loader| Self::find_template_loader(py, template_loader))
                .collect()
        }

        fn find_template_loader<'py>(
            py: Python<'py>,
            ld: Bound<'_, PyAny>,
        ) -> Result<Loader, PyErr> {
            // Try as string first
            if let Ok(loader_str) = ld.downcast::<PyString>() {
                return Self::map_loader(
                    py,
                    &loader_str.extract::<String>()?,
                    Vec::new(),
                    HashMap::new(),
                );
            }

            // Try as tuple (loader, args)
            if let Ok(loader_tuple) = ld.downcast::<PyTuple>() {
                if let Ok((loader_obj, args)) =
                    loader_tuple.extract::<(Bound<'_, PyString>, Bound<'_, PyAny>)>()
                {
                    let loader_path = loader_obj.extract::<String>()?;

                    // Process args based on type
                    // Check if args is PyList
                    if let Ok(true) = args.is_instance(&py.get_type::<PyList>()) {
                        let args_list = args.downcast::<PyList>()?;
                        let args_vec = args_list
                            .iter()
                            .map(|item| item.extract::<String>())
                            .collect::<Result<Vec<String>, PyErr>>()?;

                        return Self::map_loader(py, &loader_path, args_vec, HashMap::new());
                    // Check if args is PyTuple
                    } else if let Ok(true) = args.is_instance(&py.get_type::<PyTuple>()) {
                        let args_tuple = args.downcast::<PyTuple>()?;
                        let args_vec = args_tuple
                            .iter()
                            .map(|item| item.extract::<String>())
                            .collect::<Result<Vec<String>, PyErr>>()?;

                        return Self::map_loader(py, &loader_path, args_vec, HashMap::new());
                    // Check if args is PyDict
                    } else if let Ok(true) = args.is_instance(&py.get_type::<PyDict>()) {
                        let args_dict = args.downcast::<PyDict>()?;
                        let args_hashmap = args_dict
                            .iter()
                            .map(|(key, value)| {
                                Ok((key.extract::<String>()?, value.extract::<String>()?))
                            })
                            .collect::<Result<HashMap<String, String>, PyErr>>()?;

                        return Self::map_loader(py, &loader_path, Vec::new(), args_hashmap);
                    } else {
                        return Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                            "Unsupported args type: {:?}",
                            args
                        )));
                    }
                }
            }

            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "Unsupported loader type: {:?}",
                ld
            )))
        }

        fn map_loader(
            py: Python<'_>,
            loader_path: &str,
            args_vec: Vec<String>,
            args_hashmap: HashMap<String, String>,
        ) -> Result<Loader, PyErr> {
            match loader_path {
                "django.template.loaders.filesystem.Loader" => {
                    Ok(Loader::FileSystem(FileSystemLoader::new(
                        args_vec.into_iter().map(PathBuf::from).collect(),
                        encoding_rs::UTF_8,
                    )))
                }
                "django.template.loaders.app_directories.Loader" => {
                    Ok(Loader::AppDirs(AppDirsLoader::new(encoding_rs::UTF_8)))
                }
                "django.template.loaders.locmem.Loader" => {
                    Ok(Loader::LocMem(LocMemLoader::new(args_hashmap)))
                }
                "django.template.loaders.cached.Loader" => {
                    // Process nested loaders without cloning the whole args_vec
                    let nested_loaders = args_vec
                        .iter()
                        .map(|item| Self::map_loader(py, item, Vec::new(), HashMap::new()))
                        .collect::<Result<Vec<_>, _>>()?;

                    Ok(Loader::Cached(CachedLoader::new(nested_loaders)))
                }
                unknown => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                    "Unsupported loader type: {}",
                    unknown
                ))),
            }
        }
    }

    #[pymethods]
    impl Engine {
        #[new]
        #[pyo3(signature = (dirs=None, app_dirs=false, context_processors=None, debug=false, loaders=None, string_if_invalid="".to_string(), file_charset="utf-8".to_string(), libraries=None, builtins=None, autoescape=true))]
        #[allow(clippy::too_many_arguments)] // We're matching Django's Engine __init__ signature
        pub fn new(
            _py: Python<'_>,
            dirs: Option<Bound<'_, PyAny>>,
            app_dirs: bool,
            context_processors: Option<Bound<'_, PyAny>>,
            debug: bool,
            loaders: Option<Bound<'_, PyList>>,
            string_if_invalid: String,
            file_charset: String,
            libraries: Option<Bound<'_, PyAny>>,
            builtins: Option<Bound<'_, PyAny>>,
            autoescape: bool,
        ) -> PyResult<Self> {
            let dirs = match dirs {
                Some(dirs) => dirs.extract()?,
                None => Vec::new(),
            };
            let context_processors = match context_processors {
                Some(context_processors) => context_processors.extract()?,
                None => Vec::new(),
            };
            let encoding = match Encoding::for_label(file_charset.as_bytes()) {
                Some(encoding) => encoding,
                None => todo!(),
            };
            let template_loaders = match loaders {
                Some(_) if app_dirs => {
                    let err = ImproperlyConfigured::new_err(
                        "app_dirs must not be set when loaders is defined.",
                    );
                    return Err(err);
                }
                Some(_loaders) => {
                    let py_loaders = _loaders.downcast::<PyList>().unwrap();

                    let loaders = match Self::get_template_loaders(_py, py_loaders) {
                        Ok(loaders) => loaders,
                        Err(err) => {
                            let error = format!(
                                "Invalid template loader specified. ImportError raised when trying to load '{}'",
                                err
                            );
                            return Err(InvalidTemplateLibrary::new_err(error));
                        }
                    };

                    loaders
                }
                None => {
                    let filesystem_loader =
                        Loader::FileSystem(FileSystemLoader::new(dirs.clone(), encoding));
                    let appdirs_loader = Loader::AppDirs(AppDirsLoader::new(encoding));
                    let loaders = if app_dirs {
                        vec![filesystem_loader, appdirs_loader]
                    } else {
                        vec![filesystem_loader]
                    };
                    let cached_loader = Loader::Cached(CachedLoader::new(loaders));
                    vec![cached_loader]
                }
            };
            let libraries = match libraries {
                None => HashMap::new(),
                Some(libraries) => import_libraries(libraries)?,
            };
            let builtins = vec![];
            let data = EngineData {
                autoescape,
                libraries,
            };
            Ok(Self {
                dirs,
                app_dirs,
                context_processors,
                debug,
                template_loaders,
                string_if_invalid,
                encoding,
                builtins,
                data,
            })
        }

        pub fn get_template(
            &mut self,
            py: Python<'_>,
            template_name: String,
        ) -> PyResult<Template> {
            let mut tried = Vec::new();
            for loader in &mut self.template_loaders {
                match loader.get_template(py, &template_name, &self.data) {
                    Ok(template) => return template,
                    Err(e) => tried.push(e.tried),
                }
            }
            Err(TemplateDoesNotExist::new_err((template_name, tried)))
        }

        #[allow(clippy::wrong_self_convention)] // We're implementing a Django interface
        pub fn from_string(&self, template_code: Bound<'_, PyString>) -> PyResult<Template> {
            Template::new_from_string(template_code.py(), template_code.extract()?, &self.data)
        }

        // TODO render_to_string needs implementation.
    }

    #[derive(Debug, Clone, PartialEq)]
    #[pyclass]
    pub struct Template {
        pub filename: Option<PathBuf>,
        pub template: String,
        pub nodes: Vec<TokenTree>,
        pub autoescape: bool,
    }

    impl Template {
        pub fn new(
            py: Python<'_>,
            template: &str,
            filename: PathBuf,
            engine_data: &EngineData,
        ) -> PyResult<Self> {
            let mut parser = Parser::new(py, TemplateString(template), &engine_data.libraries);
            let nodes = match parser.parse() {
                Ok(nodes) => nodes,
                Err(err) => {
                    let err = err.try_into_parse_error()?;
                    let source =
                        miette::NamedSource::new(filename.to_string_lossy(), template.to_string());
                    return Err(TemplateSyntaxError::with_source_code(err.into(), source));
                }
            };
            Ok(Self {
                template: template.to_string(),
                filename: Some(filename),
                nodes,
                autoescape: engine_data.autoescape,
            })
        }

        pub fn new_from_string(
            py: Python<'_>,
            template: String,
            engine_data: &EngineData,
        ) -> PyResult<Self> {
            let mut parser = Parser::new(py, TemplateString(&template), &engine_data.libraries);
            let nodes = match parser.parse() {
                Ok(nodes) => nodes,
                Err(err) => {
                    let err = err.try_into_parse_error()?;
                    return Err(TemplateSyntaxError::with_source_code(err.into(), template));
                }
            };
            Ok(Self {
                template,
                filename: None,
                nodes,
                autoescape: engine_data.autoescape,
            })
        }

        fn _render(&self, py: Python<'_>, context: &mut Context) -> PyResult<String> {
            let mut rendered = String::with_capacity(self.template.len());
            let template = TemplateString(&self.template);
            for node in &self.nodes {
                match node.render(py, template, context) {
                    Ok(content) => rendered.push_str(&content),
                    Err(err) => {
                        let err = err.try_into_render_error()?;
                        return Err(VariableDoesNotExist::with_source_code(
                            err.into(),
                            self.template.clone(),
                        ));
                    }
                }
            }
            Ok(rendered)
        }
    }

    #[pymethods]
    impl Template {
        #[pyo3(signature = (context=None, request=None))]
        pub fn render(
            &self,
            py: Python<'_>,
            context: Option<Bound<'_, PyDict>>,
            request: Option<Bound<'_, PyAny>>,
        ) -> PyResult<String> {
            let mut base_context = HashMap::from([
                ("None".to_string(), py.None()),
                ("True".to_string(), PyBool::new(py, true).to_owned().into()),
                (
                    "False".to_string(),
                    PyBool::new(py, false).to_owned().into(),
                ),
            ]);
            let context = match context {
                Some(context) => {
                    let new_context: HashMap<_, _> = context.extract()?;
                    base_context.extend(new_context.into_iter());
                    base_context
                }
                None => base_context,
            };
            let request = request.map(|request| request.unbind());
            let mut context = Context {
                request,
                context,
                autoescape: self.autoescape,
            };
            self._render(py, &mut context)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::django_rusty_templates::*;

    use pyo3::Python;
    use pyo3::types::{PyDict, PyDictMethods, PyList, PyString, PyTuple};

    #[test]
    fn test_syntax_error() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let mut filename = std::env::current_dir().unwrap();
            filename.push("tests");
            filename.push("templates");
            filename.push("parse_error.txt");

            let expected = format!(
                "TemplateSyntaxError:   × Empty variable tag
   ╭─[{}:1:28]
 1 │ This is an empty variable: {{{{ }}}}
   ·                            ──┬──
   ·                              ╰── here
   ╰────
",
                filename.display(),
            );

            let engine = EngineData::empty();
            let template_string = std::fs::read_to_string(&filename).unwrap();
            let error = temp_env::with_var("NO_COLOR", Some("1"), || {
                Template::new(py, &template_string, filename, &engine).unwrap_err()
            });

            let error_string = format!("{error}");
            assert_eq!(error_string, expected);
        })
    }

    #[test]
    fn test_syntax_error_from_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ foo.bar|title'foo' }}".to_string();
            let error = temp_env::with_var("NO_COLOR", Some("1"), || {
                Template::new_from_string(py, template_string, &engine).unwrap_err()
            });

            let expected = "TemplateSyntaxError:   × Could not parse the remainder
   ╭────
 1 │ {{ foo.bar|title'foo' }}
   ·                 ──┬──
   ·                   ╰── here
   ╰────
";

            let error_string = format!("{error}");
            assert_eq!(error_string, expected);
        })
    }

    #[test]
    fn test_render_empty_template() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let context = PyDict::new(py);

            assert_eq!(template.render(py, Some(context), None).unwrap(), "");
        })
    }

    #[test]
    fn test_render_template_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "Hello {{ user }}!".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let context = PyDict::new(py);
            context.set_item("user", "Lily").unwrap();

            assert_eq!(
                template.render(py, Some(context), None).unwrap(),
                "Hello Lily!"
            );
        })
    }

    #[test]
    fn test_render_template_unknown_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "Hello {{ user }}!".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let context = PyDict::new(py);

            assert_eq!(template.render(py, Some(context), None).unwrap(), "Hello !");
        })
    }

    #[test]
    fn test_render_template_variable_nested() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "Hello {{ user.profile.names.0 }}!".to_string();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let locals = PyDict::new(py);
            py.run(
                cr#"
class User:
    def __init__(self, names):
        self.profile = {"names": names}

user = User(["Lily"])
"#,
                None,
                Some(&locals),
            )
            .unwrap();
            let user = locals.get_item("user").unwrap().unwrap();
            let context = PyDict::new(py);
            context.set_item("user", user.into_any()).unwrap();

            assert_eq!(
                template.render(py, Some(context), None).unwrap(),
                "Hello Lily!"
            );
        })
    }

    #[test]
    fn test_engine_from_string() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = Engine::new(
                py,
                None,
                false,
                None,
                false,
                None,
                "".to_string(),
                "utf-8".to_string(),
                None,
                None,
                false,
            )
            .unwrap();
            let template_string = PyString::new(py, "Hello {{ user }}!");
            let template = engine.from_string(template_string).unwrap();
            let context = PyDict::new(py);

            assert_eq!(template.render(py, Some(context), None).unwrap(), "Hello !");
        })
    }

    #[test]
    fn test_clone_template() {
        use std::collections::HashMap;

        use pyo3::IntoPyObject;
        use pyo3::types::{PyAnyMethods, PyListMethods};

        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let cwd = std::env::current_dir().unwrap();
            let sys_path = py.import("sys").unwrap().getattr("path").unwrap();
            let sys_path = sys_path.downcast().unwrap();
            sys_path.append(cwd).unwrap();
            let mut engine = Engine::new(
                py,
                Some(vec!["tests/templates"].into_pyobject(py).unwrap()),
                false,
                None,
                false,
                None,
                "".to_string(),
                "utf-8".to_string(),
                Some(
                    HashMap::from([("custom_filters", "tests.templatetags.custom_filters")])
                        .into_pyobject(py)
                        .unwrap()
                        .into_any(),
                ),
                None,
                false,
            )
            .unwrap();
            let template = engine
                .get_template(py, "full_example.html".to_string())
                .unwrap();
            let cloned = template.clone();
            assert_eq!(cloned, template);
        })
    }

    #[test]
    fn test_engine_loader_priority() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            // Create a Python list
            let py_list = PyList::new(
                py,
                &[
                    PyString::new(py, "django.template.loaders.filesystem.Loader").into_any(),
                    PyString::new(py, "django.template.loaders.app_directories.Loader").into_any(),
                ],
            );

            let engine = Engine::new(
                py,
                None,
                false,
                None,
                false,
                py_list.ok(),
                "".to_string(),
                "utf-8".to_string(),
                None,
                None,
                false,
            )
            .unwrap();

            let template_string = PyString::new(py, "Hello {{ user }}!");
            let template = engine.from_string(template_string).unwrap();
            let context = PyDict::new(py);

            assert_eq!(template.render(py, Some(context), None).unwrap(), "Hello !");
        })
    }

    #[test]
    fn test_engine_cached_loader_priority() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            // // Create a Python string
            let py_string = PyString::new(py, "django.template.loaders.filesystem.Loader");

            // Create a Python tuple
            let py_tuple = PyTuple::new(
                py,
                &[
                    PyString::new(py, "django.template.loaders.cached.Loader").into_any(),
                    PyList::new(
                        py,
                        &[
                            PyString::new(py, "django.template.loaders.filesystem.Loader"),
                            PyString::new(py, "django.template.loaders.app_directories.Loader"),
                        ],
                    )
                    .unwrap()
                    .into_any(),
                ],
            );

            // Create a heterogeneous Python list (containing a string and a tuple)
            let py_list = PyList::new(py, &[py_string.into_any(), py_tuple.unwrap().into_any()]);

            let engine = Engine::new(
                py,
                None,
                false,
                None,
                false,
                py_list.ok(),
                "".to_string(),
                "utf-8".to_string(),
                None,
                None,
                false,
            )
            .unwrap();

            let template_string = PyString::new(py, "Hello {{ user }}!");
            let template = engine.from_string(template_string).unwrap();
            let context = PyDict::new(py);

            assert_eq!(template.render(py, Some(context), None).unwrap(), "Hello !");
        })
    }
}
