use crate::lex::common::{
    LexerError, NextChar, lex_numeric, lex_text, lex_translated, lex_variable, text_content_at,
    translated_text_content_at,
};
use crate::lex::tag::TagParts;
use crate::types::TemplateString;

#[derive(Debug, PartialEq, Eq)]
pub enum IfConditionAtom {
    Numeric,
    Text,
    TranslatedText,
    Variable,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum IfConditionOperator {
    And,
    Or,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanEqual,
    GreaterThanEqual,
    In,
    NotIn,
    Is,
    IsNot,
}

#[derive(Debug, PartialEq, Eq)]
pub enum IfConditionTokenType {
    Atom(IfConditionAtom),
    Operator(IfConditionOperator),
    Not,
}

#[derive(Debug, PartialEq, Eq)]
pub struct IfConditionToken {
    pub at: (usize, usize),
    pub token_type: IfConditionTokenType,
}

impl IfConditionToken {
    pub fn content_at(&self) -> (usize, usize) {
        match self.token_type {
            IfConditionTokenType::Atom(IfConditionAtom::Text) => text_content_at(self.at),
            IfConditionTokenType::Atom(IfConditionAtom::TranslatedText) => {
                translated_text_content_at(self.at)
            }
            _ => self.at,
        }
    }
}

pub struct IfConditionLexer<'t> {
    rest: &'t str,
    byte: usize,
}

impl<'t> IfConditionLexer<'t> {
    pub fn new(template: TemplateString<'t>, parts: TagParts) -> Self {
        Self {
            rest: template.content(parts.at),
            byte: parts.at.0,
        }
    }

    fn lex_condition(&mut self) -> Result<IfConditionToken, LexerError> {
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

    fn lex_variable(&mut self) -> IfConditionToken {
        let (at, byte, rest) = lex_variable(self.byte, self.rest);
        self.rest = rest;
        self.byte = byte;
        IfConditionToken {
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
            at,
        }
    }

    fn lex_numeric(&mut self) -> IfConditionToken {
        let (at, byte, rest) = lex_numeric(self.byte, self.rest);
        self.rest = rest;
        self.byte = byte;
        IfConditionToken {
            at,
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Numeric),
        }
    }

    fn lex_text(
        &mut self,
        chars: &mut std::str::Chars,
        end: char,
    ) -> Result<IfConditionToken, LexerError> {
        match lex_text(self.byte, self.rest, chars, end) {
            Ok((at, byte, rest)) => {
                self.rest = rest;
                self.byte = byte;
                Ok(IfConditionToken {
                    token_type: IfConditionTokenType::Atom(IfConditionAtom::Text),
                    at,
                })
            }
            Err(e) => {
                self.rest = "";
                Err(e)
            }
        }
    }

    fn lex_translated(
        &mut self,
        chars: &mut std::str::Chars,
    ) -> Result<IfConditionToken, LexerError> {
        match lex_translated(self.byte, self.rest, chars) {
            Ok((at, byte, rest)) => {
                self.rest = rest;
                self.byte = byte;
                Ok(IfConditionToken {
                    token_type: IfConditionTokenType::Atom(IfConditionAtom::TranslatedText),
                    at,
                })
            }
            Err(e) => {
                self.rest = "";
                Err(e)
            }
        }
    }

    fn lex_remainder(&mut self) -> Result<(), LexerError> {
        match self.rest.next_whitespace() {
            0 => {
                let rest = self.rest.trim_start();
                self.byte += self.rest.len() - rest.len();
                self.rest = rest;
                Ok(())
            }
            n => {
                self.rest = "";
                let at = (self.byte, n).into();
                Err(LexerError::InvalidRemainder { at })
            }
        }
    }
}

impl Iterator for IfConditionLexer<'_> {
    type Item = Result<IfConditionToken, LexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let index = self.rest.next_whitespace();
        let (token_type, index) = match &self.rest[..index] {
            "and" => (
                IfConditionTokenType::Operator(IfConditionOperator::And),
                index,
            ),
            "or" => (
                IfConditionTokenType::Operator(IfConditionOperator::Or),
                index,
            ),
            "not" => {
                let rest = &self.rest[index..];
                let whitespace_index = rest.next_non_whitespace();
                let rest = &rest[whitespace_index..];
                let next_index = rest.next_whitespace();
                match &rest[..next_index] {
                    "in" => (
                        IfConditionTokenType::Operator(IfConditionOperator::NotIn),
                        index + whitespace_index + next_index,
                    ),
                    _ => (IfConditionTokenType::Not, index),
                }
            }
            "==" => (
                IfConditionTokenType::Operator(IfConditionOperator::Equal),
                index,
            ),
            "!=" => (
                IfConditionTokenType::Operator(IfConditionOperator::NotEqual),
                index,
            ),
            "<" => (
                IfConditionTokenType::Operator(IfConditionOperator::LessThan),
                index,
            ),
            ">" => (
                IfConditionTokenType::Operator(IfConditionOperator::GreaterThan),
                index,
            ),
            "<=" => (
                IfConditionTokenType::Operator(IfConditionOperator::LessThanEqual),
                index,
            ),
            ">=" => (
                IfConditionTokenType::Operator(IfConditionOperator::GreaterThanEqual),
                index,
            ),
            "in" => (
                IfConditionTokenType::Operator(IfConditionOperator::In),
                index,
            ),
            "is" => {
                let rest = &self.rest[index..];
                let whitespace_index = rest.next_non_whitespace();
                let rest = &rest[whitespace_index..];
                let next_index = rest.next_whitespace();
                match &rest[..next_index] {
                    "not" => (
                        IfConditionTokenType::Operator(IfConditionOperator::IsNot),
                        index + whitespace_index + next_index,
                    ),
                    _ => (
                        IfConditionTokenType::Operator(IfConditionOperator::Is),
                        index,
                    ),
                }
            }
            _ => return Some(self.lex_condition()),
        };
        let at = (self.byte, index);

        let rest = &self.rest[index..];
        let next_index = rest.next_non_whitespace();
        self.byte += index + next_index;
        self.rest = &rest[next_index..];

        Some(Ok(IfConditionToken { at, token_type }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_variable() {
        let template = "{% if foo %}";
        let parts = TagParts { at: (6, 3) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = IfConditionToken {
            at: (6, 3),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        assert_eq!(tokens, vec![Ok(foo)]);
    }

    #[test]
    fn test_lex_variable_leading_underscorej() {
        let template = "{% if _foo %}";
        let parts = TagParts { at: (6, 4) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foo = IfConditionToken {
            at: (6, 4),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        assert_eq!(tokens, vec![Ok(foo)]);
    }

    #[test]
    fn test_lex_numeric() {
        let template = "{% if 5.3 %}";
        let parts = TagParts { at: (6, 3) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let numeric = IfConditionToken {
            at: (6, 3),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Numeric),
        };
        assert_eq!(tokens, vec![Ok(numeric)]);
    }

    #[test]
    fn test_lex_text() {
        let template = "{% if 'foo' %}";
        let parts = TagParts { at: (6, 5) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let text = IfConditionToken {
            at: (6, 5),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Text),
        };
        assert_eq!(tokens, vec![Ok(text)]);
    }

    #[test]
    fn test_lex_text_double_quotes() {
        let template = "{% if \"foo\" %}";
        let parts = TagParts { at: (6, 5) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let text = IfConditionToken {
            at: (6, 5),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Text),
        };
        assert_eq!(tokens, vec![Ok(text)]);
    }

    #[test]
    fn test_lex_translated() {
        let template = "{% if _('foo') %}";
        let parts = TagParts { at: (6, 8) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let text = IfConditionToken {
            at: (6, 8),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::TranslatedText),
        };
        assert_eq!(tokens, vec![Ok(text)]);
    }

    #[test]
    fn test_lex_translated_error() {
        let template = "{% if _('foo' %}";
        let parts = TagParts { at: (6, 7) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let error = LexerError::IncompleteTranslatedString { at: (6, 7).into() };
        assert_eq!(tokens, vec![Err(error)]);
    }

    #[test]
    fn test_lex_and() {
        let template = "{% if and %}";
        let parts = TagParts { at: (6, 3) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let and = IfConditionToken {
            at: (6, 3),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::And),
        };
        assert_eq!(tokens, vec![Ok(and)]);
    }

    #[test]
    fn test_lex_or() {
        let template = "{% if or %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let or = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::Or),
        };
        assert_eq!(tokens, vec![Ok(or)]);
    }

    #[test]
    fn test_lex_not() {
        let template = "{% if not %}";
        let parts = TagParts { at: (6, 3) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let not = IfConditionToken {
            at: (6, 3),
            token_type: IfConditionTokenType::Not,
        };
        assert_eq!(tokens, vec![Ok(not)]);
    }

    #[test]
    fn test_lex_equal() {
        let template = "{% if == %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let equal = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::Equal),
        };
        assert_eq!(tokens, vec![Ok(equal)]);
    }

    #[test]
    fn test_lex_not_equal() {
        let template = "{% if != %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let not_equal = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::NotEqual),
        };
        assert_eq!(tokens, vec![Ok(not_equal)]);
    }

    #[test]
    fn test_lex_less_than() {
        let template = "{% if < %}";
        let parts = TagParts { at: (6, 1) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let less_than = IfConditionToken {
            at: (6, 1),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::LessThan),
        };
        assert_eq!(tokens, vec![Ok(less_than)]);
    }

    #[test]
    fn test_lex_greater_than() {
        let template = "{% if > %}";
        let parts = TagParts { at: (6, 1) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let greater_than = IfConditionToken {
            at: (6, 1),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::GreaterThan),
        };
        assert_eq!(tokens, vec![Ok(greater_than)]);
    }

    #[test]
    fn test_lex_less_equal() {
        let template = "{% if <= %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let less_equal = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::LessThanEqual),
        };
        assert_eq!(tokens, vec![Ok(less_equal)]);
    }

    #[test]
    fn test_lex_greater_equal() {
        let template = "{% if >= %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let greater_equal = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::GreaterThanEqual),
        };
        assert_eq!(tokens, vec![Ok(greater_equal)]);
    }

    #[test]
    fn test_lex_in() {
        let template = "{% if in %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let in_ = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::In),
        };
        assert_eq!(tokens, vec![Ok(in_)]);
    }

    #[test]
    fn test_lex_not_in() {
        let template = "{% if not in %}";
        let parts = TagParts { at: (6, 6) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let not_in = IfConditionToken {
            at: (6, 6),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::NotIn),
        };
        assert_eq!(tokens, vec![Ok(not_in)]);
    }

    #[test]
    fn test_lex_is() {
        let template = "{% if is %}";
        let parts = TagParts { at: (6, 2) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let is = IfConditionToken {
            at: (6, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::Is),
        };
        assert_eq!(tokens, vec![Ok(is)]);
    }

    #[test]
    fn test_lex_is_not() {
        let template = "{% if is not %}";
        let parts = TagParts { at: (6, 6) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let is_not = IfConditionToken {
            at: (6, 6),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::IsNot),
        };
        assert_eq!(tokens, vec![Ok(is_not)]);
    }

    #[test]
    fn test_lex_complex_condition() {
        let template = "{% if foo.bar|default:'spam' and count >= 1.5 or enabled is not False %}";
        let parts = TagParts { at: (6, 63) };
        let lexer = IfConditionLexer::new(template.into(), parts);
        let tokens: Vec<_> = lexer.collect();

        let foobar = IfConditionToken {
            at: (6, 22),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        let and = IfConditionToken {
            at: (29, 3),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::And),
        };
        let count = IfConditionToken {
            at: (33, 5),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        let greater_equal = IfConditionToken {
            at: (39, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::GreaterThanEqual),
        };
        let numeric = IfConditionToken {
            at: (42, 3),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Numeric),
        };
        let or = IfConditionToken {
            at: (46, 2),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::Or),
        };
        let enabled = IfConditionToken {
            at: (49, 7),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        let is_not = IfConditionToken {
            at: (57, 6),
            token_type: IfConditionTokenType::Operator(IfConditionOperator::IsNot),
        };
        let falsey = IfConditionToken {
            at: (64, 5),
            token_type: IfConditionTokenType::Atom(IfConditionAtom::Variable),
        };
        let condition = vec![
            Ok(foobar),
            Ok(and),
            Ok(count),
            Ok(greater_equal),
            Ok(numeric),
            Ok(or),
            Ok(enabled),
            Ok(is_not),
            Ok(falsey),
        ];
        assert_eq!(tokens, condition);
    }

    #[test]
    fn test_lex_invalid_remainder() {
        let template = "{% if 'foo'remainder %}";
        let parts = TagParts { at: (6, 14) };
        let mut lexer = IfConditionLexer::new(template.into(), parts);
        let error = lexer.next().unwrap().unwrap_err();
        assert_eq!(error, LexerError::InvalidRemainder { at: (11, 9).into() });
    }
}
