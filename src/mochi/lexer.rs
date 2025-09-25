#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Keyword(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    EOF,
}

pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut buf = String::new();

    fn flush(buf: &mut String, tokens: &mut Vec<Token>) {
        if !buf.is_empty() {
            let word = buf.clone();
            buf.clear();
            match word.as_str() {
                "voxel" => tokens.push(Token::Keyword("voxel".to_string())),
                _ => tokens.push(Token::Ident(word)),
            }
        }
    }

    for ch in input.chars() {
        match ch {
            '{' => { flush(&mut buf, &mut tokens); tokens.push(Token::LBrace); }
            '}' => { flush(&mut buf, &mut tokens); tokens.push(Token::RBrace); }
            '[' => { flush(&mut buf, &mut tokens); tokens.push(Token::LBracket); }
            ']' => { flush(&mut buf, &mut tokens); tokens.push(Token::RBracket); }
            ':' => { flush(&mut buf, &mut tokens); tokens.push(Token::Colon); }
            '\n' | ' ' | '\t' => { flush(&mut buf, &mut tokens); }
            _ => buf.push(ch),
        }
    }

    flush(&mut buf, &mut tokens);
    tokens.push(Token::EOF);
    tokens
}
