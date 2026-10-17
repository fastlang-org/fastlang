#[derive(Debug, PartialEq, Clone)]
pub enum TokenKind {
    // 1. Data Types
    Int(i128),
    UInt(u128),
    Float(f64),
    String(String),
    UString(String),
    Char(char),
    UChar(u32),
    Identifier(String),
    Bool(bool),
    //we will make Str and Array as a primitive type
    // 2. Keywords
    //Let,      // let   we will remove let to make the syntax more simple
    Const,    // const
    Set,      // set
    If,       // if
    Else,     // else
    Match,    // match
    For,      // for
    In,       // in
    While,    // while
    Do,       // do
    Loop,     // loop
    Break,    // break
    Continue, // continue
    Return,   // return
    Fn,       // fn
    Del,      // del

    Constructor, // constructor
    Extends,     // extends
    Super,       // super

    Import,      // import
    Extern,      // extern
    As,          // as
    Define,      // define
    Abstract,    // abstract
    Virtual,     // virtual
    TypeMachine, // machine
    New,         // new

    This,   // this
    Global, // global

    Try,   // try
    Catch, // catch
    Throw, // throw

    //new meta
    Leave,             // leave
    Yield,             // yield
    Goto,              // goto
    Call,              // call
    Label,             // label
    LabelName(String), // label_name  @string
    TemplateText(String), // Raw template text inside foreign blocks
    // comments
    MultiLineComment,
    InlineComment,

    // 3. Built-in Types

    // Primitives
    TypeInt(u8),   // int, int8, int16, int32, int64, int128
    TypeUInt(u8),  // uint, uint8, uint16, uint32, uint64, uint128, byte
    TypeUSize,     // usize
    TypeISize,     // isize
    TypeFloat(u8), // float, float32, float64, float128
    TypeChar,      // char
    TypeUChar,     // uchar
    TypeStr,       // str
    TypeUStr,      // ustr
    TypeBool,      // bool
    TypeVoid,      // void
    TypeType,      // type
    Undefined,     // undefined
    Using,         // using
    //the mabeuptype is removed and we will add a alternative for it as a ast node if we need it
    // but i don't think so

    // built-in fn
    ToString, // to_string()

    TypeBluePrint, // blueprint  //* with objects
    // blueprint support
    Impl, // impl
    //scopes types
    TypeObject, // object    //* with oop scopes and custom
    TypeStruct, // struct
    TypeBlock,  // block
    TypeMicro,  // micro
    TypeMacro,  // macro
    TypeClass,  // class
    TypeEnum,   // enum
    TypeMethod, // method
    TypeFn,     // Fn
    TypeLambda, // lambda
    TypeFunction, // function
    TypeNumber,   // number, num
    TypeUnknown,  // unknown
    TypeAuto,     // auto
    //scopes fields
    Init,      // init for getting constructor   //* with  oop scopes and custom
    Flag,      // flag      //* with  scope and fn and looped and block and custom
    Static,    // static    //* with  oop scopes and custom
    Public,    // public    //* with  class and struct and  custom and scope
    Private,   // private   //* with  class and struct and  custom and scope
    Handle,    // handle    //* with all meta-block: handle.<flag> -> { ... }
    Share,     // share
    Statement, // statement //* with all meta-block: statement -> { ... }

    // 4. Operators / punctuation
    Assign,   // =
    Arrow,    // ->
    FatArrow, // =>
    Dot,      // .
    Not,      // !
    Plus,     // +
    Minus,    // -
    Multiply, // *
    Divide,   // /

    PlusPlus,    // ++
    MinusMinus,  // --
    PlusAssign,  // +=
    MinusAssign, // -=
    MulAssign,   // *=
    DivAssign,   // /=
    DotDotDot,   // ...  //todo: add it
    DotDot,      //..  //todo : add it
    DashDot,     // -.
    Mod,         // %
    ModAssign,   // %=
    Underscore,  // _

    // logical
    And,  // && or and
    Or,   // || or or
    Pipe, // |

    // Relational
    Eq,        // ==
    NotEq,     // !=
    Greater,   // >
    Less,      // <
    GreaterEq, // >=
    LessEq,    // <=

    Ampersand, // &
    At,        // @

    //not used yet
    Hash,       //#  we will use it in future update for hex and colors
    DollarSign, //$
    Tilde,      //~

    // Symbols
    LParen,      // (
    RParen,      // )
    LBrace,      // {
    RBrace,      // }
    LBracket,    // [
    RBracket,    // ]
    Colon,       // :
    DoubleColon, // ::
    Walrus,      // :=
    Comma,       // ,
    SemiColon,   // ;
    Question,    // ?

    EOF,
    Error(String), // lexical error with a human-readable message; scanning continues
                   // afterward so the parser can still synchronize() and report more errors.
}
impl TokenKind {
    pub fn as_str(&self) -> &str {
        match self {
            TokenKind::Int(_) => "int",
            TokenKind::UInt(_) => "uint",
            TokenKind::Float(_) => "float",
            TokenKind::String(_) => "str",
            TokenKind::UString(_) => "ustr",
            TokenKind::TypeStr => "str",
            TokenKind::TypeUStr => "ustr",
            TokenKind::Char(_) => "char",
            TokenKind::UChar(_) => "uchar",
            TokenKind::Identifier(v) => v,
            TokenKind::Bool(_) => "bool",
            //TokenKind::Let => "let",
            TokenKind::Const => "const",
            TokenKind::Set => "set",
            TokenKind::ToString => "to_string",
            TokenKind::If => "if",
            TokenKind::Else => "else",
            TokenKind::Match => "match",
            TokenKind::Loop => "loop",
            TokenKind::While => "while",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Return => "return",
            TokenKind::Fn => "fn",
            TokenKind::Del => "del",
            TokenKind::Extends => "extends",
            TokenKind::Super => "super",
            TokenKind::Import => "import",
            TokenKind::Extern => "extern",
            TokenKind::As => "as",
            TokenKind::Define => "define",
            TokenKind::Abstract => "abstract",
            TokenKind::Virtual => "virtual",
            TokenKind::TypeMachine => "machine",
            TokenKind::New => "new",
            TokenKind::This => "this",
            TokenKind::Global => "global",
            TokenKind::Try => "try",
            TokenKind::Catch => "catch",
            TokenKind::Throw => "throw",
            TokenKind::Leave => "leave",
            TokenKind::Yield => "yield",
            TokenKind::Goto => "goto",
            TokenKind::Call => "call",
            TokenKind::Label => "label",
            TokenKind::LabelName(v) => v,
            TokenKind::TemplateText(v) => v,
            TokenKind::TypeClass => "class",
            TokenKind::TypeMicro => "micro",
            TokenKind::TypeMacro => "macro",
            TokenKind::TypeEnum => "enum",
            TokenKind::TypeLambda => "lambda",
            TokenKind::Handle => "handle",
            TokenKind::Share => "share",
            TokenKind::Private => "private",
            TokenKind::Public => "public",
            TokenKind::Static => "static",
            TokenKind::Statement => "statement",
            TokenKind::Init => "init",
            TokenKind::TypeBluePrint => "blueprint",
            TokenKind::Constructor => "constructor",
            TokenKind::Flag => "flag",
            TokenKind::TypeType => "type",
            TokenKind::Assign => "=",
            TokenKind::Arrow => "->",
            TokenKind::FatArrow => "=>",
            TokenKind::Dot => ".",
            TokenKind::DashDot => "-.",
            TokenKind::Not => "!",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Multiply => "*",
            TokenKind::Divide => "/",
            TokenKind::PlusAssign => "+=",
            TokenKind::MinusAssign => "-=",
            TokenKind::MulAssign => "*=",
            TokenKind::DivAssign => "/=",
            TokenKind::Mod => "%",
            TokenKind::ModAssign => "%=",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::Eq => "==",
            TokenKind::NotEq => "!=",
            TokenKind::Less => "<",
            TokenKind::Greater => ">",
            TokenKind::GreaterEq => ">=",
            TokenKind::LessEq => "<=",
            TokenKind::And => "and",
            TokenKind::Or => "or",
            TokenKind::Pipe => "|",
            TokenKind::Underscore => "_",
            TokenKind::Walrus => ":=",
            TokenKind::Undefined => "undefined",
            TokenKind::Comma => ",",
            TokenKind::SemiColon => ";",
            TokenKind::Question => "?",
            TokenKind::TypeAuto => "auto",
            TokenKind::Error(v) => v,
            _ => "error",
        }
    }
    /// Returns the source keyword string for type tokens.
    /// Mirrors `as_str` but covers the built-in type variants that the main
    /// `as_str` falls through to the `_ => "error"` arm.
    pub fn type_keyword(&self) -> Option<&str> {
        match self {
            TokenKind::TypeAuto => Some("auto"),
            TokenKind::TypeInt(_) => Some("int"),
            TokenKind::TypeUInt(_) => Some("uint"),
            TokenKind::TypeUSize => Some("usize"),
            TokenKind::TypeISize => Some("isize"),
            TokenKind::TypeFloat(_) => Some("float"),
            TokenKind::TypeType => Some("type"),
            TokenKind::TypeBool => Some("bool"),
            TokenKind::TypeChar => Some("char"),
            TokenKind::TypeUChar => Some("uchar"),
            TokenKind::TypeStr => Some("str"),
            TokenKind::TypeUStr => Some("ustr"),
            TokenKind::TypeVoid => Some("void"),
            TokenKind::TypeMethod => Some("method"),
            TokenKind::TypeFn => Some("Fn"),
            TokenKind::TypeFunction => Some("function"),
            TokenKind::TypeObject => Some("object"),
            TokenKind::TypeNumber => Some("number"),
            TokenKind::Flag => Some("flag"),
            TokenKind::Using => Some("using"),
            _ => None,
        }
    }
}
/// A token plus the source position where it *starts*. Position is captured
/// before any of the token's characters are consumed, so it points at the
/// first character of the lexeme, not the character after it.
#[derive(Debug, PartialEq, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Token { kind, line, column }
    }
}
