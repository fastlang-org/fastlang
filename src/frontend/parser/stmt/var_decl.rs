use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;

#[derive(Debug, Clone, PartialEq)]
pub enum RhsValue {
    Single(Expr),
    Group(Vec<RhsValue>),
}

impl Parser {
    pub(crate) fn parse_const(&mut self, scope: ScopeType) -> Result<Decl, String> {
        self.advance(); // consume 'const'

        let mut decl = match &self.peek().kind {
            TokenKind::TypeInt(_)
            | TokenKind::TypeUInt(_)
            | TokenKind::TypeUSize
            | TokenKind::TypeISize
            | TokenKind::TypeFloat(_)
            | TokenKind::TypeChar
            | TokenKind::TypeUChar
            | TokenKind::TypeStr
            | TokenKind::TypeUStr
            | TokenKind::TypeBool
            | TokenKind::TypeType
            | TokenKind::Identifier(_) => self.parse_var_decl(scope)?,
            _ => {
                return Err("Syntax Error: Expected variable declaration".to_string());
            }
        };

        match &mut decl {
            Decl::VarDecl {
                ref mut editability,
                ..
            } => {
                *editability = Editability::NotEditable;
            }
            Decl::DestructureDecl {
                ref mut editability,
                ..
            } => {
                *editability = Editability::NotEditable;
            }
            Decl::ArrayDecl {
                ref mut editability,
                ..
            } => {
                *editability = Editability::NotEditable;
            }
            _ => {
                return Err("Syntax Error: Only variables can be declared as const".to_string());
            }
        }

        Ok(decl)
    }

    pub(crate) fn consume_optional_let(&mut self) {
        if let TokenKind::Identifier(name) = &self.peek().kind {
            if name == "let" {
                self.advance();
            }
        }
    }

    pub(crate) fn is_var_decl_start(&self) -> bool {
        match &self.peek().kind {
            | TokenKind::Const
            | TokenKind::TypeInt(_)
            | TokenKind::TypeUInt(_)
            | TokenKind::TypeUSize
            | TokenKind::TypeISize
            | TokenKind::TypeFloat(_)
            | TokenKind::TypeChar
            | TokenKind::TypeUChar
            | TokenKind::TypeStr
            | TokenKind::TypeUStr
            | TokenKind::TypeBool
            | TokenKind::TypeVoid
            | TokenKind::Flag
            | TokenKind::TypeType
            | TokenKind::TypeMethod => {
                if let Some(next) = self.tokens.get(self.current + 1) {
                    if let TokenKind::Identifier(_) = next.kind {
                        if let Some(after) = self.tokens.get(self.current + 2) {
                            if after.kind == TokenKind::LParen || after.kind == TokenKind::Less {
                                return false;
                            }
                        }
                    }
                }
                true
            }
            | TokenKind::TypeFn
            | TokenKind::TypeLambda
            | TokenKind::TypeAuto => true,
            TokenKind::Identifier(_) => {
                if let Some(next) = self.tokens.get(self.current + 1) {
                    if next.kind == TokenKind::Walrus {
                        return true;
                    }
                }
                self.is_custom_type_var_decl_start_at(self.current)
            }
            _ => false,
        }
    }

    pub(crate) fn is_custom_type_var_decl_start_at(&self, start: usize) -> bool {
        let mut idx = start;
        if idx >= self.tokens.len() {
            return false;
        }
        let first_name = match &self.tokens[idx].kind {
            TokenKind::Identifier(name) => name.clone(),
            _ => return false,
        };
        if self.macro_metadata.contains(&first_name) {
            return false;
        }
        idx += 1;
        while idx < self.tokens.len() && self.tokens[idx].kind == TokenKind::DoubleColon {
            idx += 1;
            if idx >= self.tokens.len()
                || !matches!(&self.tokens[idx].kind, TokenKind::Identifier(_))
            {
                return false;
            }
            idx += 1;
        }
        if idx >= self.tokens.len() {
            return false;
        }
        if matches!(&self.tokens[idx].kind, TokenKind::Ampersand | TokenKind::Multiply) {
            while idx < self.tokens.len()
                && matches!(&self.tokens[idx].kind, TokenKind::Ampersand | TokenKind::Multiply)
            {
                idx += 1;
            }
            if idx < self.tokens.len() && matches!(&self.tokens[idx].kind, TokenKind::Identifier(_)) {
                return true;
            }
            return false;
        }

        if matches!(&self.tokens[idx].kind, TokenKind::Identifier(_)) {
            if let Some(after) = self.tokens.get(idx + 1) {
                if after.kind == TokenKind::Comma {
                    return false;
                }
            }
            return true;
        }

        if self.tokens[idx].kind == TokenKind::LParen {
            let mut p_depth = 0;
            let mut p_idx = idx;
            while p_idx < self.tokens.len() {
                if self.tokens[p_idx].kind == TokenKind::LParen {
                    p_depth += 1;
                } else if self.tokens[p_idx].kind == TokenKind::RParen {
                    p_depth -= 1;
                    if p_depth == 0 {
                        let next = self.tokens.get(p_idx + 1).map(|t| &t.kind);
                        if matches!(next, Some(TokenKind::Assign) | Some(TokenKind::Arrow) | Some(TokenKind::Comma)) {
                            return true;
                        }
                        break;
                    }
                } else if matches!(self.tokens[p_idx].kind, TokenKind::SemiColon | TokenKind::EOF) {
                    break;
                }
                p_idx += 1;
            }
        }

        if self.tokens[idx].kind == TokenKind::Less {
            let mut depth = 0;
            while idx < self.tokens.len() {
                if self.tokens[idx].kind == TokenKind::Less {
                    depth += 1;
                } else if self.tokens[idx].kind == TokenKind::Greater {
                    depth -= 1;
                    if depth == 0 {
                        idx += 1;
                        break;
                    }
                } else if matches!(
                    self.tokens[idx].kind,
                    TokenKind::SemiColon | TokenKind::Assign | TokenKind::Arrow | TokenKind::EOF
                ) {
                    return false;
                }
                idx += 1;
            }
            if depth != 0 || idx >= self.tokens.len() {
                return false;
            }
            if matches!(&self.tokens[idx].kind, TokenKind::Identifier(_)) {
                return true;
            }
            if self.tokens[idx].kind == TokenKind::LParen {
                let mut p_depth = 0;
                let mut p_idx = idx;
                while p_idx < self.tokens.len() {
                    if self.tokens[p_idx].kind == TokenKind::LParen {
                        p_depth += 1;
                    } else if self.tokens[p_idx].kind == TokenKind::RParen {
                        p_depth -= 1;
                        if p_depth == 0 {
                            let next = self.tokens.get(p_idx + 1).map(|t| &t.kind);
                            if matches!(next, Some(TokenKind::Assign) | Some(TokenKind::Arrow) | Some(TokenKind::Comma)) {
                                return true;
                            }
                            break;
                        }
                    } else if matches!(self.tokens[p_idx].kind, TokenKind::SemiColon | TokenKind::EOF) {
                        break;
                    }
                    p_idx += 1;
                }
            }
        }

        if self.tokens[idx].kind == TokenKind::LBracket {
            while idx < self.tokens.len() && self.tokens[idx].kind == TokenKind::LBracket {
                let mut depth = 0;
                while idx < self.tokens.len() {
                    if self.tokens[idx].kind == TokenKind::LBracket {
                        depth += 1;
                    } else if self.tokens[idx].kind == TokenKind::RBracket {
                        depth -= 1;
                        if depth == 0 {
                            idx += 1;
                            break;
                        }
                    } else if matches!(
                        self.tokens[idx].kind,
                        TokenKind::SemiColon
                            | TokenKind::Assign
                            | TokenKind::Arrow
                            | TokenKind::EOF
                    ) {
                        return false;
                    }
                    idx += 1;
                }
                if depth != 0 {
                    return false;
                }
            }
            if idx < self.tokens.len() && matches!(&self.tokens[idx].kind, TokenKind::Identifier(_))
            {
                return true;
            }
        }

        false
    }

    pub(crate) fn is_named_destructure_start_at(&self, start: usize) -> bool {
        let mut idx = start;
        if idx >= self.tokens.len() {
            return false;
        }
        if !matches!(&self.tokens[idx].kind, TokenKind::Identifier(_)) {
            return false;
        }
        idx += 1;
        while idx < self.tokens.len() && self.tokens[idx].kind == TokenKind::DoubleColon {
            idx += 1;
            if idx >= self.tokens.len()
                || !matches!(&self.tokens[idx].kind, TokenKind::Identifier(_))
            {
                return false;
            }
            idx += 1;
        }
        if idx >= self.tokens.len() {
            return false;
        }
        let open_kind = &self.tokens[idx].kind;
        if *open_kind != TokenKind::LBrace && *open_kind != TokenKind::LParen {
            return false;
        }
        let (open_tok, close_tok) = if *open_kind == TokenKind::LBrace {
            (TokenKind::LBrace, TokenKind::RBrace)
        } else {
            (TokenKind::LParen, TokenKind::RParen)
        };
        let mut depth = 0;
        while idx < self.tokens.len() {
            if self.tokens[idx].kind == open_tok {
                depth += 1;
            } else if self.tokens[idx].kind == close_tok {
                depth -= 1;
                if depth == 0 {
                    let next = self.tokens.get(idx + 1).map(|t| &t.kind);
                    return matches!(next, Some(TokenKind::Assign) | Some(TokenKind::Arrow));
                }
            } else if self.tokens[idx].kind == TokenKind::EOF {
                break;
            }
            idx += 1;
        }
        false
    }

    pub(crate) fn is_named_destructure_start(&self) -> bool {
        self.is_named_destructure_start_at(self.current)
    }

    pub(crate) fn parse_named_destructure_decl(
        &mut self,
        editability: Editability,
        scope: ScopeType,
    ) -> Result<Stmt, String> {
        let mut type_name = self.get_identifier("Expected type name in destructure")?;
        while self.peek().kind == TokenKind::DoubleColon {
            self.advance();
            let sub = self.get_identifier("Expected identifier after '::'")?;
            type_name.push_str("::");
            type_name.push_str(&sub);
        }
        let is_tuple = self.peek().kind == TokenKind::LParen;
        if is_tuple {
            self.consume(TokenKind::LParen, "Expected '(' in tuple destructure")?;
        } else {
            self.consume(TokenKind::LBrace, "Expected '{' in struct destructure")?;
        }
        let close_kind = if is_tuple {
            TokenKind::RParen
        } else {
            TokenKind::RBrace
        };
        let mut fields = Vec::new();
        while !self.is_at_end() && self.peek().kind != close_kind {
            if self.peek().kind == TokenKind::DotDot {
                self.advance(); // consume '..'
                if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma
                {
                    self.advance();
                }
                continue;
            }
            let (type_node, name) = if self.is_var_decl_start() {
                let ty = self.parse_type()?;
                let n = self.get_identifier("Expected variable name in destructure")?;
                (ty, n)
            } else {
                let n = self.get_identifier("Expected variable name in destructure")?;
                (BaseType::Unknown, n)
            };
            if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
                self.advance();
            }
            let var_meta = VarMetadata {
                name: name.clone(),
                type_node: type_node.clone(),
                visibility: Visibility::Private,
                editability: editability.clone(),
                scope: scope.clone(),
                is_array: false,
            };
            self.var_metadata.insert(name.clone(), var_meta);
            fields.push((type_node, name));
        }
        self.consume(
            close_kind,
            "Expected closing brace/paren after destructure fields",
        )?;
        self.consume(TokenKind::Assign, "Expected '=' after destructure pattern")?;
        let rhs = self.parse_expression()?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }
        Ok(Stmt::Declaration(Decl::ObjectDestructureDecl {
            visibility: Visibility::Private,
            editability,
            type_name: Some(type_name),
            fields,
            rhs,
        }))
    }

    pub(crate) fn is_object_destructure_start(&self) -> bool {
        if self.peek().kind != TokenKind::LBrace {
            return false;
        }
        let mut depth = 0;
        let mut idx = self.current;
        while idx < self.tokens.len() {
            match &self.tokens[idx].kind {
                TokenKind::LBrace => {
                    depth += 1;
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        let next = self.tokens.get(idx + 1).map(|t| &t.kind);
                        return matches!(next, Some(TokenKind::Assign) | Some(TokenKind::Arrow));
                    }
                }
                TokenKind::EOF => {
                    break;
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(crate) fn parse_object_destructure_decl(
        &mut self,
        editability: Editability,
        scope: ScopeType,
    ) -> Result<Stmt, String> {
        self.consume(
            TokenKind::LBrace,
            "Expected '{' to start object destructure",
        )?;
        let mut fields = Vec::new();
        while !self.is_at_end() && self.peek().kind != TokenKind::RBrace {
            if self.peek().kind == TokenKind::DotDot {
                self.advance(); // consume '..'
                if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma
                {
                    self.advance();
                }
                continue;
            }
            let (type_node, name) = if self.is_var_decl_start() {
                let ty = self.parse_type()?;
                let n = self.get_identifier("Expected variable name in destructure")?;
                (ty, n)
            } else {
                let n = self.get_identifier("Expected variable name in destructure")?;
                (BaseType::Unknown, n)
            };
            if self.peek().kind == TokenKind::SemiColon || self.peek().kind == TokenKind::Comma {
                self.advance();
            }
            let var_meta = VarMetadata {
                name: name.clone(),
                type_node: type_node.clone(),
                visibility: Visibility::Private,
                editability: editability.clone(),
                scope: scope.clone(),
                is_array: false,
            };
            self.var_metadata.insert(name.clone(), var_meta);
            fields.push((type_node, name));
        }
        self.consume(
            TokenKind::RBrace,
            "Expected '}' after object destructure fields",
        )?;
        self.consume(
            TokenKind::Assign,
            "Expected '=' after object destructure pattern",
        )?;
        let rhs = self.parse_expression()?;
        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }
        Ok(Stmt::Declaration(Decl::ObjectDestructureDecl {
            visibility: Visibility::Private,
            editability,
            type_name: None,
            fields,
            rhs,
        }))
    }

    pub(crate) fn parse_for_init_stmt(&mut self) -> Result<Stmt, String> {
        self.consume_optional_let();
        if self.is_var_decl_start() {
            self.parse_var_decl(ScopeType::Block).map(Stmt::Declaration)
        } else {
            self.parse_expression_stmt()
        }
    }

    pub(crate) fn parse_pattern_item(&mut self) -> Result<Pattern, String> {
        match &self.peek().kind.clone() {
            TokenKind::LParen => {
                self.advance();
                let mut items = Vec::new();
                if self.peek().kind != TokenKind::RParen {
                    items.push(self.parse_pattern_item()?);
                    while self.peek().kind == TokenKind::Comma {
                        self.advance();
                        items.push(self.parse_pattern_item()?);
                    }
                }
                self.consume(TokenKind::RParen, "Expected ')' after pattern tuple")?;
                if items.len() > 3 {
                    println!(
                        "Warning (Clean Code): Stack destructuring contains {} elements (recommended max: 3).",
                        items.len()
                    );
                }
                for sub in &items {
                    if let Pattern::Tuple(sub_items) = sub {
                        if sub_items.len() > 2 {
                            println!(
                                "Warning (Clean Code): Nested stack destructuring contains {} elements (recommended max: 2).",
                                sub_items.len()
                            );
                        }
                    }
                }
                Ok(Pattern::Tuple(items))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                if self.peek().kind != TokenKind::RBracket {
                    items.push(self.parse_pattern_item()?);
                    while self.peek().kind == TokenKind::Comma {
                        self.advance();
                        items.push(self.parse_pattern_item()?);
                    }
                }
                self.consume(TokenKind::RBracket, "Expected ']' after pattern list")?;
                Ok(Pattern::Tuple(items))
            }
            TokenKind::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::EOF {
                    if self.peek().kind == TokenKind::DotDot {
                        self.advance();
                        if self.peek().kind == TokenKind::Comma
                            || self.peek().kind == TokenKind::SemiColon
                        {
                            self.advance();
                        }
                        continue;
                    }
                    let fname = self
                        .get_identifier("Expected field identifier inside destructuring braces")?;
                    fields.push(fname);
                    if self.peek().kind == TokenKind::Comma
                        || self.peek().kind == TokenKind::SemiColon
                    {
                        self.advance();
                    }
                }
                self.consume(TokenKind::RBrace, "Expected '}' after destructuring fields")?;
                Ok(Pattern::Struct { name: None, fields })
            }
            _ => {
                let err = format!(
                    "Expected variable name or destructuring pattern, found {:?}",
                    self.peek().kind
                );
                let name = self.get_identifier(&err)?;
                Ok(Pattern::Identifier(name))
            }
        }
    }

    pub(crate) fn parse_destructuring_pattern(&mut self) -> Result<Pattern, String> {
        let first = self.parse_pattern_item()?;
        if self.peek().kind == TokenKind::Comma {
            let mut list = vec![first];
            while self.peek().kind == TokenKind::Comma {
                self.advance();
                list.push(self.parse_pattern_item()?);
            }
            if list.len() > 3 {
                println!(
                    "Warning (Clean Code): Declaring {} variables in a single statement is discouraged for code readability.",
                    list.len()
                );
            }
            Ok(Pattern::Tuple(list))
        } else {
            Ok(first)
        }
    }

    pub(crate) fn parse_rhs_item(&mut self) -> Result<RhsValue, String> {
        if self.peek().kind == TokenKind::LParen && !self.is_lambda_ahead() {
            self.advance();
            let mut items = Vec::new();
            let mut has_comma = false;
            if self.peek().kind != TokenKind::RParen {
                items.push(self.parse_rhs_item()?);
                while self.peek().kind == TokenKind::Comma {
                    has_comma = true;
                    self.advance();
                    items.push(self.parse_rhs_item()?);
                }
            }
            self.consume(TokenKind::RParen, "Expected ')' after group in RHS")?;
            if has_comma || items.len() > 1 {
                Ok(RhsValue::Group(items))
            } else if items.len() == 1 {
                let inner = items.into_iter().next().unwrap();
                if let RhsValue::Single(expr) = inner {
                    let full_expr = self.parse_expr_with_lhs(expr, 0)?;
                    Ok(RhsValue::Single(full_expr))
                } else {
                    Ok(inner)
                }
            } else {
                Ok(RhsValue::Group(Vec::new()))
            }
        } else {
            let expr = self.parse_expression()?;
            Ok(RhsValue::Single(expr))
        }
    }

    pub(crate) fn parse_rhs_values(&mut self) -> Result<RhsValue, String> {
        let first = self.parse_rhs_item()?;
        if self.peek().kind == TokenKind::Comma {
            let mut list = vec![first];
            while self.peek().kind == TokenKind::Comma {
                self.advance();
                list.push(self.parse_rhs_item()?);
            }
            Ok(RhsValue::Group(list))
        } else {
            Ok(first)
        }
    }

    pub(crate) fn parse_var_decl(&mut self, scope: ScopeType) -> Result<Decl, String> {
        if self.peek().kind == TokenKind::Const {
            if let (Some(t1), Some(t2)) = (
                self.tokens.get(self.current + 1),
                self.tokens.get(self.current + 2),
            ) {
                if matches!(&t1.kind, TokenKind::Identifier(_)) && t2.kind == TokenKind::Walrus {
                    self.advance(); // consume 'const'
                    let name = self.get_identifier("Expected variable name after 'const'")?;
                    self.consume(TokenKind::Walrus, "Expected ':=' after variable name")?;
                    let val = self.parse_expression()?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    let visibility = if scope == ScopeType::Global {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    let var_meta = VarMetadata {
                        name: name.clone(),
                        type_node: BaseType::Unknown,
                        visibility: visibility.clone(),
                        editability: Editability::NotEditable,
                        scope,
                        is_array: false,
                    };
                    self.var_metadata.insert(name.clone(), var_meta);
                    return Ok(Decl::VarDecl {
                        visibility,
                        editability: Editability::NotEditable,
                        type_node: BaseType::Unknown,
                        place: Place::Local,
                        name,
                        assign_op: ":=".to_string(),
                        value: val,
                    });
                }
            }
        }

        if let TokenKind::Identifier(ref _ident) = self.peek().kind {
            if let Some(t1) = self.tokens.get(self.current + 1) {
                if t1.kind == TokenKind::Walrus {
                    let name = self.get_identifier("Expected variable name")?;
                    self.consume(TokenKind::Walrus, "Expected ':=' after variable name")?;
                    let val = self.parse_expression()?;
                    if self.peek().kind == TokenKind::SemiColon {
                        self.advance();
                    }
                    let visibility = if scope == ScopeType::Global {
                        Visibility::Public
                    } else {
                        Visibility::Private
                    };
                    let var_meta = VarMetadata {
                        name: name.clone(),
                        type_node: BaseType::Unknown,
                        visibility: visibility.clone(),
                        editability: Editability::Editable,
                        scope,
                        is_array: false,
                    };
                    self.var_metadata.insert(name.clone(), var_meta);
                    return Ok(Decl::VarDecl {
                        visibility,
                        editability: Editability::Editable,
                        type_node: BaseType::Unknown,
                        place: Place::Local,
                        name,
                        assign_op: ":=".to_string(),
                        value: val,
                    });
                }
            }
        }

        let type_name = self.parse_type()?;
        let pattern = self.parse_destructuring_pattern()?;

        let mut is_arr = false;
        let mut array_sizes = Vec::new();
        if let Pattern::Identifier(_) = &pattern {
            while self.peek().kind == TokenKind::LBracket {
                is_arr = true;
                self.advance();
                let size = if self.peek().kind != TokenKind::RBracket {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                self.consume(TokenKind::RBracket, "Expected ']' after array size")?;
                array_sizes.push(size);
            }
        }

        let mut assign_op = String::new();
        let mut rhs_opt = None;
        if self.peek().kind == TokenKind::Assign || self.peek().kind == TokenKind::Arrow {
            assign_op = self.peek().kind.clone().as_str().to_string();
            self.advance();
            rhs_opt = Some(self.parse_rhs_values()?);
        }

        if self.peek().kind == TokenKind::SemiColon {
            self.advance();
        }

        let visibility = if scope == ScopeType::Global {
            Visibility::Public
        } else {
            Visibility::Private
        };

        match pattern {
            Pattern::Identifier(ref name) if !is_arr => {
                let val = match rhs_opt {
                    Some(RhsValue::Single(expr)) => expr,
                    Some(RhsValue::Group(items)) => {
                        // Case 7: int(32) x = (10, 20); -> ArrayLiteral
                        let mut elems = Vec::new();
                        for item in items {
                            match item {
                                RhsValue::Single(e) => elems.push(e),
                                RhsValue::Group(_) => {
                                    let mut sub = Vec::new();
                                    flatten_rhs(&item, &mut sub);
                                    elems.push(Expr::ArrayLiteral(sub));
                                }
                            }
                        }
                        Expr::ArrayLiteral(elems)
                    }
                    None => Expr::Default(None),
                };

                let var_meta = VarMetadata {
                    name: name.clone(),
                    type_node: type_name.clone(),
                    visibility: visibility.clone(),
                    editability: Editability::Editable,
                    scope,
                    is_array: false,
                };
                self.var_metadata.insert(name.clone(), var_meta);

                let place = if matches!(val, Expr::New { .. }) {
                    Place::Heap
                } else {
                    Place::Local
                };

                Ok(Decl::VarDecl {
                    visibility,
                    editability: Editability::Editable,
                    type_node: type_name,
                    place,
                    assign_op,
                    name: name.clone(),
                    value: val,
                })
            }
            Pattern::Identifier(ref name) if is_arr => {
                let val = match rhs_opt {
                    Some(RhsValue::Single(expr)) => expr,
                    Some(RhsValue::Group(items)) => {
                        let mut elems = Vec::new();
                        for item in items {
                            match item {
                                RhsValue::Single(e) => elems.push(e),
                                RhsValue::Group(_) => {
                                    let mut sub = Vec::new();
                                    flatten_rhs(&item, &mut sub);
                                    elems.push(Expr::ArrayLiteral(sub));
                                }
                            }
                        }
                        Expr::ArrayLiteral(elems)
                    }
                    None => Expr::Default(None),
                };

                let elem_type = if array_sizes.len() > 1 {
                    let mut t = type_name.clone();
                    for size in array_sizes.iter().skip(1).rev() {
                        t = BaseType::Array {
                            base_type: Box::new(t),
                            size: Box::new(size.clone()),
                        };
                    }
                    t
                } else {
                    type_name.clone()
                };

                let var_meta = VarMetadata {
                    name: name.clone(),
                    type_node: elem_type.clone(),
                    visibility: visibility.clone(),
                    editability: Editability::Editable,
                    scope,
                    is_array: true,
                };
                self.var_metadata.insert(name.clone(), var_meta);

                let len_expr = array_sizes
                    .first()
                    .cloned()
                    .flatten()
                    .unwrap_or(Expr::LiteralInt(0));

                Ok(Decl::ArrayDecl {
                    visibility,
                    editability: Editability::Editable,
                    type_node: elem_type,
                    name: name.clone(),
                    assign_op,
                    length: len_expr,
                    value: val,
                })
            }
            _ => {
                // Destructuring (multiple vars, tuple, struct)
                let mut assignments = Vec::new();
                if let Some(rhs) = rhs_opt {
                    match_destructure(&pattern, &rhs, &mut assignments)?;
                } else {
                    let mut names = Vec::new();
                    collect_pattern_identifiers(&pattern, &mut names);
                    for n in names {
                        assignments.push((n, Expr::Default(None)));
                    }
                }

                for (name, _) in &assignments {
                    let var_meta = VarMetadata {
                        name: name.clone(),
                        type_node: type_name.clone(),
                        visibility: visibility.clone(),
                        editability: Editability::Editable,
                        scope: scope.clone(),
                        is_array: false,
                    };
                    self.var_metadata.insert(name.clone(), var_meta);
                }

                Ok(Decl::DestructureDecl {
                    visibility,
                    editability: Editability::Editable,
                    type_node: type_name,
                    pattern,
                    assignments,
                    assign_op,
                })
            }
        }
    }
}

fn match_destructure(
    pattern: &Pattern,
    value: &RhsValue,
    results: &mut Vec<(String, Expr)>,
) -> Result<(), String> {
    match pattern {
        Pattern::Identifier(name) => match value {
            RhsValue::Single(expr) => {
                results.push((name.clone(), expr.clone()));
                Ok(())
            }
            RhsValue::Group(items) => {
                let mut expr_elements = Vec::new();
                for item in items {
                    match item {
                        RhsValue::Single(e) => expr_elements.push(e.clone()),
                        RhsValue::Group(_) => {
                            let mut sub = Vec::new();
                            flatten_rhs(item, &mut sub);
                            expr_elements.push(Expr::ArrayLiteral(sub));
                        }
                    }
                }
                results.push((name.clone(), Expr::ArrayLiteral(expr_elements)));
                Ok(())
            }
        },
        Pattern::Tuple(patterns) => match value {
            RhsValue::Single(Expr::ArrayLiteral(elems)) if elems.len() == patterns.len() => {
                for (p, elem) in patterns.iter().zip(elems.iter()) {
                    match_destructure(p, &RhsValue::Single(elem.clone()), results)?;
                }
                Ok(())
            }
            RhsValue::Single(expr) => {
                for p in patterns {
                    match_destructure(p, &RhsValue::Single(expr.clone()), results)?;
                }
                Ok(())
            }
            RhsValue::Group(items) => {
                if items.len() == 1 {
                    let single_item = &items[0];
                    for p in patterns {
                        match_destructure(p, single_item, results)?;
                    }
                    Ok(())
                } else if patterns.len() == items.len() {
                    for (p, item) in patterns.iter().zip(items.iter()) {
                        match_destructure(p, item, results)?;
                    }
                    Ok(())
                } else {
                    Err(
                        format!(
                            "Semantic Error: Destructuring mismatch: {} variables on LHS vs {} values on RHS.",
                            patterns.len(),
                            items.len()
                        )
                    )
                }
            }
        },
        Pattern::Struct { name: _, fields } => match value {
            RhsValue::Single(expr) => {
                for f in fields {
                    let prop_access = Expr::PropertyAccess {
                        object: Box::new(expr.clone()),
                        property: f.clone(),
                    };
                    results.push((f.clone(), prop_access));
                }
                Ok(())
            }
            RhsValue::Group(_) => Err(
                "Semantic Error: Cannot unpack struct pattern from tuple/group RHS.".to_string(),
            ),
        },
    }
}

fn flatten_rhs(val: &RhsValue, out: &mut Vec<Expr>) {
    match val {
        RhsValue::Single(e) => out.push(e.clone()),
        RhsValue::Group(items) => {
            for it in items {
                flatten_rhs(it, out);
            }
        }
    }
}

fn collect_pattern_identifiers(pattern: &Pattern, out: &mut Vec<String>) {
    match pattern {
        Pattern::Identifier(n) => out.push(n.clone()),
        Pattern::Tuple(pats) => {
            for p in pats {
                collect_pattern_identifiers(p, out);
            }
        }
        Pattern::Struct { fields, .. } => {
            for f in fields {
                out.push(f.clone());
            }
        }
    }
}
