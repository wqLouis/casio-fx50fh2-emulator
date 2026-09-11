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
    /// Bitwise/base-n `and`.
    And,
    /// Bitwise/base-n `or`, `xor`, `xnor`.
    Or,
    Xor,
    Xnor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

/// A statistical variable such as `Σx`, `x̄` or `σx`.
///
/// The transpiler keeps its own copy so it stays free of the interpreter
/// dependency; it mirrors `casio_fx50fh2::stats::StatVar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatVar {
    N,
    SumX,
    SumX2,
    SumY,
    SumY2,
    SumXY,
    MeanX,
    MeanY,
    SigmaX,
    SigmaY,
    Sx,
    Sy,
    MinX,
    MaxX,
    MinY,
    MaxY,
    RegA,
    RegB,
    RegR,
}

impl StatVar {
    /// Every statistical variable, in documentation order. Used by the
    /// language server to offer completions without a second list.
    pub const ALL: [StatVar; 19] = [
        StatVar::N,
        StatVar::SumX,
        StatVar::SumX2,
        StatVar::SumY,
        StatVar::SumY2,
        StatVar::SumXY,
        StatVar::MeanX,
        StatVar::MeanY,
        StatVar::SigmaX,
        StatVar::SigmaY,
        StatVar::Sx,
        StatVar::Sy,
        StatVar::MinX,
        StatVar::MaxX,
        StatVar::MinY,
        StatVar::MaxY,
        StatVar::RegA,
        StatVar::RegB,
        StatVar::RegR,
    ];

    /// The display spelling (glyph mode).
    pub fn glyph(self) -> &'static str {
        use StatVar::*;
        match self {
            N => "n",
            SumX => "\u{03a3}x",
            SumX2 => "\u{03a3}x\u{00b2}",
            SumY => "\u{03a3}y",
            SumY2 => "\u{03a3}y\u{00b2}",
            SumXY => "\u{03a3}xy",
            MeanX => "x\u{0304}",
            MeanY => "y\u{0304}",
            SigmaX => "\u{03c3}x",
            SigmaY => "\u{03c3}y",
            Sx => "sx",
            Sy => "sy",
            MinX => "minX",
            MaxX => "maxX",
            MinY => "minY",
            MaxY => "maxY",
            RegA => "regA",
            RegB => "regB",
            RegR => "regR",
        }
    }

    /// The ASCII-alias spelling, which the calculator's lexer also accepts.
    pub fn ascii(self) -> &'static str {
        use StatVar::*;
        match self {
            N => "n",
            SumX => "sumx",
            SumX2 => "sumx2",
            SumY => "sumy",
            SumY2 => "sumy2",
            SumXY => "sumxy",
            MeanX => "meanx",
            MeanY => "meany",
            SigmaX => "sigmax",
            SigmaY => "sigmay",
            Sx => "sx",
            Sy => "sy",
            MinX => "minx",
            MaxX => "maxx",
            MinY => "miny",
            MaxY => "maxy",
            RegA => "rega",
            RegB => "regb",
            RegR => "regr",
        }
    }

    /// Parse the name written after the `stat.` namespace.
    pub fn parse(name: &str) -> Option<StatVar> {
        use StatVar::*;
        Some(match name {
            "n" => N,
            "sumx" => SumX,
            "sumx2" => SumX2,
            "sumy" => SumY,
            "sumy2" => SumY2,
            "sumxy" => SumXY,
            "meanx" => MeanX,
            "meany" => MeanY,
            "sigmax" => SigmaX,
            "sigmay" => SigmaY,
            "sx" => Sx,
            "sy" => Sy,
            "minx" | "minX" => MinX,
            "maxx" | "maxX" => MaxX,
            "miny" | "minY" => MinY,
            "maxy" | "maxY" => MaxY,
            "rega" | "regA" => RegA,
            "regb" | "regB" => RegB,
            "regr" | "regR" => RegR,
            _ => return None,
        })
    }
}

/// The number bases the calculator works in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Dec,
    Hex,
    Bin,
    Oct,
}

impl Base {
    /// The digit suffix used by a PRGM tagged literal.
    pub fn suffix(self) -> char {
        match self {
            Base::Dec => 'd',
            Base::Hex => 'h',
            Base::Bin => 'b',
            Base::Oct => 'o',
        }
    }

    /// The lexer keyword that selects the base (`Hex`, `Bin`, …).
    pub fn keyword(self) -> &'static str {
        match self {
            Base::Dec => "Dec",
            Base::Hex => "Hex",
            Base::Bin => "Bin",
            Base::Oct => "Oct",
        }
    }

    /// Format a non-negative integer as a tagged PRGM literal.
    pub fn tag(self, value: u64) -> String {
        match self {
            Base::Dec => format!("{value}{}", self.suffix()),
            Base::Hex => format!("{value:X}{}", self.suffix()),
            Base::Bin => format!("{value:b}{}", self.suffix()),
            Base::Oct => format!("{value:o}{}", self.suffix()),
        }
    }
}

/// A display, angle, base or complex-format setup command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    Deg,
    Rad,
    Gra,
    Fix(u8),
    Sci(u8),
    Norm(u8),
    Dec,
    Hex,
    Bin,
    Oct,
    /// `▶a+b𝑖`
    Cartesian,
    /// `▶r∠θ`
    Polar,
}

impl Setup {
    /// The PRGM spelling in glyph mode.
    pub fn glyph(self) -> String {
        match self {
            Setup::Deg => "Deg".into(),
            Setup::Rad => "Rad".into(),
            Setup::Gra => "Gra".into(),
            Setup::Fix(n) => format!("Fix {n}"),
            Setup::Sci(n) => format!("Sci {n}"),
            Setup::Norm(n) => format!("Norm {n}"),
            Setup::Dec => "Dec".into(),
            Setup::Hex => "Hex".into(),
            Setup::Bin => "Bin".into(),
            Setup::Oct => "Oct".into(),
            Setup::Cartesian => "\u{25b6}a+b\u{1d456}".into(),
            Setup::Polar => "\u{25b6}r\u{2220}\u{03b8}".into(),
        }
    }

    /// The PRGM spelling in ASCII mode.
    pub fn ascii(self) -> String {
        match self {
            Setup::Cartesian => ">a+bi".into(),
            Setup::Polar => ">rangle".into(),
            other => other.glyph(),
        }
    }

    /// Whether the command is a `Fix`/`Sci`/`Norm` with a digit argument.
    pub fn argument(self) -> Option<u8> {
        match self {
            Setup::Fix(n) | Setup::Sci(n) | Setup::Norm(n) => Some(n),
            _ => None,
        }
    }

    /// The `.fxc` call that produces this setup.
    pub fn call_name(self) -> &'static str {
        match self {
            Setup::Deg => "deg",
            Setup::Rad => "rad",
            Setup::Gra => "gra",
            Setup::Fix(_) => "fix",
            Setup::Sci(_) => "sci",
            Setup::Norm(_) => "norm",
            Setup::Dec => "dec",
            Setup::Hex => "hex",
            Setup::Bin => "bin",
            Setup::Oct => "oct",
            Setup::Cartesian => "to_cartesian",
            Setup::Polar => "to_polar",
        }
    }
}

/// `M+` / `M-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemOp {
    Plus,
    Minus,
}

/// One step of a compile-time data path, as in `config.size` or `weights[0]`.
///
/// The same type also describes an array element reference (`a[0]`): the
/// parser cannot tell the two apart, so the allocator and emitter decide by
/// whether the name is a declared array or a `#data` table.
#[derive(Debug, Clone, PartialEq)]
pub enum Accessor {
    /// `.field`
    Field { name: String, pos: usize },
    /// `[index]`
    Index { index: usize, pos: usize },
    /// `[expr]` where the index is not a literal in the source. A constant
    /// expression is folded to [`Accessor::Index`]; a loop induction variable
    /// is made literal by unrolling the enclosing `for`. Anything left over is
    /// reported when the program is allocated.
    IndexExpr { expr: Expr, pos: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    /// A base-tagged literal, e.g. `0x1F` (emitted as `1Fh`).
    BaseLiteral {
        value: u64,
        base: Base,
        pos: usize,
    },
    /// A variable reference, with its byte offset for diagnostics.
    Name(String, usize),
    Pi(usize),
    E(usize),
    /// The `Ans` answer memory.
    Ans(usize),
    /// A statistical variable, reached through the `stat.` namespace.
    StatVar(StatVar, usize),
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
    /// `let name[size];` or `let name[size] = {…};` — a fixed-size array whose
    /// elements each occupy one memory.
    ///
    /// `values` is empty when the declaration has no initialiser list, in
    /// which case every element starts out undefined. The parser guarantees
    /// `values.len() == size` when `values` is non-empty.
    LetArray {
        name: String,
        size: usize,
        values: Vec<Expr>,
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
    /// `name[index] = value;` — assign to one element of an array.
    ///
    /// `index` must be a compile-time constant: PRGM has no indirect
    /// addressing, so an element is a fixed memory chosen while transpiling.
    AssignElement {
        name: String,
        index: usize,
        value: Expr,
        pos: usize,
    },
    /// `name[expr] = value;` — an element assignment whose index is not yet a
    /// literal. Unrolling turns this into [`Stmt::AssignElement`]; if it cannot,
    /// the index is reported as a compile-time error.
    AssignElementExpr {
        name: String,
        index: Expr,
        value: Expr,
        pos: usize,
    },
    /// `free name;` — release the memory holding `name`, so a later variable
    /// can use it. `unsafe_free name;` does the same without the control-flow
    /// safety check, for programs that contain `goto`/`label`.
    Free {
        name: String,
        pos: usize,
        /// True for `unsafe_free`, which waives the `goto`/`label` check.
        is_unsafe: bool,
    },
    /// `print(value);`
    Print(Expr),
    /// An expression evaluated for its side effects (or just discarded).
    ExprStmt(Expr),
    /// `mplus(value);` / `mminus(value);` — the `M+` / `M-` keys.
    Memory {
        value: Expr,
        op: MemOp,
        pos: usize,
    },
    /// `deg();`, `fix(3);`, `hex();`, `to_polar();`, …
    Setup {
        setup: Setup,
        pos: usize,
    },
    /// `clrmemory();`
    ClrMemory,
    /// `clrstat();`
    ClrStat,
    /// `freqon();` / `freqoff();`
    FreqOn,
    FreqOff,
    /// `dt(x);`, `dt(x, y);`, `dt(x, y, f);` — statistics data entry.
    Data {
        x: Expr,
        y: Option<Expr>,
        freq: Option<Expr>,
        pos: usize,
    },
    /// `cond => stmt;` — the `⇒` key.
    CondJump {
        cond: Expr,
        target: Box<Stmt>,
        pos: usize,
    },
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
