use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

use crate::lex::common::{LexerError, lex_numeric, lex_text, lex_translated, lex_variable};
use crate::lex::tag::TagParts;
use crate::types::TemplateString;

#[derive(Clone, Error, Debug, Diagnostic, PartialEq, Eq)]
pub enum ForLexerError {
    #[error(transparent)]
    LexerError(#[from] LexerError),
    #[error("Invalid variable name {name} in for loop:")]
    InvalidName {
        name: String,
        #[label("invalid variable name")]
        at: SourceSpan,
    },
    #[error("Unexpected expression in for loop. Did you miss a comma when unpacking?")]
    MissingComma {
        #[label("unexpected expression")]
        at: SourceSpan,
    },
    #[error("Expected the 'in' keyword or a variable name:")]
    MissingIn {
        #[label("after this name")]
        at: SourceSpan,
    },
    #[error("Expected an expression after the 'in' keyword:")]
    MissingExpression {
        #[label("after this keyword")]
        at: SourceSpan,
    },
    #[error("Unexpected expression in for loop:")]
    UnexpectedExpression {
        #[label("unexpected expression")]
        at: SourceSpan,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ForTokenType {
    Numeric,
    Text,
    TranslatedText,
    Variable,
    VariableName,
    In,
    Reversed,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ForToken {
    pub at: (usize, usize),
    pub token_type: ForTokenType,
}

enum State {
    VariableName,
    In,
    Variable,
    Reversed,
    Done,
}

pub struct ForLexer<'t> {
    rest: &'t str,
    byte: usize,
    state: State,
    previous_at: Option<(usize, usize)>,
}

trait NextChar {
    fn next_whitespace(&self) -> usize;
    fn next_non_whitespace(&self) -> usize;
}

impl NextChar for str {
    fn next_whitespace(&self) -> usize {
        self.find(char::is_whitespace).unwrap_or(self.len())
    }

    fn next_non_whitespace(&self) -> usize {
        self.find(|c: char| !c.is_whitespace())
            .unwrap_or(self.len())
    }
}

impl<'t> ForLexer<'t> {
    pub fn new(template: TemplateString<'t>, parts: TagParts) -> Self {
        Self {
            rest: template.content(parts.at),
            byte: parts.at.0,
            state: State::VariableName,
            previous_at: None,
        }
    }

    fn lex_expression(&mut self) -> Result<ForToken, ForLexerError> {
        let mut chars = self.rest.chars();
        let token = match chars.next().expect("self.rest is not empty") {
            '_' => {
                if let Some('(') = chars.next() {
                    self.lex_translated(&mut chars)?
                } else {
                    self.lex_variable()
                }
            }
            '"' => self.lex_text(&mut chars, '"')?,
            '\'' => self.lex_text(&mut chars, '\'')?,
            '0'..='9' | '-' => self.lex_numeric(),
            _ => self.lex_variable(),
        };
        self.lex_remainder()?;
        Ok(token)
    }

    fn lex_variable(&mut self) -> ForToken {
        let (at, byte, rest) = lex_variable(self.byte, self.rest);
        self.rest = rest;
        self.byte = byte;
        self.previous_at = Some(at);
        ForToken {
            token_type: ForTokenType::Variable,
            at,
        }
    }

    fn lex_numeric(&mut self) -> ForToken {
        let (at, byte, rest) = lex_numeric(self.byte, self.rest);
        self.rest = rest;
        self.byte = byte;
        self.previous_at = Some(at);
        ForToken {
            at,
            token_type: ForTokenType::Numeric,
        }
    }

    fn lex_text(
        &mut self,
        chars: &mut std::str::Chars,
        end: char,
    ) -> Result<ForToken, ForLexerError> {
        match lex_text(self.byte, self.rest, chars, end) {
            Ok((at, byte, rest)) => {
                self.rest = rest;
                self.byte = byte;
                self.previous_at = Some(at);
                Ok(ForToken {
                    token_type: ForTokenType::Text,
                    at,
                })
            }
            Err(e) => {
                self.rest = "";
                Err(e.into())
            }
        }
    }

    fn lex_translated(&mut self, chars: &mut std::str::Chars) -> Result<ForToken, ForLexerError> {
        match lex_translated(self.byte, self.rest, chars) {
            Ok((at, byte, rest)) => {
                self.rest = rest;
                self.byte = byte;
                self.previous_at = Some(at);
                Ok(ForToken {
                    token_type: ForTokenType::TranslatedText,
                    at,
                })
            }
            Err(e) => {
                self.rest = "";
                Err(e.into())
            }
        }
    }

    fn lex_remainder(&mut self) -> Result<(), ForLexerError> {
        let remainder = self
            .rest
            .find(char::is_whitespace)
            .unwrap_or(self.rest.len());
        match remainder {
            0 => {
                let rest = self.rest.trim_start();
                self.byte += self.rest.len() - rest.len();
                self.rest = rest;
                Ok(())
            }
            n => {
                self.rest = "";
                let at = (self.byte, n).into();
                Err(LexerError::InvalidRemainder { at }.into())
            }
        }
    }
}

impl Iterator for ForLexer<'_> {
    type Item = Result<ForToken, ForLexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            match self.state {
                State::In => {
                    self.state = State::Done;
                    return Some(Err(ForLexerError::MissingIn {
                        at: self.previous_at.expect("previous_at is set").into(),
                    }));
                }
                State::Variable => {
                    self.state = State::Done;
                    return Some(Err(ForLexerError::MissingExpression {
                        at: self.previous_at.expect("previous_at is set").into(),
                    }));
                }
                _ => return None,
            }
        }
        let index = self.rest.next_whitespace();
        match self.state {
            State::VariableName => {
                let (index, next_index) = match self.rest.find(',') {
                    Some(comma_index) if comma_index < index => {
                        let next_index = self.rest[comma_index + 1..].next_non_whitespace();
                        (comma_index, next_index + 1)
                    }
                    _ => {
                        self.state = State::In;
                        let next_index = self.rest[index..].next_non_whitespace();
                        (index, next_index)
                    }
                };
                let at = (self.byte, index);
                self.previous_at = Some(at);
                let name = &self.rest[..index];
                if name.contains(['"', '\'', '|']) {
                    self.rest = "";
                    self.state = State::Done;
                    return Some(Err(ForLexerError::InvalidName {
                        name: name.to_string(),
                        at: at.into(),
                    }));
                }
                self.byte += index + next_index;
                self.rest = &self.rest[index + next_index..];
                let token_type = ForTokenType::VariableName;
                Some(Ok(ForToken { at, token_type }))
            }
            State::In => {
                let at = (self.byte, index);
                match &self.rest[..index] {
                    "in" => {
                        self.state = State::Variable;
                        let token_type = ForTokenType::In;
                        let next_index = self.rest[index..].next_non_whitespace();
                        self.byte += index + next_index;
                        self.rest = &self.rest[index + next_index..];
                        self.previous_at = Some(at);
                        Some(Ok(ForToken { at, token_type }))
                    }
                    _ => {
                        self.rest = "";
                        self.state = State::Done;
                        Some(Err(ForLexerError::MissingComma { at: at.into() }))
                    }
                }
            }
            State::Variable => {
                self.state = State::Reversed;
                Some(self.lex_expression())
            }
            State::Reversed => {
                let at = (self.byte, index);
                match &self.rest[..index] {
                    "reversed" => {
                        self.state = State::Done;
                        let token_type = ForTokenType::Reversed;
                        let next_index = self.rest[index..].next_non_whitespace();
                        self.byte += index + next_index;
                        self.rest = &self.rest[index + next_index..];
                        self.previous_at = Some(at);
                        Some(Ok(ForToken { at, token_type }))
                    }
                    _ => {
                        self.rest = "";
                        Some(Err(ForLexerError::UnexpectedExpression { at: at.into() }))
                    }
                }
            }
            State::Done => {
                self.rest = "";
                let at = (self.byte, index);
                Some(Err(ForLexerError::UnexpectedExpression { at: at.into() }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_simple() {
        let template = "{% for foo in bar %}";
        let parts = TagParts { at: (7, 10) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 3),
            token_type: ForTokenType::Variable,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_text() {
        let template = "{% for foo in 'bar' %}";
        let parts = TagParts { at: (7, 12) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 5),
            token_type: ForTokenType::Text,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_text_double_quotes() {
        let template = "{% for foo in \"bar\" %}";
        let parts = TagParts { at: (7, 12) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 5),
            token_type: ForTokenType::Text,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_translated_text() {
        let template = "{% for foo in _('bar') %}";
        let parts = TagParts { at: (7, 15) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 8),
            token_type: ForTokenType::TranslatedText,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_underscore_expression() {
        let template = "{% for foo in _bar %}";
        let parts = TagParts { at: (7, 11) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 4),
            token_type: ForTokenType::Variable,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_int() {
        let template = "{% for foo in 123 %}";
        let parts = TagParts { at: (7, 10) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 3),
            token_type: ForTokenType::Numeric,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar)]);
    }

    #[test]
    fn test_lex_variable_names() {
        let template = "{% for foo, bar in spam %}";
        let parts = TagParts { at: (7, 16) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let bar = ForToken {
            at: (12, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (16, 2),
            token_type: ForTokenType::In,
        };
        let spam = ForToken {
            at: (19, 4),
            token_type: ForTokenType::Variable,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(bar), Ok(in_token), Ok(spam)]);
    }

    #[test]
    fn test_lex_variable_names_no_whitespace_after_comma() {
        let template = "{% for foo,bar in spam %}";
        let parts = TagParts { at: (7, 15) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let bar = ForToken {
            at: (11, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (15, 2),
            token_type: ForTokenType::In,
        };
        let spam = ForToken {
            at: (18, 4),
            token_type: ForTokenType::Variable,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(bar), Ok(in_token), Ok(spam)]);
    }

    #[test]
    fn test_lex_comma_in_text() {
        let template = "{% for foo in 'spam,' %}";
        let parts = TagParts { at: (7, 14) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let spam = ForToken {
            at: (14, 7),
            token_type: ForTokenType::Text,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(spam)]);
    }

    #[test]
    fn test_lex_reversed() {
        let template = "{% for foo in bar reversed %}";
        let parts = TagParts { at: (7, 19) };
        let lexer = ForLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 3),
            token_type: ForTokenType::Variable,
        };
        let reversed = ForToken {
            at: (18, 8),
            token_type: ForTokenType::Reversed,
        };
        assert_eq!(tokens, vec![Ok(foo), Ok(in_token), Ok(bar), Ok(reversed)]);
    }

    #[test]
    fn test_unexpected_before_in() {
        let template = "{% for foo bar in bar reversed %}";
        let parts = TagParts { at: (7, 23) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let unexpected = ForLexerError::MissingComma { at: (11, 3).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap_err(), unexpected);
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_unexpected_after_iterable() {
        let template = "{% for foo in bar invalid %}";
        let parts = TagParts { at: (7, 18) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 3),
            token_type: ForTokenType::Variable,
        };
        let unexpected = ForLexerError::UnexpectedExpression { at: (18, 7).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap(), in_token);
        assert_eq!(lexer.next().unwrap().unwrap(), bar);
        assert_eq!(lexer.next().unwrap().unwrap_err(), unexpected);
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_unexpected_after_reversed() {
        let template = "{% for foo in bar reversed invalid %}";
        let parts = TagParts { at: (7, 27) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let bar = ForToken {
            at: (14, 3),
            token_type: ForTokenType::Variable,
        };
        let reversed = ForToken {
            at: (18, 8),
            token_type: ForTokenType::Reversed,
        };
        let unexpected = ForLexerError::UnexpectedExpression { at: (27, 7).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap(), in_token);
        assert_eq!(lexer.next().unwrap().unwrap(), bar);
        assert_eq!(lexer.next().unwrap().unwrap(), reversed);
        assert_eq!(lexer.next().unwrap().unwrap_err(), unexpected);
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_incomplete_string() {
        let template = "{% for foo in 'bar %}";
        let parts = TagParts { at: (7, 11) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let incomplete = LexerError::IncompleteString { at: (14, 4).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap(), in_token);
        assert_eq!(lexer.next().unwrap().unwrap_err(), incomplete.into());
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_incomplete_translated_string() {
        let template = "{% for foo in _('bar' %}";
        let parts = TagParts { at: (7, 14) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let incomplete = LexerError::IncompleteTranslatedString { at: (14, 7).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap(), in_token);
        assert_eq!(lexer.next().unwrap().unwrap_err(), incomplete.into());
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_invalid_remainder() {
        let template = "{% for foo in 'bar'baz %}";
        let parts = TagParts { at: (7, 15) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let foo = ForToken {
            at: (7, 3),
            token_type: ForTokenType::VariableName,
        };
        let in_token = ForToken {
            at: (11, 2),
            token_type: ForTokenType::In,
        };
        let incomplete = LexerError::InvalidRemainder { at: (19, 3).into() };
        assert_eq!(lexer.next().unwrap().unwrap(), foo);
        assert_eq!(lexer.next().unwrap().unwrap(), in_token);
        assert_eq!(lexer.next().unwrap().unwrap_err(), incomplete.into());
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn test_invalid_name() {
        let template = "{% for '2' in 'bar' %}";
        let parts = TagParts { at: (7, 12) };
        let mut lexer = ForLexer::new(template.into(), parts);

        let invalid = ForLexerError::InvalidName {
            name: "'2'".to_string(),
            at: (7, 3).into(),
        };
        assert_eq!(lexer.next().unwrap().unwrap_err(), invalid);
        assert_eq!(lexer.next(), None);
    }
}
