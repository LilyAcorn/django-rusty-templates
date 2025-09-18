use crate::lex::core::{Lexer, TokenType};
use crate::lex::variable::lex_variable;
use crate::lex::START_TAG_LEN;
use crate::types::TemplateString;
use oxc::allocator::Allocator;
use oxc::ast::ast::{
    Argument, BinaryOperator, Expression, FormalParameterKind, FormalParameters, FunctionType,
    SourceType, Statement,
};
use oxc::ast::{AstBuilder, NONE};
use oxc::codegen::Codegen;
use oxc::span::SPAN;

pub trait SomeWrap<T> {
    fn wrap(self) -> Option<T>;
}

impl<T> SomeWrap<T> for T {
    fn wrap(self) -> Option<T> {
        Some(self)
    }
}

impl<T> SomeWrap<T> for Option<T> {
    fn wrap(self) -> Self {
        self
    }
}

trait SimplerAstMethods<'a> {
    /// Produces a simple `fn_name(arg1, arg2, arg3, ...)` call
    fn expression_call_simple<A>(&self, fn_name: &'a str, args: A) -> Expression<'a>
    where
        A: Into<Vec<&'a str>>;

    fn formal_parameters_simple<A>(&self, params: A) -> FormalParameters<'a>
    where
        A: Into<Vec<&'a str>>;
}
impl<'a> SimplerAstMethods<'a> for AstBuilder<'a> {
    fn expression_call_simple<ARG>(&self, fn_name: &'a str, args: ARG) -> Expression<'a>
    where
        ARG: Into<Vec<&'a str>>,
    {
        self.expression_call(
            SPAN,
            self.expression_identifier(SPAN, fn_name),
            NONE,
            self.vec_from_iter(
                args.into()
                    .iter()
                    .map(|arg| Argument::from(self.expression_identifier(SPAN, *arg))),
            ),
            false,
        )
    }

    fn formal_parameters_simple<A>(&self, params: A) -> FormalParameters<'a>
    where
        A: Into<Vec<&'a str>>,
    {
        self.formal_parameters(
            SPAN,
            FormalParameterKind::FormalParameter,
            self.vec_from_iter(params.into().iter().map(|param| {
                self.plain_formal_parameter(
                    SPAN,
                    self.binding_pattern(
                        self.binding_pattern_kind_binding_identifier(SPAN, *param),
                        NONE,
                        false,
                    ),
                )
            })),
            NONE,
        )
    }
}

pub struct Parser<'t> {
    template: TemplateString<'t>,
    lexer: Lexer<'t>,
}

impl<'t> Parser<'t> {
    pub fn new(template: TemplateString<'t>) -> Self {
        Self {
            template,
            lexer: Lexer::new(template),
        }
    }

    pub fn render(&mut self) -> String {
        let allocator = Allocator::default();
        let ast_builder = AstBuilder::new(&allocator);

        let mut expressions = Vec::<Expression>::new();
        while let Some(token) = self.lexer.next() {
            match token.token_type {
                TokenType::Text => expressions.push(ast_builder.expression_string_literal(
                    SPAN,
                    self.template.content(token.at),
                    None,
                )),
                TokenType::Comment => continue,
                TokenType::Variable => expressions.push(*self.parse_variable(
                    token.content(self.template),
                    token.at.0 + START_TAG_LEN,
                    &ast_builder,
                )),
                TokenType::Tag => continue,
            };
        }

        let result = expressions
            .into_iter()
            .reduce(|acc, expr| {
                ast_builder.expression_binary(SPAN, acc, BinaryOperator::Addition, expr)
            })
            .unwrap_or_else(|| ast_builder.expression_string_literal(SPAN, "", None));

        let program = ast_builder.program(
            SPAN,
            SourceType::mjs(),
            "",
            ast_builder.vec(),
            None,
            ast_builder.vec(),
            ast_builder.vec1(*self.get_export_fn(&ast_builder, result)),
        );

        Codegen::new().build(&program).code
    }

    fn parse_variable(
        &self,
        variable: &str,
        start: usize,
        ast_builder: &AstBuilder<'t>,
    ) -> Box<Expression<'t>> {
        if let Ok(Some((variable_token, _))) = lex_variable(variable, start) {
            ast_builder
                .expression_call_simple("String", [self.template.content(variable_token.at)])
                .into()
        } else {
            ast_builder.expression_string_literal(SPAN, "", None).into()
        }
    }

    fn get_export_fn(
        &self,
        ast_builder: &AstBuilder<'t>,
        result_to_return: Expression<'t>,
    ) -> Box<Statement<'t>> {
        let params = ast_builder.formal_parameters_simple(["engine", "context"]);
        let return_statement = ast_builder.statement_return(SPAN, result_to_return.wrap());
        let export = ast_builder.export_default_declaration_kind_function_declaration(
            SPAN,
            FunctionType::FunctionDeclaration,
            None,
            false,
            false,
            false,
            NONE,
            NONE,
            params,
            NONE,
            ast_builder
                .alloc_function_body(SPAN, ast_builder.vec(), ast_builder.vec1(return_statement))
                .wrap(),
        );
        Statement::from(ast_builder.module_declaration_export_default_declaration(SPAN, export))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use crate::js::Parser;
    use crate::types::TemplateString;

    fn assert_template(expected: &str, template: &str) {
        let rendered = Parser::new(TemplateString(template)).render();
        let actual = Regex::new(r"\s*\n\t?\s*").unwrap().replace_all(rendered.trim(), "");
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_empty() {
        assert_template(r#"export default function(engine, context) {return "";}"#, "");
    }

    #[test]
    fn test_simple_text() {
        assert_template(
            r#"export default function(engine, context) {return "<div>Some text here</div>";}"#,
            r#"<div>Some text here</div>"#,
        );
    }

    #[test]
    fn test_simple_variable() {
        assert_template(
            r#"export default function(engine, context) {return "<div class=\"" + String(test_class) + "\">Some text here</div>";}"#,
            r#"<div class="{{ test_class }}">Some text here</div>"#,
        );
    }
}
