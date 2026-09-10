//! Abstract syntax tree for the C-like `.fxc` language.

/// A parsed program is just a list of statements.
pub type Program = Vec<Stmt>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    /// A variable reference, with its byte offset for diagnostics.
    Name(String, usize),
    Pi(usize),
    E(usize),
    /// A `phys.NAME` scientific constant, resolved at parse time.
    Constant(&'static crate::constants::Constant, usize),
    /// `input()`
    Input(usize),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A call to a built-in such as `sqrt(x)`, with its byte offset.
    Call(String, Vec<Expr>, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub init_name: String,
    pub init_value: Expr,
    /// `true` when written as `for (let i = ...)` (a declaration).
    pub is_decl: bool,
    pub cond: Expr,
    pub update_name: String,
    pub update_value: Expr,
    pub body: Vec<Stmt>,
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let name = value;`
    Let {
        name: String,
        value: Expr,
        pos: usize,
    },
    /// `name = value;`
    Assign {
        name: String,
        value: Expr,
        pos: usize,
    },
    /// `print(value);`
    Print(Expr),
    /// An expression evaluated for its side effects (or just discarded).
    ExprStmt(Expr),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For(ForStmt),
    Break,
    Goto(u8, usize),
    Label(u8, usize),
    Block(Vec<Stmt>),
    /// A stray `;`.
    Empty,
}
