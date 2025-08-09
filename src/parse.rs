use std::collections::HashMap;
use std::iter::Peekable;

use either::Either;
use miette::{Diagnostic, SourceSpan};
use num_bigint::BigInt;
use pyo3::intern;
use pyo3::prelude::*;
use thiserror::Error;

use crate::filters::AddFilter;
use crate::filters::AddSlashesFilter;
use crate::filters::CapfirstFilter;
use crate::filters::CenterFilter;
use crate::filters::DefaultFilter;
use crate::filters::EscapeFilter;
use crate::filters::ExternalFilter;
use crate::filters::FilterType;
use crate::filters::LowerFilter;
use crate::filters::SafeFilter;
use crate::filters::SlugifyFilter;
use crate::filters::UpperFilter;
use crate::lex::START_TAG_LEN;
use crate::lex::autoescape::{AutoescapeEnabled, AutoescapeError, lex_autoescape_argument};
use crate::lex::common::LexerError;
use crate::lex::core::{Lexer, TokenType};
use crate::lex::ifcondition::{
    IfConditionAtom, IfConditionLexer, IfConditionOperator, IfConditionTokenType,
};
use crate::lex::load::{LoadLexer, LoadToken};
use crate::lex::tag::{TagLexerError, TagParts, lex_tag};
use crate::lex::url::{UrlLexer, UrlLexerError, UrlToken, UrlTokenType};
use crate::lex::variable::{
    Argument as ArgumentToken, ArgumentType as ArgumentTokenType, VariableLexerError,
    VariableTokenType, lex_variable,
};
use crate::types::Argument;
use crate::types::ArgumentType;
use crate::types::TemplateString;
use crate::types::Text;
use crate::types::TranslatedText;
use crate::types::Variable;

impl ArgumentToken {
    fn parse(&self, template: TemplateString<'_>) -> Result<Argument, ParseError> {
        Ok(Argument {
            at: self.at,
            argument_type: match self.argument_type {
                ArgumentTokenType::Variable => ArgumentType::Variable(Variable::new(self.at)),
                ArgumentTokenType::Text => ArgumentType::Text(Text::new(self.content_at())),
                ArgumentTokenType::Numeric => match template.content(self.at).parse::<BigInt>() {
                    Ok(n) => ArgumentType::Int(n),
                    Err(_) => match template.content(self.at).parse::<f64>() {
                        Ok(f) => ArgumentType::Float(f),
                        Err(_) => return Err(ParseError::InvalidNumber { at: self.at.into() }),
                    },
                },
                ArgumentTokenType::TranslatedText => {
                    ArgumentType::TranslatedText(TranslatedText::new(self.content_at()))
                }
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TagElement {
    Int(BigInt),
    Float(f64),
    Text(Text),
    TranslatedText(Text),
    Variable(Variable),
    Filter(Box<Filter>),
}

fn unexpected_argument(filter: &'static str, right: Argument) -> ParseError {
    ParseError::UnexpectedArgument {
        filter,
        at: right.at.into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filter {
    pub at: (usize, usize),
    pub left: TagElement,
    pub filter: FilterType,
}

impl Filter {
    pub fn new(
        parser: &Parser,
        at: (usize, usize),
        left: TagElement,
        right: Option<Argument>,
    ) -> Result<Self, ParseError> {
        let filter = match parser.template.content(at) {
            "add" => match right {
                Some(right) => FilterType::Add(AddFilter::new(right)),
                None => return Err(ParseError::MissingArgument { at: at.into() }),
            },
            "addslashes" => match right {
                Some(right) => return Err(unexpected_argument("addslashes", right)),
                None => FilterType::AddSlashes(AddSlashesFilter),
            },
            "capfirst" => match right {
                Some(right) => return Err(unexpected_argument("capfirst", right)),
                None => FilterType::Capfirst(CapfirstFilter),
            },
            "center" => match right {
                Some(right) => FilterType::Center(CenterFilter::new(right)),
                None => return Err(ParseError::MissingArgument { at: at.into() }),
            },
            "default" => match right {
                Some(right) => FilterType::Default(DefaultFilter::new(right)),
                None => return Err(ParseError::MissingArgument { at: at.into() }),
            },
            "escape" => match right {
                Some(right) => return Err(unexpected_argument("escape", right)),
                None => FilterType::Escape(EscapeFilter),
            },
            "lower" => match right {
                Some(right) => return Err(unexpected_argument("lower", right)),
                None => FilterType::Lower(LowerFilter),
            },
            "safe" => match right {
                Some(right) => return Err(unexpected_argument("safe", right)),
                None => FilterType::Safe(SafeFilter),
            },
            "slugify" => match right {
                Some(right) => return Err(unexpected_argument("slugify", right)),
                None => FilterType::Slugify(SlugifyFilter),
            },
            "upper" => match right {
                Some(right) => return Err(unexpected_argument("upper", right)),
                None => FilterType::Upper(UpperFilter),
            },
            external => {
                let external = match parser.external_filters.get(external) {
                    Some(external) => external.clone().unbind(),
                    None => {
                        return Err(ParseError::InvalidFilter {
                            at: at.into(),
                            filter: external.to_string(),
                        });
                    }
                };
                FilterType::External(ExternalFilter::new(external, right))
            }
        };
        Ok(Self { at, left, filter })
    }
}

fn parse_numeric(content: &str, at: (usize, usize)) -> Result<TagElement, ParseError> {
    match content.parse::<BigInt>() {
        Ok(n) => Ok(TagElement::Int(n)),
        Err(_) => match content.parse::<f64>() {
            Ok(f) => Ok(TagElement::Float(f)),
            Err(_) => Err(ParseError::InvalidNumber { at: at.into() }),
        },
    }
}

impl UrlToken {
    fn parse(&self, parser: &Parser) -> Result<TagElement, ParseError> {
        let content_at = self.content_at();
        let (start, _len) = content_at;
        let content = parser.template.content(content_at);
        match self.token_type {
            UrlTokenType::Numeric => parse_numeric(content, self.at),
            UrlTokenType::Text => Ok(TagElement::Text(Text::new(content_at))),
            UrlTokenType::TranslatedText => Ok(TagElement::TranslatedText(Text::new(content_at))),
            UrlTokenType::Variable => parser.parse_variable(content, content_at, start),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Url {
    pub view_name: TagElement,
    pub args: Vec<TagElement>,
    pub kwargs: Vec<(String, TagElement)>,
    pub variable: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum IfCondition {
    Variable(TagElement),
    And(Box<(IfCondition, IfCondition)>),
    Or(Box<(IfCondition, IfCondition)>),
    Not(Box<IfCondition>),
    Equal(Box<(IfCondition, IfCondition)>),
    NotEqual(Box<(IfCondition, IfCondition)>),
    LessThan(Box<(IfCondition, IfCondition)>),
    GreaterThan(Box<(IfCondition, IfCondition)>),
    LessThanEqual(Box<(IfCondition, IfCondition)>),
    GreaterThanEqual(Box<(IfCondition, IfCondition)>),
    In(Box<(IfCondition, IfCondition)>),
    NotIn(Box<(IfCondition, IfCondition)>),
    Is(Box<(IfCondition, IfCondition)>),
    IsNot(Box<(IfCondition, IfCondition)>),
}

fn parse_if_condition(
    parser: &mut Parser,
    parts: TagParts,
    at: (usize, usize),
) -> Result<IfCondition, ParseError> {
    let mut lexer = IfConditionLexer::new(parser.template, parts).peekable();
    if lexer.peek().is_none() {
        return Err(ParseError::MissingBooleanExpression { at: at.into() });
    }
    parse_if_binding_power(parser, &mut lexer, 0, at)
}

fn parse_if_binding_power(
    parser: &mut Parser,
    lexer: &mut Peekable<IfConditionLexer>,
    min_binding_power: u8,
    at: (usize, usize),
) -> Result<IfCondition, ParseError> {
    let token = match lexer.next().transpose()? {
        Some(token) => token,
        None => return Err(ParseError::UnexpectedEndExpression { at: at.into() }),
    };
    let content = parser.template.content(token.at);
    let token_at = token.content_at();
    let mut lhs = match token.token_type {
        IfConditionTokenType::Atom(IfConditionAtom::Numeric) => {
            IfCondition::Variable(parse_numeric(content, token_at)?)
        }
        IfConditionTokenType::Atom(IfConditionAtom::Text) => {
            IfCondition::Variable(TagElement::Text(Text::new(token_at)))
        }
        IfConditionTokenType::Atom(IfConditionAtom::TranslatedText) => {
            IfCondition::Variable(TagElement::TranslatedText(Text::new(token_at)))
        }
        IfConditionTokenType::Atom(IfConditionAtom::Variable) => {
            IfCondition::Variable(parser.parse_variable(content, token_at, token.at.0)?)
        }
        IfConditionTokenType::Not => {
            let if_condition = parse_if_binding_power(parser, lexer, NOT_BINDING_POWER, token_at)?;
            IfCondition::Not(Box::new(if_condition))
        }
        _ => {
            return Err(ParseError::InvalidIfPosition {
                at: token.at.into(),
                token: content.to_string(),
            });
        }
    };

    loop {
        let token = match lexer.peek() {
            None => break,
            Some(Err(e)) => return Err(e.clone().into()),
            Some(Ok(token)) => token,
        };
        let operator = match &token.token_type {
            IfConditionTokenType::Atom(_) | IfConditionTokenType::Not => {
                return Err(ParseError::UnusedExpression {
                    at: token.at.into(),
                    expression: parser.template.content(token.at).to_string(),
                });
            }
            IfConditionTokenType::Operator(operator) => *operator,
        };
        let binding_power = operator.binding_power();
        if binding_power <= min_binding_power {
            break;
        }

        // We can get the next token properly now, since we have the right binding
        // power and don't need to `break`.
        let token = lexer
            .next()
            .expect("already `break`ed in match peek()")
            .expect("already `return Err` in match peek()");
        let rhs = parse_if_binding_power(parser, lexer, binding_power, token.at)?;

        lhs = operator.build_condition(lhs, rhs)
    }

    Ok(lhs)
}

const NOT_BINDING_POWER: u8 = 8;

impl IfConditionOperator {
    fn binding_power(&self) -> u8 {
        match self {
            Self::Or => 6,
            Self::And => 7,
            Self::In => 9,
            Self::NotIn => 9,
            Self::Is => 10,
            Self::IsNot => 10,
            Self::Equal => 10,
            Self::NotEqual => 10,
            Self::GreaterThan => 10,
            Self::GreaterThanEqual => 10,
            Self::LessThan => 10,
            Self::LessThanEqual => 10,
        }
    }

    fn build_condition(&self, lhs: IfCondition, rhs: IfCondition) -> IfCondition {
        let inner = Box::new((lhs, rhs));
        match self {
            Self::And => IfCondition::And(inner),
            Self::Or => IfCondition::Or(inner),
            Self::In => IfCondition::In(inner),
            Self::NotIn => IfCondition::NotIn(inner),
            Self::Is => IfCondition::Is(inner),
            Self::IsNot => IfCondition::IsNot(inner),
            Self::Equal => IfCondition::Equal(inner),
            Self::NotEqual => IfCondition::NotEqual(inner),
            Self::GreaterThan => IfCondition::GreaterThan(inner),
            Self::GreaterThanEqual => IfCondition::GreaterThanEqual(inner),
            Self::LessThan => IfCondition::LessThan(inner),
            Self::LessThanEqual => IfCondition::LessThanEqual(inner),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Autoescape {
        enabled: AutoescapeEnabled,
        nodes: Vec<TokenTree>,
    },
    If {
        condition: IfCondition,
        truthy: Vec<TokenTree>,
        falsey: Option<Vec<TokenTree>>,
    },
    Load,
    Url(Url),
}

#[derive(PartialEq, Eq)]
enum EndTagType {
    Autoescape,
    Elif,
    Else,
    EndIf,
    Verbatim,
}

impl EndTagType {
    fn as_str(&self) -> &'static str {
        match self {
            EndTagType::Autoescape => "endautoescape",
            EndTagType::Elif => "elif",
            EndTagType::Else => "else",
            EndTagType::EndIf => "endif",
            EndTagType::Verbatim => "endverbatim",
        }
    }
}

#[derive(PartialEq, Eq)]
struct EndTag {
    at: (usize, usize),
    end: EndTagType,
    parts: TagParts,
}

impl EndTag {
    fn as_str(&self) -> &'static str {
        self.end.as_str()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenTree {
    Text(Text),
    TranslatedText(Text),
    Int(BigInt),
    Float(f64),
    Tag(Tag),
    Variable(Variable),
    Filter(Box<Filter>),
}

impl From<TagElement> for TokenTree {
    fn from(tag_element: TagElement) -> Self {
        match tag_element {
            TagElement::Text(text) => Self::Text(text),
            TagElement::TranslatedText(text) => Self::TranslatedText(text),
            TagElement::Variable(variable) => Self::Variable(variable),
            TagElement::Filter(filter) => Self::Filter(filter),
            TagElement::Int(n) => Self::Int(n),
            TagElement::Float(f) => Self::Float(f),
        }
    }
}

#[derive(Error, Debug, Diagnostic, PartialEq, Eq)]
pub enum ParseError {
    #[error("Empty block tag")]
    EmptyTag {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Empty variable tag")]
    EmptyVariable {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Expected an argument")]
    MissingArgument {
        #[label("here")]
        at: SourceSpan,
    },
    #[error(transparent)]
    #[diagnostic(transparent)]
    AutoescapeError(#[from] AutoescapeError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    BlockError(#[from] TagLexerError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    LexerError(#[from] LexerError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    UrlLexerError(#[from] UrlLexerError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    VariableError(#[from] VariableLexerError),
    #[error("Invalid filter: '{filter}'")]
    InvalidFilter {
        filter: String,
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Not expecting '{token}' in this position")]
    InvalidIfPosition {
        token: String,
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Invalid numeric literal")]
    InvalidNumber {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Missing boolean expression")]
    MissingBooleanExpression {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Unclosed '{start}' tag. Looking for one of: {expected}")]
    MissingEndTag {
        start: &'static str,
        expected: String,
        #[label("started here")]
        at: SourceSpan,
    },
    #[error("'{tag}' is not a valid tag or filter in tag library '{library}'")]
    MissingFilterTag {
        tag: String,
        library: String,
        #[label("tag or filter")]
        tag_at: SourceSpan,
        #[label("library")]
        library_at: SourceSpan,
    },
    #[error("'{library}' is not a registered tag library.")]
    MissingTagLibrary {
        library: String,
        #[label("here")]
        at: SourceSpan,
        #[help]
        help: String,
    },
    #[error("Cannot mix arguments and keyword arguments")]
    MixedArgsKwargs {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("'url' view name must be a string or variable, not a number")]
    NumericUrlName {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("{filter} filter does not take an argument")]
    UnexpectedArgument {
        filter: &'static str,
        #[label("unexpected argument")]
        at: SourceSpan,
    },
    #[error("Unexpected end of expression")]
    UnexpectedEndExpression {
        #[label("after this")]
        at: SourceSpan,
    },
    #[error("Unexpected tag {unexpected}")]
    UnexpectedEndTag {
        unexpected: &'static str,
        #[label("unexpected tag")]
        at: SourceSpan,
    },
    #[error("Unused expression '{expression}' in if tag")]
    UnusedExpression {
        expression: String,
        #[label("here")]
        at: SourceSpan,
    },
    #[error("'url' takes at least one argument, a URL pattern name")]
    UrlTagNoArguments {
        #[label("here")]
        at: SourceSpan,
    },
    #[error("Unexpected tag {unexpected}, expected {expected}")]
    WrongEndTag {
        unexpected: &'static str,
        expected: String,
        #[label("unexpected tag")]
        at: SourceSpan,
        #[label("start tag")]
        start_at: SourceSpan,
    },
}

#[derive(Error, Debug)]
pub enum PyParseError {
    #[error(transparent)]
    PyErr(#[from] PyErr),
    #[error(transparent)]
    ParseError(#[from] ParseError),
}

impl PyParseError {
    pub fn try_into_parse_error(self) -> Result<ParseError, PyErr> {
        match self {
            Self::ParseError(err) => Ok(err),
            Self::PyErr(err) => Err(err),
        }
    }

    #[cfg(test)]
    pub fn unwrap_parse_error(self) -> ParseError {
        match self {
            Self::ParseError(err) => err,
            Self::PyErr(err) => panic!("{err:?}"),
        }
    }
}

impl LoadToken {
    fn load_library<'l, 'py>(
        &self,
        py: Python<'py>,
        libraries: &'l HashMap<String, Py<PyAny>>,
        template: TemplateString<'_>,
    ) -> Result<&'l Bound<'py, PyAny>, ParseError> {
        let library_name = template.content(self.at);
        match libraries.get(library_name) {
            Some(library) => Ok(library.bind(py)),
            None => {
                let mut libraries: Vec<_> = libraries.keys().map(String::as_str).collect();
                libraries.sort_unstable();
                let help = format!("Must be one of:\n{}", libraries.join("\n"));
                Err(ParseError::MissingTagLibrary {
                    at: self.at.into(),
                    library: library_name.to_string(),
                    help,
                })
            }
        }
    }
}

pub struct Parser<'t, 'l, 'py> {
    py: Python<'py>,
    template: TemplateString<'t>,
    lexer: Lexer<'t>,
    libraries: &'l HashMap<String, Py<PyAny>>,
    external_tags: HashMap<String, Bound<'py, PyAny>>,
    external_filters: HashMap<String, Bound<'py, PyAny>>,
}

impl<'t, 'l, 'py> Parser<'t, 'l, 'py> {
    pub fn new(
        py: Python<'py>,
        template: TemplateString<'t>,
        libraries: &'l HashMap<String, Py<PyAny>>,
    ) -> Self {
        Self {
            py,
            template,
            lexer: Lexer::new(template),
            libraries,
            external_tags: HashMap::new(),
            external_filters: HashMap::new(),
        }
    }

    #[cfg(test)]
    fn new_with_filters(
        py: Python<'py>,
        template: TemplateString<'t>,
        libraries: &'l HashMap<String, Py<PyAny>>,
        external_filters: HashMap<String, Bound<'py, PyAny>>,
    ) -> Self {
        Self {
            py,
            template,
            lexer: Lexer::new(template),
            libraries,
            external_tags: HashMap::new(),
            external_filters,
        }
    }

    pub fn parse(&mut self) -> Result<Vec<TokenTree>, PyParseError> {
        let mut nodes = Vec::new();
        while let Some(token) = self.lexer.next() {
            let node = match token.token_type {
                TokenType::Text => TokenTree::Text(Text::new(token.at)),
                TokenType::Comment => continue,
                TokenType::Variable => self
                    .parse_variable(
                        token.content(self.template),
                        token.at,
                        token.at.0 + START_TAG_LEN,
                    )?
                    .into(),
                TokenType::Tag => match self.parse_tag(token.content(self.template), token.at)? {
                    Either::Left(token_tree) => token_tree,
                    Either::Right(end_tag) => {
                        return Err(ParseError::UnexpectedEndTag {
                            at: end_tag.at.into(),
                            unexpected: end_tag.as_str(),
                        }
                        .into());
                    }
                },
            };
            nodes.push(node)
        }
        Ok(nodes)
    }

    fn parse_until(
        &mut self,
        until: Vec<EndTagType>,
        start: &'static str,
        start_at: (usize, usize),
    ) -> Result<(Vec<TokenTree>, EndTag), PyParseError> {
        let mut nodes = Vec::new();
        while let Some(token) = self.lexer.next() {
            let node = match token.token_type {
                TokenType::Text => TokenTree::Text(Text::new(token.at)),
                TokenType::Comment => continue,
                TokenType::Variable => self
                    .parse_variable(
                        token.content(self.template),
                        token.at,
                        token.at.0 + START_TAG_LEN,
                    )?
                    .into(),
                TokenType::Tag => match self.parse_tag(token.content(self.template), token.at)? {
                    Either::Left(token_tree) => token_tree,
                    Either::Right(end_tag) => {
                        if until.contains(&end_tag.end) {
                            return Ok((nodes, end_tag));
                        } else {
                            return Err(ParseError::WrongEndTag {
                                expected: until
                                    .iter()
                                    .map(|u| u.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                unexpected: end_tag.as_str(),
                                at: end_tag.at.into(),
                                start_at: start_at.into(),
                            }
                            .into());
                        }
                    }
                },
            };
            nodes.push(node)
        }
        Err(ParseError::MissingEndTag {
            start,
            expected: until
                .iter()
                .map(|u| u.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            at: start_at.into(),
        }
        .into())
    }

    fn parse_variable(
        &self,
        variable: &str,
        at: (usize, usize),
        start: usize,
    ) -> Result<TagElement, ParseError> {
        let (variable_token, filter_lexer) = match lex_variable(variable, start)? {
            None => return Err(ParseError::EmptyVariable { at: at.into() }),
            Some(t) => t,
        };
        let mut var = match variable_token.token_type {
            VariableTokenType::Variable => TagElement::Variable(Variable::new(variable_token.at)),
            VariableTokenType::Int(n) => TagElement::Int(n),
            VariableTokenType::Float(f) => TagElement::Float(f),
        };
        for filter_token in filter_lexer {
            let filter_token = filter_token?;
            let argument = match filter_token.argument {
                None => None,
                Some(ref a) => Some(a.parse(self.template)?),
            };
            let filter = Filter::new(self, filter_token.at, var, argument)?;
            var = TagElement::Filter(Box::new(filter));
        }
        Ok(var)
    }

    fn parse_tag(
        &mut self,
        tag: &'t str,
        at: (usize, usize),
    ) -> Result<Either<TokenTree, EndTag>, PyParseError> {
        let maybe_tag = match lex_tag(tag, at.0 + START_TAG_LEN) {
            Ok(maybe_tag) => maybe_tag,
            Err(e) => {
                let parse_error: ParseError = e.into();
                return Err(parse_error.into());
            }
        };
        let (tag, parts) = match maybe_tag {
            None => return Err(ParseError::EmptyTag { at: at.into() }.into()),
            Some(t) => t,
        };
        Ok(match self.template.content(tag.at) {
            "url" => Either::Left(self.parse_url(at, parts)?),
            "load" => Either::Left(self.parse_load(at, parts)?),
            "autoescape" => Either::Left(self.parse_autoescape(at, parts)?),
            "endautoescape" => Either::Right(EndTag {
                end: EndTagType::Autoescape,
                at,
                parts,
            }),
            "endverbatim" => Either::Right(EndTag {
                end: EndTagType::Verbatim,
                at,
                parts,
            }),
            "if" => Either::Left(self.parse_if(at, parts, "if")?),
            "elif" => Either::Right(EndTag {
                end: EndTagType::Elif,
                at,
                parts,
            }),
            "else" => Either::Right(EndTag {
                end: EndTagType::Else,
                at,
                parts,
            }),
            "endif" => Either::Right(EndTag {
                end: EndTagType::EndIf,
                at,
                parts,
            }),
            _ => todo!(),
        })
    }

    fn parse_load(
        &mut self,
        _at: (usize, usize),
        parts: TagParts,
    ) -> Result<TokenTree, PyParseError> {
        let tokens: Vec<_> = LoadLexer::new(self.template, parts).collect();
        let mut rev = tokens.iter().rev();
        if let (Some(last), Some(prev)) = (rev.next(), rev.next())
            && self.template.content(prev.at) == "from"
        {
            let library = last.load_library(self.py, self.libraries, self.template)?;
            let filters = self.get_filters(library)?;
            let tags = self.get_tags(library)?;
            for token in rev {
                let content = self.template.content(token.at);
                if let Some(filter) = filters.get(content) {
                    self.external_filters
                        .insert(content.to_string(), filter.clone());
                } else if let Some(tag) = tags.get(content) {
                    self.external_tags.insert(content.to_string(), tag.clone());
                } else {
                    return Err(ParseError::MissingFilterTag {
                        library: self.template.content(last.at).to_string(),
                        library_at: last.at.into(),
                        tag: content.to_string(),
                        tag_at: token.at.into(),
                    }
                    .into());
                }
            }
            return Ok(TokenTree::Tag(Tag::Load));
        }
        for token in tokens {
            let library = token.load_library(self.py, self.libraries, self.template)?;
            let filters = self.get_filters(library)?;
            let tags = self.get_tags(library)?;
            self.external_filters.extend(filters);
            self.external_tags.extend(tags);
        }
        Ok(TokenTree::Tag(Tag::Load))
    }

    fn get_tags(
        &mut self,
        library: &Bound<'py, PyAny>,
    ) -> Result<HashMap<String, Bound<'py, PyAny>>, PyErr> {
        library.getattr(intern!(self.py, "tags"))?.extract()
    }

    fn get_filters(
        &mut self,
        library: &Bound<'py, PyAny>,
    ) -> Result<HashMap<String, Bound<'py, PyAny>>, PyErr> {
        library.getattr(intern!(self.py, "filters"))?.extract()
    }

    fn parse_url(&mut self, at: (usize, usize), parts: TagParts) -> Result<TokenTree, ParseError> {
        let mut lexer = UrlLexer::new(self.template, parts);
        let view_name = match lexer.next() {
            Some(view_token) => view_token?.parse(self)?,
            None => return Err(ParseError::UrlTagNoArguments { at: at.into() }),
        };

        let mut tokens = vec![];
        for token in lexer {
            tokens.push(token?);
        }
        let mut rev = tokens.iter().rev();
        let variable = match (rev.next(), rev.next()) {
            (
                Some(UrlToken {
                    at: last,
                    token_type: UrlTokenType::Variable,
                    ..
                }),
                Some(UrlToken {
                    at: prev,
                    token_type: UrlTokenType::Variable,
                    ..
                }),
            ) => {
                let prev = self.template.content(*prev);
                if prev == "as" {
                    Some(self.template.content(*last).to_string())
                } else {
                    None
                }
            }
            _ => None,
        };
        if variable.is_some() {
            tokens.truncate(tokens.len() - 2)
        }
        let mut args = vec![];
        let mut kwargs = vec![];
        for token in tokens {
            let element = token.parse(self)?;
            match token.kwarg {
                None => args.push(element),
                Some(at) => {
                    let kwarg = self.template.content(at).to_string();
                    kwargs.push((kwarg, element));
                }
            }
        }
        if !args.is_empty() && !kwargs.is_empty() {
            return Err(ParseError::MixedArgsKwargs { at: at.into() });
        }
        let url = Url {
            view_name,
            args,
            kwargs,
            variable,
        };
        Ok(TokenTree::Tag(Tag::Url(url)))
    }

    fn parse_autoescape(
        &mut self,
        at: (usize, usize),
        parts: TagParts,
    ) -> Result<TokenTree, PyParseError> {
        let token = lex_autoescape_argument(self.template, parts).map_err(ParseError::from)?;
        let (nodes, _) = self.parse_until(vec![EndTagType::Autoescape], "autoescape", at)?;
        Ok(TokenTree::Tag(Tag::Autoescape {
            enabled: token.enabled,
            nodes,
        }))
    }

    fn parse_if(
        &mut self,
        at: (usize, usize),
        parts: TagParts,
        start: &'static str,
    ) -> Result<TokenTree, PyParseError> {
        let condition = parse_if_condition(self, parts, at)?;
        let (nodes, end_tag) = self.parse_until(
            vec![EndTagType::Elif, EndTagType::Else, EndTagType::EndIf],
            start,
            at,
        )?;
        let falsey = match end_tag {
            EndTag {
                at,
                end: EndTagType::Elif,
                parts,
            } => Some(vec![self.parse_if(at, parts, "elif")?]),
            EndTag {
                at,
                end: EndTagType::Else,
                parts: _parts,
            } => {
                let (nodes, _) = self.parse_until(vec![EndTagType::EndIf], "else", at)?;
                Some(nodes)
            }
            EndTag {
                at: _end_at,
                end: EndTagType::EndIf,
                parts: _parts,
            } => None,
            _ => unreachable!(),
        };
        Ok(TokenTree::Tag(Tag::If {
            condition,
            truthy: nodes,
            falsey,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::types::{PyDict, PyDictMethods};
    use std::sync::Arc;

    use crate::lex::common::LexerError;
    use crate::{
        filters::{DefaultFilter, ExternalFilter, LowerFilter},
        template::django_rusty_templates::{EngineData, Template},
    };

    fn get_external_filter(node: &TokenTree) -> Arc<Py<PyAny>> {
        match node {
            TokenTree::Filter(filter) => match &filter.filter {
                FilterType::External(filter) => filter.filter.clone(),
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    fn get_external_filter_tag_element(node: &TokenTree) -> Arc<Py<PyAny>> {
        match node {
            TokenTree::Filter(filter) => match &filter.left {
                TagElement::Filter(filter) => match &filter.filter {
                    FilterType::External(filter) => filter.filter.clone(),
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    #[test]
    fn test_empty_template() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();
            assert_eq!(nodes, vec![]);
        })
    }

    #[test]
    fn test_text() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "Some text";
            let template_string = TemplateString(template);
            let mut parser = Parser::new(py, template_string, &libraries);
            let nodes = parser.parse().unwrap();
            let text = Text::new((0, template.len()));
            assert_eq!(nodes, vec![TokenTree::Text(text)]);
            assert_eq!(template_string.content(text.at), template);
        })
    }

    #[test]
    fn test_comment() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{# A commment #}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();
            assert_eq!(nodes, vec![]);
        })
    }

    #[test]
    fn test_empty_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ }}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::EmptyVariable { at: (0, 5).into() });
        })
    }

    #[test]
    fn test_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = TemplateString("{{ foo }}");
            let mut parser = Parser::new(py, template, &libraries);
            let nodes = parser.parse().unwrap();
            let variable = Variable { at: (3, 3) };
            assert_eq!(nodes, vec![TokenTree::Variable(variable)]);
            assert_eq!(
                variable.parts(template).collect::<Vec<_>>(),
                vec![("foo", (3, 3))]
            );
        })
    }

    #[test]
    fn test_variable_attribute() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = TemplateString("{{ foo.bar.baz }}");
            let mut parser = Parser::new(py, template, &libraries);
            let nodes = parser.parse().unwrap();
            let variable = Variable { at: (3, 11) };
            assert_eq!(nodes, vec![TokenTree::Variable(variable)]);
            assert_eq!(
                variable.parts(template).collect::<Vec<_>>(),
                vec![("foo", (3, 3)), ("bar", (7, 3)), ("baz", (11, 3))]
            );
        })
    }

    #[test]
    fn test_filter() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = TemplateString("{{ foo|bar }}");
            let mut parser = Parser::new_with_filters(py, template, &libraries, filters);
            let nodes = parser.parse().unwrap();

            assert_eq!(nodes.len(), 1);

            let foo = Variable { at: (3, 3) };
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: TagElement::Variable(foo),
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: None,
                }),
            }));
            assert_eq!(nodes, vec![bar]);
            assert_eq!(
                foo.parts(template).collect::<Vec<_>>(),
                vec![("foo", (3, 3))]
            );
        })
    }

    #[test]
    fn test_unknown_filter() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = TemplateString("{{ foo|bar }}");
            let mut parser = Parser::new(py, template, &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(
                error,
                ParseError::InvalidFilter {
                    filter: "bar".to_string(),
                    at: (7, 3).into()
                }
            );
        })
    }

    #[test]
    fn test_filter_multiple() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ foo|bar|baz }}";
            let filters = HashMap::from([
                ("bar".to_string(), py.None().bind(py).clone()),
                ("baz".to_string(), py.None().bind(py).clone()),
            ]);
            let mut parser = Parser::new_with_filters(py, template.into(), &libraries, filters);
            let nodes = parser.parse().unwrap();
            assert_eq!(nodes.len(), 1);

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let external = get_external_filter_tag_element(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TagElement::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: None,
                }),
            }));
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let baz = TokenTree::Filter(Box::new(Filter {
                at: (11, 3),
                left: bar,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: None,
                }),
            }));
            assert_eq!(nodes, vec![baz]);
        })
    }

    #[test]
    fn test_filter_argument() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = TemplateString("{{ foo|bar:baz }}");
            let mut parser = Parser::new_with_filters(py, template, &libraries, filters);
            let nodes = parser.parse().unwrap();
            assert_eq!(nodes.len(), 1);

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let baz = Variable { at: (11, 3) };
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(Argument {
                        at: (11, 3),
                        argument_type: ArgumentType::Variable(baz),
                    }),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
            assert_eq!(
                baz.parts(template).collect::<Vec<_>>(),
                vec![("baz", (11, 3))]
            );
        })
    }

    #[test]
    fn test_filter_argument_text() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = TemplateString("{{ foo|bar:'baz' }}");
            let mut parser = Parser::new_with_filters(py, template, &libraries, filters);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let baz = Text::new((12, 3));
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(Argument {
                        at: (11, 5),
                        argument_type: ArgumentType::Text(baz),
                    }),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
            assert_eq!(template.content(baz.at), "baz");
        })
    }

    #[test]
    fn test_filter_argument_translated_text() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = TemplateString("{{ foo|bar:_('baz') }}");
            let mut parser = Parser::new_with_filters(py, template, &libraries, filters);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let baz = TranslatedText::new((14, 3));
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(Argument {
                        at: (11, 8),
                        argument_type: ArgumentType::TranslatedText(baz),
                    }),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
            assert_eq!(template.content(baz.at), "baz");
        })
    }

    #[test]
    fn test_filter_argument_float() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = "{{ foo|bar:5.2e3 }}";
            let mut parser = Parser::new_with_filters(py, template.into(), &libraries, filters);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let num = Argument {
                at: (11, 5),
                argument_type: ArgumentType::Float(5.2e3),
            };
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(num),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
        })
    }

    #[test]
    fn test_filter_argument_int() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = "{{ foo|bar:99 }}";
            let mut parser = Parser::new_with_filters(py, template.into(), &libraries, filters);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let num = Argument {
                at: (11, 2),
                argument_type: ArgumentType::Int(99.into()),
            };
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(num),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
        })
    }

    #[test]
    fn test_filter_argument_bigint() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let filters = HashMap::from([("bar".to_string(), py.None().bind(py).clone())]);
            let template = "{{ foo|bar:99999999999999999 }}";
            let mut parser = Parser::new_with_filters(py, template.into(), &libraries, filters);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let num = Argument {
                at: (11, 17),
                argument_type: ArgumentType::Int("99999999999999999".parse::<BigInt>().unwrap()),
            };
            let external = get_external_filter(&nodes[0]);
            assert!(external.is_none(py));
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 3),
                left: foo,
                filter: FilterType::External(ExternalFilter {
                    filter: external,
                    argument: Some(num),
                }),
            }));
            assert_eq!(nodes, vec![bar]);
        })
    }

    #[test]
    fn test_filter_argument_invalid_number() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ foo|bar:9.9.9 }}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::InvalidNumber { at: (11, 5).into() });
        })
    }

    #[test]
    fn test_filter_parse_addslashes() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let engine = EngineData::empty();
            let template_string = "{{ foo|addslashes }}".to_string();
            let context = PyDict::new(py);
            context.set_item("bar", "").unwrap();
            let template = Template::new_from_string(py, template_string, &engine).unwrap();
            let result = template.render(py, Some(context), None).unwrap();

            assert_eq!(result, "");

            let context = PyDict::new(py);
            context.set_item("foo", "").unwrap();
            let template_string = "{{ foo|addslashes:invalid }}".to_string();
            let error = Template::new_from_string(py, template_string, &engine).unwrap_err();

            let error_string = format!("{error}");
            assert!(error_string.contains("addslashes filter does not take an argument"));
        })
    }

    #[test]
    fn test_filter_default() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = TemplateString("{{ foo|default:baz }}");
            let mut parser = Parser::new(py, template, &libraries);
            let nodes = parser.parse().unwrap();

            let foo = TagElement::Variable(Variable { at: (3, 3) });
            let baz = Variable { at: (15, 3) };
            let bar = TokenTree::Filter(Box::new(Filter {
                at: (7, 7),
                left: foo,
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (15, 3),
                    argument_type: ArgumentType::Variable(baz),
                })),
            }));
            assert_eq!(nodes, vec![bar]);
            assert_eq!(
                baz.parts(template).collect::<Vec<_>>(),
                vec![("baz", (15, 3))]
            );
        })
    }

    #[test]
    fn test_filter_default_missing_argument() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ foo|default|baz }}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::MissingArgument { at: (7, 7).into() });
        })
    }

    #[test]
    fn test_filter_lower_unexpected_argument() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ foo|lower:baz }}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(
                error,
                ParseError::UnexpectedArgument {
                    filter: "lower",
                    at: (13, 3).into()
                }
            );
        })
    }

    #[test]
    fn test_variable_lexer_error() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{{ _foo }}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(
                error,
                ParseError::VariableError(
                    LexerError::InvalidVariableName { at: (3, 4).into() }.into()
                )
            );
        })
    }

    #[test]
    fn test_parse_empty_tag() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{%  %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::EmptyTag { at: (0, 6).into() });
        })
    }

    #[test]
    fn test_block_error() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url'foo' %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(
                error,
                ParseError::BlockError(TagLexerError::InvalidTagName { at: (3, 8).into() })
            );
        })
    }

    #[test]
    fn test_parse_url_tag() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url 'some-url-name' %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Text(Text { at: (8, 13) }),
                args: vec![],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_view_name_translated() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url _('some-url-name') %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::TranslatedText(Text { at: (10, 13) }),
                args: vec![],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_view_name_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_view_name_filter() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name|default:'home' %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let some_view_name = TagElement::Variable(Variable { at: (7, 14) });
            let home = Text { at: (31, 4) };
            let default = Box::new(Filter {
                at: (22, 7),
                left: some_view_name,
                filter: FilterType::Default(DefaultFilter::new(Argument {
                    at: (30, 6),
                    argument_type: ArgumentType::Text(home),
                })),
            });
            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Filter(default),
                args: vec![],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_no_arguments() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::UrlTagNoArguments { at: (0, 9).into() });
        })
    }

    #[test]
    fn test_parse_url_view_name_integer() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url 64 %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Int(64.into()),
                args: vec![],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_arguments() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name 'foo' bar|default:'home' 64 5.7 _(\"spam\") %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![
                    TagElement::Text(Text { at: (23, 3) }),
                    TagElement::Filter(Box::new(Filter {
                        at: (32, 7),
                        left: TagElement::Variable(Variable { at: (28, 3) }),
                        filter: FilterType::Default(DefaultFilter::new(Argument {
                            at: (40, 6),
                            argument_type: ArgumentType::Text(Text { at: (41, 4) }),
                        })),
                    })),
                    TagElement::Int(64.into()),
                    TagElement::Float(5.7),
                    TagElement::TranslatedText(Text { at: (57, 4) }),
                ],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_kwargs() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name foo='foo' extra=-64 %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![],
                kwargs: vec![
                    ("foo".to_string(), TagElement::Text(Text { at: (27, 3) })),
                    ("extra".to_string(), TagElement::Int((-64).into())),
                ],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_arguments_as_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name 'foo' as some_url %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![TagElement::Text(Text { at: (23, 3) })],
                kwargs: vec![],
                variable: Some("some_url".to_string()),
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_kwargs_as_variable() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name foo='foo' as some_url %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![],
                kwargs: vec![("foo".to_string(), TagElement::Text(Text { at: (27, 3) }))],
                variable: Some("some_url".to_string()),
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_arguments_last_variables() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name 'foo' arg arg2 %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let nodes = parser.parse().unwrap();

            let url = TokenTree::Tag(Tag::Url(Url {
                view_name: TagElement::Variable(Variable { at: (7, 14) }),
                args: vec![
                    TagElement::Text(Text { at: (23, 3) }),
                    TagElement::Variable(Variable { at: (28, 3) }),
                    TagElement::Variable(Variable { at: (32, 4) }),
                ],
                kwargs: vec![],
                variable: None,
            }));

            assert_eq!(nodes, vec![url]);
        })
    }

    #[test]
    fn test_parse_url_tag_mixed_args_kwargs() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url some_view_name 'foo' arg name=arg2 %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(
                error,
                ParseError::MixedArgsKwargs {
                    at: (0, template.len()).into()
                }
            );
        })
    }

    #[test]
    fn test_parse_url_tag_invalid_number() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            let libraries = HashMap::new();
            let template = "{% url foo 9.9.9 %}";
            let mut parser = Parser::new(py, template.into(), &libraries);
            let error = parser.parse().unwrap_err().unwrap_parse_error();
            assert_eq!(error, ParseError::InvalidNumber { at: (11, 5).into() });
        })
    }

    #[test]
    fn test_filter_type_partial_eq() {
        pyo3::prepare_freethreaded_python();

        Python::with_gil(|py| {
            assert_eq!(
                FilterType::Lower(LowerFilter),
                FilterType::Lower(LowerFilter)
            );
            assert_ne!(
                FilterType::External(ExternalFilter::new(py.None(), None)),
                FilterType::External(ExternalFilter::new(py.None(), None))
            );
            assert_ne!(
                FilterType::Lower(LowerFilter),
                FilterType::Default(DefaultFilter::new(Argument {
                    at: (0, 3),
                    argument_type: ArgumentType::Float(1.0)
                }))
            );
        })
    }
}
