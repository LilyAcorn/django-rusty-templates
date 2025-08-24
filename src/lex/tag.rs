use miette::{Diagnostic, SourceSpan};
use thiserror::Error;
use unicode_xid::UnicodeXID;

use crate::lex::common::NextChar;

#[derive(Error, Debug, Diagnostic, Eq, PartialEq)]
pub enum TagLexerError {
    #[error("Invalid block tag name")]
    InvalidTagName {
        #[label("here")]
        at: SourceSpan,
    },
}

#[derive(Debug, PartialEq)]
pub struct TagToken {
    pub at: (usize, usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagParts {
    pub at: (usize, usize),
}

pub fn lex_tag(tag: &str, start: usize) -> Result<Option<(TagToken, TagParts)>, TagLexerError> {
    let rest = tag.trim_start();
    if rest.trim().is_empty() {
        return Ok(None);
    }

    let start = start + tag.len() - rest.len();
    let tag = tag.trim();
    let tag_len = match tag.find(|c: char| !c.is_xid_continue()) {
        Some(tag_len) => tag_len,
        None => {
            let at = (start, tag.len());
            let token = TagToken { at };
            let parts = TagParts {
                at: (start + tag.len(), 0),
            };
            return Ok(Some((token, parts)));
        }
    };
    let index = tag.next_whitespace();
    if index > tag_len {
        let at = (start, index);
        return Err(TagLexerError::InvalidTagName { at: at.into() });
    }
    let at = (start, tag_len);
    let token = TagToken { at };
    let rest = &tag[tag_len..];
    let trimmed = rest.trim();
    let start = start + tag_len + rest.len() - trimmed.len();
    let at = (start, trimmed.len());
    let parts = TagParts { at };
    Ok(Some((token, parts)))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::lex::{END_TAG_LEN, START_TAG_LEN};
    use crate::types::TemplateString;

    fn trim_tag(template: &str) -> &str {
        &template[START_TAG_LEN..(template.len() - END_TAG_LEN)]
    }

    #[test]
    fn test_lex_empty() {
        let template = "{%  %}";
        let tag = trim_tag(template);
        assert!(lex_tag(tag, START_TAG_LEN).unwrap().is_none());
    }

    #[test]
    fn test_lex_tag() {
        let template = "{% csrftoken %}";
        let tag = trim_tag(template);
        let (token, rest) = lex_tag(tag, START_TAG_LEN).unwrap().unwrap();
        assert_eq!(token, TagToken { at: (3, 9) });
        assert_eq!(TemplateString(template).content(token.at), "csrftoken");
        assert_eq!(rest, TagParts { at: (12, 0) })
    }

    #[test]
    fn test_lex_invalid_tag() {
        let template = "{% url'foo' %}";
        let tag = trim_tag(template);
        let error = lex_tag(tag, START_TAG_LEN).unwrap_err();
        assert_eq!(error, TagLexerError::InvalidTagName { at: (3, 8).into() })
    }

    #[test]
    fn test_lex_invalid_tag_rest() {
        let template = "{% url'foo' bar %}";
        let tag = trim_tag(template);
        let error = lex_tag(tag, START_TAG_LEN).unwrap_err();
        assert_eq!(error, TagLexerError::InvalidTagName { at: (3, 8).into() })
    }

    #[test]
    fn test_lex_tag_rest() {
        let template = "{% url name arg %}";
        let tag = trim_tag(template);
        let (token, rest) = lex_tag(tag, START_TAG_LEN).unwrap().unwrap();
        assert_eq!(token, TagToken { at: (3, 3) });
        assert_eq!(TemplateString(template).content(token.at), "url");
        assert_eq!(rest, TagParts { at: (7, 8) })
    }
}
