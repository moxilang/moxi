use std::iter::Peekable;
use std::vec::IntoIter;

use super::super::lexer::Token;
use super::ast::AstNode;
use super::expr::parse_expression;

/// Parse `name = expr`
pub fn parse_assignment(iter: &mut Peekable<IntoIter<Token>>) -> Option<AstNode> {
    // variable name
    let name = if let Some(Token::Ident(var)) = iter.next() {
        var
    } else {
        return None;
    };

    // expect '='
    if let Some(Token::Equals) = iter.peek() {
        iter.next(); // consume '='
        let expr = parse_expression(iter);
        return Some(AstNode::Assignment {
            name,
            expr: Box::new(expr),
        });
    }

    None
}

/// Parse `print [target]`
pub fn parse_print(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    iter.next(); // consume "print"
    let mut target = None;

    if let Some(Token::Ident(id)) = iter.peek() {
        target = Some(id.clone());
        iter.next();
    }

    AstNode::Print { target }
}
