use num_bigint::BigInt;

#[derive(Clone, Copy)]
pub struct TemplateString<'t>(pub &'t str);

impl<'t> TemplateString<'t> {
    pub fn content(&self, at: (usize, usize)) -> &'t str {
        let (start, len) = at;
        &self.0[start..start + len]
    }
}

impl<'t> From<&'t str> for TemplateString<'t> {
    fn from(value: &'t str) -> Self {
        TemplateString(value)
    }
}

struct PartsIterator<'t> {
    variable: &'t str,
    start: usize,
}

impl<'t> Iterator for PartsIterator<'t> {
    type Item = (&'t str, (usize, usize));

    fn next(&mut self) -> Option<Self::Item> {
        if self.variable.is_empty() {
            return None;
        }

        match self.variable.find('.') {
            Some(index) => {
                let part = &self.variable[..index];
                let at = (self.start, index);
                self.start += index + 1;
                self.variable = &self.variable[index + 1..];
                Some((part, at))
            }
            None => {
                let part = self.variable;
                self.variable = "";
                Some((part, (self.start, part.len())))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Text {
    pub at: (usize, usize),
}

impl Text {
    pub fn new(at: (usize, usize)) -> Self {
        Self { at }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranslatedText {
    pub at: (usize, usize),
}

impl TranslatedText {
    pub fn new(at: (usize, usize)) -> Self {
        Self { at }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variable {
    pub at: (usize, usize),
}

impl<'t> Variable {
    pub fn new(at: (usize, usize)) -> Self {
        Self { at }
    }

    pub fn parts(
        &self,
        template: TemplateString<'t>,
    ) -> impl Iterator<Item = (&'t str, (usize, usize))> {
        let start = self.at.0;
        let variable = template.content(self.at);
        PartsIterator { variable, start }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArgumentType {
    Variable(Variable),
    Text(Text),
    TranslatedText(TranslatedText),
    Int(BigInt),
    Float(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub at: (usize, usize),
    pub argument_type: ArgumentType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ForVariableName {
    Counter,
    Counter0,
    RevCounter,
    RevCounter0,
    First,
    Last,
    Object,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForVariable {
    pub variant: ForVariableName,
    pub parent_count: usize,
}
