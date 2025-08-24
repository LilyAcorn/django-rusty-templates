use miette::Diagnostic;
use thiserror::Error;

use crate::types::TemplateString;
use super::common::NextChar;
use super::tag::TagParts;


#[derive(Debug)]
pub enum CustomTagToken {
    Arg {at: (usize, usize)},
}

#[derive(Error, Debug, Diagnostic, PartialEq, Eq)]
pub enum CustomTagLexerError {
}


pub struct CustomTagLexer<'t> {
    rest: &'t str,
    byte: usize,
    takes_context: bool,
}

impl<'t> CustomTagLexer<'t> {
    pub fn new(template: TemplateString<'t>, parts: TagParts, takes_context: bool) -> Self {
        Self {
            rest: template.content(parts.at),
            byte: parts.at.0,
            takes_context,
        }
    }
}

impl Iterator for CustomTagLexer<'_> {
    type Item = Result<CustomTagToken, CustomTagLexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let start = self.byte;
        let len = self.rest.next_whitespace();
        let rest = &self.rest[len..];
        let next = rest.next_non_whitespace();
        self.rest = &rest[next..];
        self.byte = self.byte + len + next;

        let at = (start, len);
        Some(Ok(CustomTagToken::Arg { at }))
    }
}
