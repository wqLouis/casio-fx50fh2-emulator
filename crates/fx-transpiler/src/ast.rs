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

/// One step of a compile-time data path, as in `config.size` or `weights[0]`.
#[derive(Debug, Clone, PartialEq)]
pub enum Accessor {
    /// `.field`
    Field { name: String, pos: usize },
    /// `[index]`
    Index { index: usize, pos: usize },
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
    /// A compile-time data path such as `config.size` or `weights[0]`, with at
    /// least one accessor. A bare data name parses as [`Expr::Name`] and is
    /// resolved the same way once the emitter knows it is data.
    Data {
        name: String,
        accessors: Vec<Accessor>,
        /// Byte offset of the name.
        pos: usize,
    },
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
    /// `const name = value;` — a compile-time constant, inlined at each use
    /// and never given a register.
    Const {
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
    /// `free name;` — release the memory holding `name`, so a later variable
    /// can use it.
    Free {
        name: String,
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
