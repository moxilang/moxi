#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Keyword(String),
    StringLit(String),
    NumberLit(i32),

    LBrace, RBrace,
    LBracket, RBracket,
    LParen, RParen,

    Comma,
    Colon,
    Equals,

    EOF,
}


pub fn lex(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut buf = String::new();

    fn flush(buf: &mut String, tokens: &mut Vec<Token>) {
        if !buf.is_empty() {
            let word = buf.clone();
            buf.clear();
            if let Ok(num) = word.parse::<i32>() {
                tokens.push(Token::NumberLit(num));
            } else {
                match word.as_str() {
                    "voxel" | "atom" | "for" | "in" | "print" => tokens.push(Token::Keyword(word)),
                    _ => tokens.push(Token::Ident(word)),
                }
            }
        }
    }

    let mut in_string = false;
    let mut string_delim: Option<char> = None;
    let mut in_comment = false;

    for ch in input.chars() {
        if in_comment {
            if ch == '\n' {
                in_comment = false;
            }
            continue;
        }

        match ch {
            // start comment
            '#' if !in_string => {
                flush(&mut buf, &mut tokens);
                in_comment = true;
            }

            // string delimiters
            '"' | '\'' => {
                if in_string {
                    if Some(ch) == string_delim {
                        tokens.push(Token::StringLit(buf.clone()));
                        buf.clear();
                        in_string = false;
                        string_delim = None;
                    } else {
                        buf.push(ch);
                    }
                } else {
                    in_string = true;
                    string_delim = Some(ch);
                }
            }

            _ if in_string => buf.push(ch),

            '{' => { flush(&mut buf, &mut tokens); tokens.push(Token::LBrace); }
            '}' => { flush(&mut buf, &mut tokens); tokens.push(Token::RBrace); }
            '[' => { flush(&mut buf, &mut tokens); tokens.push(Token::LBracket); }
            ']' => { flush(&mut buf, &mut tokens); tokens.push(Token::RBracket); }
            '(' => { flush(&mut buf, &mut tokens); tokens.push(Token::LParen); }
            ')' => { flush(&mut buf, &mut tokens); tokens.push(Token::RParen); }
            ',' => { flush(&mut buf, &mut tokens); tokens.push(Token::Comma); }
            ':' => { flush(&mut buf, &mut tokens); tokens.push(Token::Colon); }
            '=' => { flush(&mut buf, &mut tokens); tokens.push(Token::Equals); }
            '\n' | ' ' | '\t' => { flush(&mut buf, &mut tokens); }
            _ => buf.push(ch),
        }
    }

    flush(&mut buf, &mut tokens);
    tokens.push(Token::EOF);
    tokens
}
