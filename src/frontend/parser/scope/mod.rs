//we will move parsing the scopes here
// fn - function
// block - block
// class - class
// struct - struct
// custom - custom

use crate::frontend::lexer::token::TokenKind;
use crate::frontend::parser::ast::*;
use crate::frontend::parser::parser::Parser;
pub mod block;
pub mod blueprint;
pub mod class;
pub mod directive_scope;
pub mod enum_decl;
pub mod function_decl;
pub mod machine;
pub mod struct_decl;

impl Parser {
    pub fn get_handle_type(&self, t: TokenKind) -> HandleMethods {
        let name = HandleMethods::from_str(t.as_str());
        return name;
    }
    pub fn is_valid_handle(&mut self, handles: Vec<HandleMethods>, t: TokenKind) -> bool {
        let t_h = self.get_handle_type(t);
        for h in handles {
            if h == t_h {
                return true;
            }
        }
        false
    }

    pub fn is_valid_setting(&mut self, t: TokenKind) -> bool {
        if let TokenKind::LabelName(_) = t {
            return true;
        }
        let mut all_settings: Vec<Setting> = Vec::new();
        all_settings.push(Setting::Constructor);
        all_settings.push(Setting::Public);
        all_settings.push(Setting::Private);
        all_settings.push(Setting::Static);
        all_settings.push(Setting::Extends);
        all_settings.push(Setting::Handle);
        all_settings.push(Setting::Data);
        all_settings.push(Setting::Label);
        let t_s = Setting::from_token(t);
        for s in all_settings {
            if s == t_s {
                return true;
            }
        }
        false
    }
    //====================================================================
}
