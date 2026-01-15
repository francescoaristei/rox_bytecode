
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum TokenType {
    // Single character token
    LeftParen, RightParen, LeftBrace, RightBrace,
    Comma, Dot, Minus, Plus, Semicolon, Slash, Star,

    // One or two characters token
    Bang, BangEqual, Equal, EqualEqual,
    Greater, GreaterEqual, Less, LessEqual,

    // Literals
    Identifier, String, Number,

    // Keywords
    And, Class, Else, False, Fun, For,
    If, Nil, Or, Print, Return, Super,
    This, True, Var, While,

    Error, Eof
}

/*
Clox identifies a lexeme by using: one pointer to the starting char and an int for the lexeme length.
This way different tokens can point to the same string.
Rust can achieve this by having a reference to the lexeme in each Token struct.
*/
#[derive(Copy, Clone)]
pub struct Token<'a> {
    token_type: TokenType,
    lexeme: &'a str,
    line: usize
}

impl<'a> Token<'a> {

    pub fn new(lexeme: &'a str) -> Token<'a> {
        Token {
            token_type: TokenType::Error,
            lexeme,
            line: 0,
        }
    }

    pub fn get_line(&self) -> usize {
        self.line
    }

    pub fn get_token_type(&self) -> TokenType {
        self.token_type
    }

    pub fn get_lexeme(&self) -> &'a str {
        self.lexeme
    }
}

// The Scanner does not read the whole source code all together giving out all the tokens, but
// only reads 1-2 tokens at a time, as the grammar does not require more to properly parse the user program (1 token lookahead).
pub struct Scanner<'a> {
    source: &'a str,
    start: usize,
    current: usize,
    line: usize
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        Scanner {
            source,
            start: 0,
            current: 0,
            line: 1
        }
    }

    // Why in the return Token the lifetime parameter 'a is needed?
    // Otherwise, Rust believes the lifetime parameter assigned to the Token struct is
    // the one of the Scanner (&mut self) and not the one of the source string slice.
    pub fn scan_token(&mut self) -> Token<'a> {
        self.skip_whitespace();
        self.start = self.current;
        if self.is_at_end() {
            return self.make_token(TokenType::Eof);
        }

        let character = self.advance();

        if character.is_ascii_alphabetic() || character == b'_' {
            return self.identifier();
        }
        if character.is_ascii_digit() {
            return self.number();
        }

        match character {
            b'(' => self.make_token(TokenType::LeftParen),
            b')' => self.make_token(TokenType::RightParen),
            b'{' => self.make_token(TokenType::LeftBrace),
            b'}' => self.make_token(TokenType::RightBrace),
            b';' => self.make_token(TokenType::Semicolon),
            b',' => self.make_token(TokenType::Comma),
            b'.' => self.make_token(TokenType::Dot),
            b'-' => self.make_token(TokenType::Minus),
            b'+' => self.make_token(TokenType::Plus),
            b'/' => self.make_token(TokenType::Slash),
            b'*' => self.make_token(TokenType::Star),
            b'!' => self.make_composite_token(b'=', TokenType::BangEqual, TokenType::Bang),
            b'=' => self.make_composite_token(b'=', TokenType::EqualEqual, TokenType::Equal),
            b'<' => self.make_composite_token(b'=', TokenType::LessEqual, TokenType::Less),
            b'>' => self.make_composite_token(b'=', TokenType::GreaterEqual, TokenType::Greater),
            b'"' => self.string(),
            _ => self.error_token("Unexpected character.")
        }
    }

    fn make_token(&mut self, token_type: TokenType) -> Token<'a> {
        Token {
            token_type,
            lexeme: &self.source[self.start..self.current],
            line: self.line
        }
    }

    fn make_composite_token(&mut self, expected: u8, composite_token_type: TokenType, simple_token_type: TokenType) -> Token<'a> {
        let token_type: TokenType;
        if self.match_token(expected) {
            token_type = composite_token_type;
        } else {
            token_type = simple_token_type;
        }
        self.make_token(token_type)
    }

    fn error_token<'b>(&mut self, msg: &'b str) -> Token<'b> {
        Token {
            token_type: TokenType::Error,
            lexeme: msg,
            line: self.line
        }
    }

    fn identifier(&mut self) -> Token<'a> {
        while self.peek().is_ascii_alphanumeric() || self.peek() == b'_' {
            self.advance();
        }
        let identifier_token_type = self.identifier_type();
        self.make_token(identifier_token_type)
    }

    /*
    The way to tell if a certain word belongs to the keywords reserved by Lox is with the help of a Trie data structure.
    */
    fn identifier_type(&mut self) -> TokenType {
        match self.source.as_bytes()[self.start] {
            b'a' => self.check_keyword(1, 2, "nd", TokenType::And),
            b'c' => self.check_keyword(1, 4, "lass", TokenType::Class),
            b'e' => self.check_keyword(1, 3, "lse", TokenType::Else),
            b'f' => {
                if self.current - self.start > 1 {
                    return match self.source.as_bytes()[self.start + 1] {
                        b'a' => self.check_keyword(2, 3, "lse", TokenType::False),
                        b'o' => self.check_keyword(2, 1, "r", TokenType::For),
                        b'u' => self.check_keyword(2, 1, "n", TokenType::Fun),
                        _ => TokenType::Identifier
                    };
                }
                TokenType::Identifier
            },
            b'i' => self.check_keyword(1, 1, "f", TokenType::If),
            b'n' => self.check_keyword(1, 2, "il", TokenType::Nil),
            b'o' => self.check_keyword(1, 1, "r", TokenType::Or),
            b'p' => self.check_keyword(1, 4, "rint", TokenType::Print),
            b'r' => self.check_keyword(1, 5, "eturn", TokenType::Return),
            b's' => self.check_keyword(1, 4, "uper", TokenType::Super),
            b't' => {
                if self.current - self.start > 1 {
                    return match self.source.as_bytes()[self.start + 1] {
                        b'h' => self.check_keyword(2, 2, "is", TokenType::This),
                        b'r' => self.check_keyword(2, 2, "ue", TokenType::True),
                        _ => TokenType::Identifier
                    };
                }
                TokenType::Identifier
            },
            b'v' => self.check_keyword(1, 2, "ar", TokenType::Var),
            b'w' => self.check_keyword(1, 4, "hile", TokenType::While),
            _ => TokenType::Identifier
        }
    }

    fn check_keyword(&mut self, start: usize, length: usize, remaining: &str, token_type: TokenType) -> TokenType {
        if self.current - self.start == start + length && self.source[self.start + start..self.start + start + length] == *remaining {
            token_type
        } else {
            TokenType::Identifier
        }
    }

    fn number(&mut self) -> Token<'a> {
        while self.peek().is_ascii_digit() && !self.is_at_end() {
            self.advance();
        }

        if self.peek() == b'.' && self.peek_next().is_ascii_digit() {
            self.advance();
            while self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        self.make_token(TokenType::Number)
    }

    fn string(&mut self) -> Token<'a> {
        while self.peek() != b'"' && !self.is_at_end() {
            if self.peek() == b'\n' {
                self.line += 1;
            }
            self.advance();
        }

        if self.is_at_end() {
            return self.error_token("Unterminated string.");
        }

        self.advance();
        // differently from the Tree-Walk Interpreter, the Token does not contain the actual runtime
        // value that the lexeme represent. This duty is deferred at compile time where the runtime value
        // is stored in the constant table of the compiled chunk.
        self.make_token(TokenType::String)
    }

    fn skip_whitespace(&mut self) {
        loop {
            let character = self.peek();
            match character {
                b' ' | b'\t' | b'\r' => {self.advance();},
                b'\n' => {
                    self.line += 1;
                    self.advance();
                },
                b'/' => {
                    if self.peek_next() == b'/' {
                        while self.peek() != b'\n' && !self.is_at_end() {
                            self.advance();
                        }
                    } else {
                        return
                    }
                }
                _ => return
            }
        }
    }

    fn peek(&self) -> u8 {
        if self.is_at_end() {
            return b'\0';
        }
        self.source.as_bytes()[self.current]
    }

    fn peek_next(&self) -> u8 {
        if self.is_at_end() {
            return b'\0';
        }
        self.source.as_bytes()[self.current + 1]
    }

    fn match_token(&mut self, expected: u8) -> bool {
        if self.is_at_end() {
            return false;
        }
        if self.source.as_bytes()[self.current] != expected {
            return false;
        }
        self.current += 1;
        true
    }

    fn is_at_end(&self) -> bool {
        self.current == self.source.len()
    }

    fn advance(&mut self) -> u8 {
        let character = self.source.as_bytes()[self.current];
        self.current += 1;
        character
    }
}