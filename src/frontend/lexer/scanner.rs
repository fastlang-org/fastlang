use crate::frontend::lexer::token::{Token, TokenKind};

pub struct Scanner {
    source: Vec<char>,
    position: usize,
    line: usize,
    column: usize,
    in_template: bool,
    template_stack: Vec<usize>,
    code_brace_stack: Vec<usize>,
    at_f_pending: bool,
}

impl Scanner {
    pub fn scan_all(source: &str) -> Vec<Token> {
        let mut scanner = Scanner::new(source.to_string());
        let mut tokens: Vec<Token> = Vec::new();
        loop {
            let tok = scanner.next_token();
            let is_eof = tok.kind == TokenKind::EOF;
            match tok.kind {
                TokenKind::InlineComment | TokenKind::MultiLineComment => {}
                _ => tokens.push(tok),
            }
            if is_eof {
                break;
            }
        }
        tokens
    }

    pub fn new(source: String) -> Self {
        let clean_source = source.strip_prefix('\u{feff}').unwrap_or(&source);
        Scanner {
            source: clean_source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
            in_template: false,
            template_stack: Vec::new(),
            code_brace_stack: Vec::new(),
            at_f_pending: false,
        }
    }

    /// Look at the character `offset` positions ahead without consuming anything.
    /// Bounds-checked: returns None past the end, for any offset.
    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source.get(self.position + offset).copied()
    }

    fn peek(&self) -> Option<char> {
        self.peek_at(0)
    }

    fn advance(&mut self) -> Option<char> {
        if self.is_at_end() {
            None
        } else {
            let c = self.source[self.position];
            self.position += 1;
            self.column += 1;
            Some(c)
        }
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.source.len()
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\r' | '\t' => {
                    self.advance();
                }
                '\n' => {
                    self.line += 1;
                    self.column = 1;
                    self.advance();
                }
                _ => {
                    break;
                }
            }
        }
    }

    fn check_keyword(word: &str) -> Option<TokenKind> {
        match word {
            // Keywords
            //"let" => Some(TokenKind::Let),
            "const" => Some(TokenKind::Const),
            "set" => Some(TokenKind::Set),
            "del" => Some(TokenKind::Del),
            "as" => Some(TokenKind::As),
            "define" => Some(TokenKind::Define),
            // func
            "fn" => Some(TokenKind::Fn),
            "return" => Some(TokenKind::Return),
            // if
            "if" => Some(TokenKind::If),
            "else" => Some(TokenKind::Else),
            "for" => Some(TokenKind::For),
            "in" => Some(TokenKind::In),
            "match" => Some(TokenKind::Match),

            // loops
            "loop" => Some(TokenKind::Loop),
            "while" => Some(TokenKind::While),
            "do" => Some(TokenKind::Do),
            "continue" => Some(TokenKind::Continue),
            "break" => Some(TokenKind::Break),

            "new" => Some(TokenKind::New),

            "class" => Some(TokenKind::TypeClass),
            "struct" => Some(TokenKind::TypeStruct),
            "enum" => Some(TokenKind::TypeEnum),
            "method" => Some(TokenKind::TypeMethod),
            "Fn" => Some(TokenKind::TypeFn),

            "extends" => Some(TokenKind::Extends),
            "super" => Some(TokenKind::Super),
            "label" => Some(TokenKind::Label),
            "goto" => Some(TokenKind::Goto),
            "yield" => Some(TokenKind::Yield),
            "leave" => Some(TokenKind::Leave),

            // Primitives
            "char" => Some(TokenKind::TypeChar),
            "uchar" => Some(TokenKind::TypeUChar),
            "str" => Some(TokenKind::TypeStr),
            "ustr" => Some(TokenKind::TypeUStr),
            "int" => Some(TokenKind::TypeInt(32)),
            "int8" => Some(TokenKind::TypeInt(8)),
            "int16" => Some(TokenKind::TypeInt(16)),
            "int32" => Some(TokenKind::TypeInt(32)),
            "int64" => Some(TokenKind::TypeInt(64)),
            "int128" => Some(TokenKind::TypeInt(128)),
            "uint" => Some(TokenKind::TypeUInt(32)),
            "uint8" => Some(TokenKind::TypeUInt(8)),
            "uint16" => Some(TokenKind::TypeUInt(16)),
            "uint32" => Some(TokenKind::TypeUInt(32)),
            "uint64" => Some(TokenKind::TypeUInt(64)),
            "uint128" => Some(TokenKind::TypeUInt(128)),
            "byte" => Some(TokenKind::TypeUInt(8)),
            "usize" => Some(TokenKind::TypeUSize),
            "isize" => Some(TokenKind::TypeISize),
            "float" => Some(TokenKind::TypeFloat(32)),
            "float32" => Some(TokenKind::TypeFloat(32)),
            "float64" => Some(TokenKind::TypeFloat(64)),
            "float128" => Some(TokenKind::TypeFloat(128)),
            "bool" => Some(TokenKind::TypeBool),
            "using" => Some(TokenKind::Using),

            "init" => Some(TokenKind::Init),
            "blueprint" => Some(TokenKind::TypeBluePrint),
            "impl" => Some(TokenKind::Impl),
            "flag" => Some(TokenKind::Flag),

            "type" => Some(TokenKind::TypeType),
            "handle" => Some(TokenKind::Handle),
            "share" => Some(TokenKind::Share),
            "public" => Some(TokenKind::Public),
            "private" => Some(TokenKind::Private),

            "static" => Some(TokenKind::Static),
            "abstract" => Some(TokenKind::Abstract),
            "virtual" => Some(TokenKind::Virtual),
            "machine" => Some(TokenKind::TypeMachine),

            // memory / instances
            "this" => Some(TokenKind::This),
            "global" => Some(TokenKind::Global),

            // logical
            "and" => Some(TokenKind::And),
            "or" => Some(TokenKind::Or),

            // boolean literals
            "true" => Some(TokenKind::Bool(true)),
            "false" => Some(TokenKind::Bool(false)),

            // scope impl / unrestricted type
            "statement" => Some(TokenKind::Statement),

            "constructor" => Some(TokenKind::Constructor),
            // context / magic types
            "void" => Some(TokenKind::TypeVoid),
            "object" => Some(TokenKind::TypeObject),
            "block" => Some(TokenKind::TypeBlock),
            "macro" => Some(TokenKind::TypeMacro),
            "lambda" => Some(TokenKind::TypeLambda),

            "try" => Some(TokenKind::Try),
            "catch" => Some(TokenKind::Catch),
            "throw" => Some(TokenKind::Throw),
            "import" => Some(TokenKind::Import),
            "extern" => Some(TokenKind::Extern),
            "undefined" => Some(TokenKind::Undefined),
            "unknown" => Some(TokenKind::TypeUnknown),
            "auto" => Some(TokenKind::TypeAuto),
            "function" => Some(TokenKind::TypeFunction),
            "number" => Some(TokenKind::TypeNumber),

            "_" => Some(TokenKind::Underscore),
            _ => None,
        }
    }

    fn scan_string_literal(&mut self, start_line: usize, is_unicode: bool) -> TokenKind {
        let mut s = String::new();
        let mut terminated = false;
        let mut non_ascii_ch: Option<char> = None;
        while let Some(next_c) = self.peek() {
            if next_c == '"' {
                self.advance();
                terminated = true;
                break;
            } else if next_c == '\n' {
                break;
            } else if next_c == '\\' {
                self.advance(); // consume '\'
                if let Some(esc) = self.advance() {
                    match esc {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '\'' => s.push('\''),
                        '0' => s.push('\0'),
                        'a' => s.push('\x07'),
                        'b' => s.push('\x08'),
                        'f' => s.push('\x0C'),
                        'v' => s.push('\x0B'),
                        other => {
                            s.push('\\');
                            s.push(other);
                        }
                    }
                }
            } else {
                let ch = self.advance().unwrap();
                if !is_unicode && !ch.is_ascii() && non_ascii_ch.is_none() {
                    non_ascii_ch = Some(ch);
                }
                s.push(ch);
            }
        }
        if !is_unicode {
            if let Some(ch) = non_ascii_ch {
                return TokenKind::Error(format!(
                    "String literal at line {} contains non-ASCII character '{}'; prefix with 'u' or 'U' for a uchar string (e.g. u\"...\")",
                    start_line, ch
                ));
            }
        }
        if terminated {
            if is_unicode {
                TokenKind::UString(s)
            } else {
                TokenKind::String(s)
            }
        } else {
            TokenKind::Error(format!(
                "Unterminated string literal starting at line {}",
                start_line
            ))
        }
    }

    fn scan_char_literal(&mut self, start_line: usize, is_unicode: bool) -> TokenKind {
        let inner = match self.advance() {
            Some('\\') => match self.advance() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\r',
                Some('\\') => '\\',
                Some('\'') => '\'',
                Some('"') => '"',
                Some('0') => '\0',
                Some('a') => '\x07',
                Some('b') => '\x08',
                Some('f') => '\x0C',
                Some('v') => '\x0B',
                Some(c) => c,
                None => '\0',
            },
            Some(c) => c,
            None => '\0',
        };

        if self.peek() == Some('\'') {
            self.advance(); // consume closing quote
            if is_unicode || !inner.is_ascii() {
                TokenKind::UChar(inner as u32)
            } else {
                TokenKind::Char(inner)
            }
        } else {
            TokenKind::Error(format!(
                "Invalid char literal starting at line {}: expected closing '\''",
                start_line
            ))
        }
    }

    pub fn next_token(&mut self) -> Token {
        if self.in_template {
            let start_line = self.line;
            let start_column = self.column;

            if self.is_at_end() {
                self.in_template = false;
                self.template_stack.clear();
                return Token::new(TokenKind::EOF, start_line, start_column);
            }

            // 1. Check if lookahead starts an interpolation hole: '@f{' or '@f {'
            if self.peek() == Some('@') && self.peek_at(1) == Some('f') {
                let mut lookahead = 2;
                while let Some(ch) = self.peek_at(lookahead) {
                    if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                        lookahead += 1;
                    } else {
                        break;
                    }
                }
                if self.peek_at(lookahead) == Some('{') {
                    self.advance(); // consume '@'
                    self.advance(); // consume 'f'
                    while let Some(ch) = self.peek() {
                        if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                            if ch == '\n' {
                                self.line += 1;
                                self.column = 1;
                            }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    self.at_f_pending = true;
                    self.in_template = false;
                    return Token::new(
                        TokenKind::LabelName("@f".to_string()),
                        start_line,
                        start_column,
                    );
                }
            }

            // 2. Check if we are at closing '}' of this template level
            if self.peek() == Some('}') {
                if let Some(&depth) = self.template_stack.last() {
                    if depth == 1 {
                        self.advance(); // consume '}'
                        self.template_stack.pop();
                        self.in_template =
                            !self.template_stack.is_empty() && self.code_brace_stack.is_empty();
                        return Token::new(TokenKind::RBrace, start_line, start_column);
                    }
                }
            }

            // 3. Scan template text chunk
            let mut text = String::new();
            while !self.is_at_end() {
                if self.peek() == Some('@') {
                    if self.peek_at(1) == Some('f') {
                        let mut lookahead = 2;
                        while let Some(ch) = self.peek_at(lookahead) {
                            if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                                lookahead += 1;
                            } else {
                                break;
                            }
                        }
                        if self.peek_at(lookahead) == Some('{') {
                            break;
                        }
                    } else if self.peek_at(1) == Some('@') {
                        self.advance();
                        self.advance();
                        text.push('@');
                        continue;
                    }
                }

                if self.peek() == Some('}') {
                    if let Some(&depth) = self.template_stack.last() {
                        if depth == 1 {
                            break;
                        } else {
                            *self.template_stack.last_mut().unwrap() -= 1;
                            let ch = self.advance().unwrap();
                            text.push(ch);
                            continue;
                        }
                    }
                }

                if self.peek() == Some('{') {
                    if let Some(depth) = self.template_stack.last_mut() {
                        *depth += 1;
                    }
                    let ch = self.advance().unwrap();
                    text.push(ch);
                    continue;
                }

                let ch = self.advance().unwrap();
                if ch == '\n' {
                    self.line += 1;
                    self.column = 1;
                }
                text.push(ch);
            }

            return Token::new(TokenKind::TemplateText(text), start_line, start_column);
        }

        self.skip_whitespace();

        // Capture the start position AFTER whitespace/comments are skipped by the
        // caller's previous call, but BEFORE this token's own characters are consumed.
        let start_line = self.line;
        let start_column = self.column;

        if self.is_at_end() {
            return Token::new(TokenKind::EOF, start_line, start_column);
        }

        let c = self.advance().unwrap();

        let kind = match c {
            'a'..='z' | 'A'..='Z' | '_' | '$' => {
                if (c == 'u' || c == 'U') && self.peek() == Some('"') {
                    self.advance(); // consume '"'
                    self.scan_string_literal(start_line, true)
                } else if (c == 'u' || c == 'U') && self.peek() == Some('\'') {
                    self.advance(); // consume '\''
                    self.scan_char_literal(start_line, true)
                } else {
                    let mut word = String::new();
                    word.push(c);
                    while let Some(next_c) = self.peek() {
                        if next_c.is_alphanumeric() || next_c == '_' || next_c == '$' {
                            word.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                    if let Some(keyword) = Self::check_keyword(&word) {
                        keyword
                    } else {
                        TokenKind::Identifier(word.clone())
                    }
                }
            }
            '@' => {
                let mut word = String::new();
                word.push(c);
                while let Some(next_c) = self.peek() {
                    if next_c.is_alphanumeric() || next_c == '_' {
                        word.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
                if word.len() == 1 {
                    TokenKind::At
                } else {
                    TokenKind::LabelName(word.clone())
                }
            }
            '0'..='9' => {
                let start_line = self.line;
                // Check for radix prefix if c == '0'
                if c == '0' && matches!(self.peek(), Some('x' | 'X' | 'b' | 'B' | 'o' | 'O')) {
                    let prefix = self.advance().unwrap();
                    let radix = match prefix {
                        'x' | 'X' => 16,
                        'b' | 'B' => 2,
                        'o' | 'O' => 8,
                        _ => unreachable!(),
                    };
                    let mut digits_str = String::new();
                    while let Some(next_c) = self.peek() {
                        if next_c == '_' {
                            self.advance(); // consume '_' separator
                            continue;
                        }
                        let is_valid_digit = match radix {
                            16 => next_c.is_ascii_hexdigit(),
                            2 => next_c == '0' || next_c == '1',
                            8 => matches!(next_c, '0'..='7'),
                            _ => false,
                        };
                        if is_valid_digit {
                            digits_str.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }
                    if digits_str.is_empty() {
                        TokenKind::Error(format!(
                            "Empty numeric literal with prefix '0{}' at line {}",
                            prefix, start_line
                        ))
                    } else {
                        // Optional suffix (e.g. u8, u16, u32, u64, u128, uint, usize, i8, i16, i32, i64, i128, int, isize)
                        let mut suffix = String::new();
                        while let Some(next_c) = self.peek() {
                            if next_c.is_alphanumeric() || next_c == '_' {
                                suffix.push(self.advance().unwrap());
                            } else {
                                break;
                            }
                        }
                        let is_unsigned =
                            suffix.starts_with('u') || suffix == "usize" || suffix == "byte";
                        match u128::from_str_radix(&digits_str, radix) {
                            Ok(u_val) => {
                                if is_unsigned || u_val > i128::MAX as u128 {
                                    TokenKind::UInt(u_val)
                                } else {
                                    TokenKind::Int(u_val as i128)
                                }
                            }
                            Err(_) => TokenKind::Error(format!(
                                "Numeric literal '0{}{}{}' out of range at line {}",
                                prefix, digits_str, suffix, start_line
                            )),
                        }
                    }
                } else {
                    let mut num_str = String::new();
                    num_str.push(c);

                    while let Some(next_c) = self.peek() {
                        if next_c == '_' {
                            self.advance(); // consume '_'
                            continue;
                        }
                        if next_c.is_ascii_digit() {
                            num_str.push(self.advance().unwrap());
                        } else {
                            break;
                        }
                    }

                    // Float: a '.' followed by at least one digit. A trailing bare '.'
                    // (e.g. `5.` or `5.foo`) is left alone so `.` can still be a Dot token
                    // (property access, etc.) on the next scan.
                    let is_float = self.peek() == Some('.')
                        && matches!(self.peek_at(1), Some(d) if d.is_ascii_digit());

                    if is_float {
                        num_str.push(self.advance().unwrap()); // consume '.'
                        while let Some(next_c) = self.peek() {
                            if next_c == '_' {
                                self.advance();
                                continue;
                            }
                            if next_c.is_ascii_digit() {
                                num_str.push(self.advance().unwrap());
                            } else {
                                break;
                            }
                        }
                        // Optional float suffix (f32, f64, float)
                        let mut suffix = String::new();
                        while let Some(next_c) = self.peek() {
                            if next_c.is_alphanumeric() || next_c == '_' {
                                suffix.push(self.advance().unwrap());
                            } else {
                                break;
                            }
                        }
                        match num_str.parse::<f64>() {
                            Ok(f) => TokenKind::Float(f),
                            Err(_) => TokenKind::Error(format!(
                                "Invalid float literal '{}{}' at line {}",
                                num_str, suffix, start_line
                            )),
                        }
                    } else {
                        // Optional int/uint suffix
                        let mut suffix = String::new();
                        while let Some(next_c) = self.peek() {
                            if next_c.is_alphanumeric() || next_c == '_' {
                                suffix.push(self.advance().unwrap());
                            } else {
                                break;
                            }
                        }
                        let is_unsigned =
                            suffix.starts_with('u') || suffix == "usize" || suffix == "byte";
                        let is_float_suffix = suffix.starts_with('f') || suffix == "float";
                        if is_float_suffix {
                            match num_str.parse::<f64>() {
                                Ok(f) => TokenKind::Float(f),
                                Err(_) => TokenKind::Error(format!(
                                    "Invalid float literal '{}{}' at line {}",
                                    num_str, suffix, start_line
                                )),
                            }
                        } else if is_unsigned {
                            match num_str.parse::<u128>() {
                                Ok(u) => TokenKind::UInt(u),
                                Err(_) => TokenKind::Error(format!(
                                    "Unsigned integer literal '{}{}' out of range at line {}",
                                    num_str, suffix, start_line
                                )),
                            }
                        } else {
                            match num_str.parse::<i128>() {
                                Ok(i) => TokenKind::Int(i),
                                Err(_) => match num_str.parse::<u128>() {
                                    Ok(u) => TokenKind::UInt(u),
                                    Err(_) => TokenKind::Error(format!(
                                        "Integer literal '{}{}' out of range at line {}",
                                        num_str, suffix, start_line
                                    )),
                                },
                            }
                        }
                    }
                }
            }

            '"' => self.scan_string_literal(start_line, false),
            '\'' => self.scan_char_literal(start_line, false),

            // comments
            '/' => {
                if let Some('/') = self.peek() {
                    self.advance(); // consume the second '/'
                    while let Some(next_c) = self.peek() {
                        if next_c == '\n' {
                            // Leave the newline for skip_whitespace to consume on the
                            // next call, so line/column bookkeeping only happens in
                            // one place.
                            break;
                        }
                        self.advance();
                    }
                    TokenKind::InlineComment
                } else if let Some('*') = self.peek() {
                    self.advance(); // consume the '*' that opens the block comment
                    let mut closed = false;
                    while let Some(next_c) = self.peek() {
                        if next_c == '*' && self.peek_at(1) == Some('/') {
                            self.advance(); // consume '*'
                            self.advance(); // consume '/'
                            closed = true;
                            break;
                        }
                        if next_c == '\n' {
                            self.line += 1;
                            self.column = 1;
                        }
                        self.advance();
                    }
                    if closed {
                        TokenKind::MultiLineComment
                    } else {
                        TokenKind::Error(format!(
                            "Unterminated block comment starting at line {}",
                            start_line
                        ))
                    }
                } else if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::DivAssign
                } else {
                    TokenKind::Divide
                }
            }
            ':' => {
                if let Some(':') = self.peek() {
                    self.advance();
                    TokenKind::DoubleColon
                } else if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::Walrus
                } else {
                    TokenKind::Colon
                }
            }
            ';' => TokenKind::SemiColon,
            '?' => TokenKind::Question,
            ',' => TokenKind::Comma,
            '.' => {
                if let Some('.') = self.peek() {
                    self.advance();
                    if let Some('.') = self.peek() {
                        self.advance();
                        TokenKind::DotDotDot
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            '+' => {
                if let Some('+') = self.peek() {
                    self.advance();
                    TokenKind::PlusPlus
                } else if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::PlusAssign
                } else {
                    TokenKind::Plus
                }
            }
            '*' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::MulAssign
                } else {
                    TokenKind::Multiply
                }
            }
            '%' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::ModAssign
                } else {
                    TokenKind::Mod
                }
            }
            '{' => {
                if let Some('{') = self.peek() {
                    self.advance();
                    TokenKind::DoubleLBrace
                } else {
                    if self.at_f_pending {
                        self.at_f_pending = false;
                        self.code_brace_stack.push(1);
                    } else if !self.code_brace_stack.is_empty() {
                        *self.code_brace_stack.last_mut().unwrap() += 1;
                    }
                    TokenKind::LBrace
                }
            }
            '}' => {
                if let Some('}') = self.peek() {
                    self.advance();
                    TokenKind::DoubleRBrace
                } else {
                    if let Some(top) = self.code_brace_stack.last_mut() {
                        *top -= 1;
                        if *top == 0 {
                            self.code_brace_stack.pop();
                            if !self.template_stack.is_empty() {
                                self.in_template = true;
                            }
                        }
                    }
                    TokenKind::RBrace
                }
            }
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '=' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::Eq
                } else if let Some('>') = self.peek() {
                    self.advance();
                    TokenKind::FatArrow
                } else {
                    TokenKind::Assign
                }
            }
            '!' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    TokenKind::Not
                }
            }
            '-' => {
                if let Some('-') = self.peek() {
                    self.advance();
                    TokenKind::MinusMinus
                } else if let Some('>') = self.peek() {
                    self.advance();
                    TokenKind::Arrow
                } else if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::MinusAssign
                } else if let Some('.') = self.peek() {
                    self.advance();
                    TokenKind::DashDot
                } else {
                    TokenKind::Minus
                }
            }
            '&' => {
                if let Some('&') = self.peek() {
                    self.advance();
                    TokenKind::And
                } else {
                    TokenKind::Ampersand
                }
            }
            '|' => {
                if let Some('|') = self.peek() {
                    self.advance();
                    TokenKind::Or
                } else {
                    TokenKind::Pipe
                }
            }
            '>' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::GreaterEq
                } else if let Some('>') = self.peek() {
                    self.advance();
                    TokenKind::ShiftRight
                } else {
                    TokenKind::Greater
                }
            }
            '<' => {
                if let Some('=') = self.peek() {
                    self.advance();
                    TokenKind::LessEq
                } else if let Some('<') = self.peek() {
                    self.advance();
                    TokenKind::ShiftLeft
                } else {
                    TokenKind::Less
                }
            }
            '^' => TokenKind::Caret,
            '#' => {
                if let Some('{') = self.peek() {
                    self.advance(); // consume '{'
                    let _start_line: usize = self.line;
                    let mut text = String::new();
                    let mut depth = 1;
                    while !self.is_at_end() {
                        let ch = self.advance().unwrap();
                        if ch == '{' {
                            depth += 1;
                            text.push(ch);
                        } else if ch == '}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            text.push(ch);
                        } else {
                            if ch == '\n' {
                                self.line += 1;
                                self.column = 1;
                            }
                            text.push(ch);
                        }
                    }
                    TokenKind::TemplateText(text)
                } else {
                    TokenKind::Hash
                }
            }
            '`' => {
                let start_line = self.line;
                let mut text = String::new();
                let mut closed = false;
                while !self.is_at_end() {
                    let ch = self.advance().unwrap();
                    if ch == '`' {
                        closed = true;
                        break;
                    }
                    if ch == '\n' {
                        self.line += 1;
                        self.column = 1;
                    }
                    text.push(ch);
                }
                if !closed {
                    TokenKind::Error(format!("Unterminated raw template delimited by backticks at line {}", start_line))
                } else {
                    let trimmed = text.trim();
                    let inner = if trimmed.starts_with('[') && trimmed.ends_with(']') {
                        &trimmed[1..trimmed.len() - 1]
                    } else {
                        trimmed
                    };
                    TokenKind::TemplateText(inner.to_string())
                }
            }

            other => TokenKind::Error(format!(
                "Unexpected character '{}' at line {}",
                other, start_line
            )),
        };

        Token::new(kind, start_line, start_column)
    }
}
