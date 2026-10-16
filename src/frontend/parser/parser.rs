use core::mem::discriminant;
use std::collections::HashMap;

use crate::frontend::lexer::token::{Token, TokenKind};
use crate::frontend::parser::ast::*;

// Fallback token returned when `current` runs past the end of the stream.
// Explicit `const` instead of an inline `&Token { .. }` literal so we don't
// depend on rvalue static-promotion rules to make the borrow-check work.
const EOF_TOKEN: Token = Token {
    kind: TokenKind::EOF,
    line: 0,
    column: 0,
};

pub struct Parser {
    pub(crate) tokens: Vec<Token>,
    pub(crate) current: usize,
    pub(crate) metadata: HashMap<String, TypeMetadata>,
    pub(crate) var_metadata: HashMap<String, VarMetadata>,
    pub(crate) fn_metadata: HashMap<String, FnType>,
    pub(crate) macro_metadata: std::collections::HashSet<String>,
    pub(crate) current_generics: std::collections::HashSet<String>,
    pub(crate) type_aliases: HashMap<String, BaseType>,
    pub(crate) in_match_pattern: bool,
}

impl Parser {
    pub fn new_with_aliases(tokens: Vec<Token>, type_aliases: HashMap<String, BaseType>) -> Self {
        let mut macro_metadata = std::collections::HashSet::new();
        for i in 0..tokens.len() {
            if tokens[i].kind == TokenKind::TypeMacro {
                if let Some(next) = tokens.get(i + 1) {
                    if let TokenKind::Identifier(ref name) = next.kind {
                        macro_metadata.insert(name.clone());
                    }
                }
            }
        }

        Parser {
            tokens,
            current: 0,
            metadata: HashMap::new(),
            var_metadata: HashMap::new(),
            fn_metadata: HashMap::new(),
            macro_metadata,
            current_generics: std::collections::HashSet::new(),
            type_aliases,
            in_match_pattern: false,
        }
    }

    pub fn new(tokens: Vec<Token>) -> Self {
        Self::new_with_aliases(tokens, HashMap::new())
    }

    pub(crate) fn peek(&self) -> &Token {
        // NOTE: EOF placeholder for out-of-range access carries dummy position 0,0.
        // Should not happen in practice since is_at_end() gates the main loops.
        self.tokens.get(self.current).unwrap_or(&EOF_TOKEN)
    }
    pub(crate) fn peek_ahead(&self, n: usize) -> Option<&Token> {
        self.tokens.get(self.current + n)
    }
    pub(crate) fn previous(&self, n: Option<usize>) -> &Token {
        let n = n.unwrap_or(1);
        &self.tokens[self.current - n]
    }

    pub(crate) fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::EOF
    }

    pub(crate) fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous(None)
    }

    pub(crate) fn is_compile_validation_ahead(&self) -> bool {
        let mut idx = self.current + 1;
        if self.tokens.get(idx).map(|t| &t.kind) == Some(&TokenKind::Arrow) {
            idx += 1;
        }
        self.tokens.get(idx).map(|t| &t.kind) == Some(&TokenKind::LBrace)
    }

    /// Keywords that can legally appear as identifiers in name positions
    /// (variable names, field names, parameter names).
    /// e.g. `let bool flag = ...` where 'flag' is a keyword we registered.

    pub fn parse_program(&mut self) -> Result<Program, String> {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            let prev_cur = self.current;
            match self.parse_statement(ScopeType::Global) {
                Ok(Some(stmt)) => {
                    if self.current == prev_cur {
                        panic!("Parser infinite loop: parse_statement returned Ok(Some) without advancing at token {:?}", self.peek());
                    }
                    statements.push(stmt);
                }

                // Skip empty statements
                Ok(None) => {
                    if self.current == prev_cur {
                        panic!("Parser infinite loop: parse_statement returned Ok(None) without advancing at token {:?}", self.peek());
                    }
                    continue;
                }

                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(Program {
            statements: statements,
        })
    }

    pub(crate) fn consume(
        &mut self,
        expected: TokenKind,
        error_message: &str,
    ) -> Result<&Token, String> {
        if discriminant(&self.peek().kind) == discriminant(&expected) {
            Ok(self.advance())
        } else {
            // Future location for line-based diagnostics and hints
            // NOTE: self.peek().line / .column are now available for exactly this.
            Err(format!(
                "{} (at line {}, column {})",
                error_message,
                self.peek().line,
                self.peek().column
            ))
        }
    }

    // Synchronize parser state after encountering a syntax error
    pub(crate) fn synchronize(&mut self) {
        self.advance();

        while !self.is_at_end() {
            if self.previous(None).kind == TokenKind::SemiColon {
                return;
            }

            match &self.peek().kind {
                TokenKind::Set
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::While
                | TokenKind::Loop
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Return
                | TokenKind::Fn
                | TokenKind::TypeClass
                | TokenKind::TypeEnum
                | TokenKind::TypeStruct => {
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ----------------------------------------------------
    // Statement Parsing
    // ----------------------------------------------------

    // Dispatches statement parsing based on token kind
}
