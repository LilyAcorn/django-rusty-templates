use miette::Diagnostic;
use thiserror::Error;
use unicode_xid::UnicodeXID;

use super::common::NextChar;
use super::tag::TagParts;
use crate::types::TemplateString;

#[derive(Debug)]
pub enum CustomTagToken {
    Arg {
        at: (usize, usize),
    },
    Kwarg {
        at: (usize, usize),
        name_at: (usize, usize),
    },
}

#[derive(Error, Debug, Diagnostic, PartialEq, Eq)]
pub enum CustomTagLexerError {}

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

    fn lex_kwarg(&mut self) -> Option<(usize, usize)> {
        let index = self.rest.find('=')?;
        match self.rest.find(|c: char| !c.is_xid_continue()) {
            Some(n) if n < index => return None,
            _ => {}
        }
        let at = (self.byte, index);
        self.rest = &self.rest[index + 1..];
        self.byte += index + 1;
        Some(at)
    }
}

impl Iterator for CustomTagLexer<'_> {
    type Item = Result<CustomTagToken, CustomTagLexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let kwarg = self.lex_kwarg();

        let start = self.byte;
        let len = self.rest.next_whitespace();
        let rest = &self.rest[len..];
        let next = rest.next_non_whitespace();
        self.rest = &rest[next..];
        self.byte = self.byte + len + next;

        let at = (start, len);
        Some(Ok(match kwarg {
            Some(kwarg) => CustomTagToken::Kwarg { at, name_at: kwarg },
            None => CustomTagToken::Arg { at },
        }))
    }
}
