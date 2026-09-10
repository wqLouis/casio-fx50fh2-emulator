//! Map `.fxc` variable names onto the calculator's seven memories.
//!
//! PRGM has exactly seven assignable memories — `A B C D X Y M` — and no more,
//! so this pass decides what each name uses. It is the transpiler's equivalent
//! of a register allocator, and the limit is a fact about the hardware.
//!
//! ## Why names are never given a shared memory
//!
//! A normal compiler reuses a register once a variable is dead. That reasoning
//! does not transfer here, because a **memory's final value is part of the
//! program's observable result**: PRGM programs leave their answer in a
//! memory, a later program can read it, and the user can read it from the
//! keypad. `let a = 1; let b = 2;` sets two memories; collapsing them onto one
//! would silently drop `a`. Since no name is ever provably unobservable, no
//! name is ever provably dead, so every name keeps its own memory for the whole
//! program.
//!
//! ## What actually relieves the pressure
//!
//! * **`const` uses no memory at all.** A `const` is a compile-time value the
//!   emitter inlines at every use, so it never reaches this module. Programs
//!   that mostly compute with fixed values therefore need very few memories.
//! * **`#data` likewise.** A value read from JSON at transpile time is a
//!   literal in the output, not a variable.
//! * **`#reg name = M` pins a name deliberately**, so the calculator side can
//!   match memories the user already relies on (or keep `M` for the
//!   independent memory). Pinned memories are reserved for the whole program
//!   and never handed to another name.
//! * **`fx50 regs`** reports the plan and says how many of the seven are left.
//!
//! When a program genuinely needs more than seven mutable variables at once,
//! the honest answer is that it does not fit; the error names the memories in
//! use and points at `const` and `#reg`.

use std::collections::BTreeSet;

use crate::ast::{Expr, Program, Stmt};
use crate::data::Data;
use crate::error::TranspileError;

/// The seven assignable calculator memories.
pub const VARIABLES: [char; 7] = ['A', 'B', 'C', 'D', 'X', 'Y', 'M'];

/// Where each variable ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    /// Variable name and the memory it uses, in first-seen order.
    pub entries: Vec<(String, char)>,
    /// Memories deliberately reserved with `#reg`, in source order.
    pub pinned: Vec<(String, char)>,
}

impl Allocation {
    /// How many of the seven memories the program uses.
    pub fn used(&self) -> usize {
        let memories: BTreeSet<char> = self.entries.iter().map(|(_, memory)| *memory).collect();
        memories.len()
    }

    /// The memories left over, in calculator order.
    pub fn free(&self) -> Vec<char> {
        let memories: BTreeSet<char> = self.entries.iter().map(|(_, memory)| *memory).collect();
        VARIABLES
            .iter()
            .copied()
            .filter(|memory| !memories.contains(memory))
            .collect()
    }
}

/// A resolved name-to-memory table.
#[derive(Debug, Clone)]
pub struct Allocator {
    names: Vec<(String, char)>,
    pinned: Vec<(String, char)>,
}

impl Allocator {
    /// Scan `program` and assign every name a memory.
    ///
    /// `pins` are the `#reg NAME = M` directives, and `data` names the
    /// compile-time tables declared with `#data` (which are never memories).
    pub fn collect(
        program: &Program,
        source: &str,
        pins: &[(String, char)],
        data: &Data,
    ) -> Result<Self, TranspileError> {
        let mut scanner = Scanner {
            source,
            data,
            consts: BTreeSet::new(),
            order: Vec::new(),
        };
        collect_consts(program, &mut scanner.consts);
        scanner.stmts(program)?;
        scanner.finish(pins)
    }

    /// The memory assigned to `name`, if it was seen during the scan.
    pub fn lookup(&self, name: &str) -> Option<char> {
        self.names
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, memory)| *memory)
    }

    /// Number of distinct names assigned.
    #[allow(dead_code)] // exercised by the unit tests
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// True when no names have been assigned.
    #[allow(dead_code)] // exercised by the unit tests
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The full allocation, for reporting.
    pub fn allocation(&self) -> Allocation {
        Allocation {
            entries: self.names.clone(),
            pinned: self.pinned.clone(),
        }
    }
}

struct Scanner<'a> {
    source: &'a str,
    data: &'a Data,
    /// Names declared with `const`; they consume no memory.
    consts: BTreeSet<String>,
    /// Variable names in first-seen order, with the offset they appeared at.
    order: Vec<(String, usize)>,
}

impl Scanner<'_> {
    /// Note that `name` appears, if it is a variable at all.
    fn touch(&mut self, name: &str, pos: usize) {
        if self.consts.contains(name) || self.data.contains(name) {
            return;
        }
        if !self.order.iter().any(|(known, _)| known == name) {
            self.order.push((name.to_string(), pos));
        }
    }

    /// Declare a name, rejecting `let` of a compile-time name.
    fn declare(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        if self.consts.contains(name) {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is a `const` and cannot be assigned; rename one of them"),
                pos,
            ));
        }
        if self.data.contains(name) {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is a compile-time data table and cannot be a variable"),
                pos,
            ));
        }
        self.touch(name, pos);
        Ok(())
    }

    fn stmts(&mut self, stmts: &[Stmt]) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), TranspileError> {
        match stmt {
            Stmt::Let { name, value, pos } | Stmt::Assign { name, value, pos } => {
                self.declare(name, *pos)?;
                self.expr(value);
            }
            // A `const` names a value, not a memory; only its operands matter.
            Stmt::Const { value, .. } => self.expr(value),
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond);
                self.stmts(then_body)?;
                self.stmts(else_body)?;
            }
            Stmt::While { cond, body } => {
                self.expr(cond);
                self.stmts(body)?;
            }
            Stmt::For(for_stmt) => {
                self.declare(&for_stmt.init_name, for_stmt.pos)?;
                self.expr(&for_stmt.init_value);
                self.expr(&for_stmt.cond);
                self.stmts(&for_stmt.body)?;
                self.declare(&for_stmt.update_name, for_stmt.pos)?;
                self.expr(&for_stmt.update_value);
            }
            Stmt::Block(stmts) => self.stmts(stmts)?,
            Stmt::Break | Stmt::Goto(..) | Stmt::Label(..) | Stmt::Empty => {}
        }
        Ok(())
    }

    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Name(name, pos) => self.touch(name, *pos),
            // A data path is a compile-time number, not a variable.
            Expr::Data { .. } => {}
            Expr::Unary(_, inner) => self.expr(inner),
            Expr::Binary(_, left, right) => {
                self.expr(left);
                self.expr(right);
            }
            Expr::Call(_, args, _) => {
                for arg in args {
                    self.expr(arg);
                }
            }
            Expr::Number(_) | Expr::Pi(_) | Expr::E(_) | Expr::Constant(..) | Expr::Input(_) => {}
        }
    }

    /// Assign memories, honouring `#reg` pins.
    fn finish(self, pins: &[(String, char)]) -> Result<Allocator, TranspileError> {
        // Validate the pins first, so a bad `#reg` is reported as such rather
        // than as an allocation failure.
        let mut pinned: Vec<(String, char)> = Vec::new();
        for (name, memory) in pins {
            if !VARIABLES.contains(memory) {
                return Err(TranspileError::at(
                    self.source,
                    format!(
                        "`{memory}` is not a memory; the calculator has {}",
                        memory_list()
                    ),
                    0,
                ));
            }
            if self.consts.contains(name) {
                return Err(TranspileError::at(
                    self.source,
                    format!(
                        "`#reg {name} = {memory}`: `{name}` is a `const`, which uses no memory"
                    ),
                    0,
                ));
            }
            if self.data.contains(name) {
                return Err(TranspileError::at(
                    self.source,
                    format!(
                        "`#reg {name} = {memory}`: `{name}` is a compile-time data table, which \
                         uses no memory"
                    ),
                    0,
                ));
            }
            if let Some((_, other)) = pinned.iter().find(|(_, m)| m == memory) {
                return Err(TranspileError::at(
                    self.source,
                    format!("`{memory}` is pinned to both `{other}` and `{name}`"),
                    0,
                ));
            }
            if !self.order.iter().any(|(known, _)| known == name) {
                return Err(TranspileError::at(
                    self.source,
                    format!("`#reg {name} = {memory}`: `{name}` is never used"),
                    0,
                ));
            }
            pinned.push((name.clone(), *memory));
        }

        let pinned_memories: BTreeSet<char> = pinned.iter().map(|(_, memory)| *memory).collect();
        let mut pool: Vec<char> = VARIABLES
            .iter()
            .copied()
            .filter(|memory| !pinned_memories.contains(memory))
            .collect();
        pool.reverse(); // pop() from the end yields A B C D X Y M

        let mut names: Vec<(String, char)> = Vec::new();
        for (name, pos) in &self.order {
            let memory = match pinned.iter().find(|(known, _)| known == name) {
                Some((_, memory)) => *memory,
                None => match pool.pop() {
                    Some(memory) => memory,
                    None => {
                        let holders: Vec<String> = names
                            .iter()
                            .map(|(name, memory)| format!("{memory} (`{name}`)"))
                            .collect();
                        return Err(TranspileError::at(
                            self.source,
                            format!(
                                "no free memory for `{name}`: all of {} are taken by {}. Use \
                                 `const` for fixed values, or `#reg` to pin a memory \
                                 deliberately — `fx50 regs` shows the plan",
                                memory_list(),
                                holders.join(", "),
                            ),
                            *pos,
                        ));
                    }
                },
            };
            names.push((name.clone(), memory));
        }

        Ok(Allocator { names, pinned })
    }
}

/// Collect every `const` name, so the scanner knows they consume no memory.
///
/// Only statements can declare a `const`, so expressions need not be walked.
fn collect_consts(stmts: &[Stmt], out: &mut BTreeSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Const { name, .. } => {
                out.insert(name.clone());
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_consts(then_body, out);
                collect_consts(else_body, out);
            }
            Stmt::While { body, .. } => collect_consts(body, out),
            Stmt::For(for_stmt) => collect_consts(&for_stmt.body, out),
            Stmt::Block(stmts) => collect_consts(stmts, out),
            Stmt::Let { .. }
            | Stmt::Assign { .. }
            | Stmt::Print(_)
            | Stmt::ExprStmt(_)
            | Stmt::Break
            | Stmt::Goto(..)
            | Stmt::Label(..)
            | Stmt::Empty => {}
        }
    }
}

/// The `const` names a program declares, sorted, for reports.
pub fn const_names(program: &Program) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_consts(program, &mut names);
    names.into_iter().collect()
}

/// `A B C D X Y M`, for messages.
fn memory_list() -> String {
    VARIABLES
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn alloc(source: &str) -> Result<Allocator, TranspileError> {
        alloc_pinned(source, &[])
    }

    fn alloc_pinned(source: &str, pins: &[(String, char)]) -> Result<Allocator, TranspileError> {
        let tokens = lex(source)?;
        let program = parse(&tokens, source)?;
        Allocator::collect(&program, source, pins, &Data::default())
    }

    #[test]
    fn assigns_in_first_seen_order() {
        let a = alloc("let b = 1; let a = 2; print(a + b);").unwrap();
        assert_eq!(a.lookup("b"), Some('A'));
        assert_eq!(a.lookup("a"), Some('B'));
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn reuses_the_same_memory_for_a_repeated_name() {
        let a = alloc("let a = 1; a = a + 1; print(a);").unwrap();
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn two_names_never_share_even_when_one_is_never_read() {
        // The memories' final values are observable, so `a` must keep its own.
        let a = alloc("let a = 1; let b = 2;").unwrap();
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.lookup("b"), Some('B'));
        assert_eq!(a.allocation().used(), 2);
    }

    #[test]
    fn seventh_name_is_the_last_one_allowed() {
        let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1;";
        let a = alloc(source).unwrap();
        assert_eq!(a.lookup("m"), Some('M'));
        assert_eq!(a.allocation().used(), 7);
        assert!(a.allocation().free().is_empty());
    }

    #[test]
    fn eighth_name_errors_with_the_holders() {
        let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1; let z=1;";
        let err = alloc(source).unwrap_err();
        assert!(
            err.message.contains("no free memory for `z`"),
            "{}",
            err.message
        );
        assert!(err.message.contains("A (`a`)"), "{}", err.message);
        assert_eq!((err.line, err.column), (1, 68));
    }

    #[test]
    fn free_reports_what_is_left() {
        let a = alloc("let p = 1; let q = 2; print(p + q);").unwrap();
        assert_eq!(a.allocation().free(), vec!['C', 'D', 'X', 'Y', 'M']);
    }

    #[test]
    fn consts_and_data_use_no_memory() {
        let source = "#data tbl = 3;\nconst k = 2;\nlet a = k + tbl; print(a);";
        let expanded = crate::include::expand(source, None, std::path::Path::new(".")).unwrap();
        let (text, data) = crate::data::extract(&expanded, std::path::Path::new(".")).unwrap();
        let tokens = lex(&text).unwrap();
        let program = parse(&tokens, &text).unwrap();
        let a = Allocator::collect(&program, &text, &[], &data).unwrap();
        assert_eq!(a.lookup("k"), None);
        assert_eq!(a.lookup("tbl"), None);
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.allocation().used(), 1);
    }

    #[test]
    fn pins_are_honoured() {
        let pins = [("total".to_string(), 'M')];
        let a = alloc_pinned("let total = 1; print(total);", &pins).unwrap();
        assert_eq!(a.lookup("total"), Some('M'));
        assert_eq!(a.allocation().pinned, pins.to_vec());
    }

    #[test]
    fn pinned_memory_is_not_given_to_others() {
        let pins = [("p".to_string(), 'A')];
        let a = alloc_pinned("let p = 1; let q = 2; print(p + q);", &pins).unwrap();
        assert_eq!(a.lookup("p"), Some('A'));
        assert_eq!(
            a.lookup("q"),
            Some('B'),
            "the next unpinned name skips the pinned memory"
        );
    }

    #[test]
    fn rejects_bad_pins() {
        let err = alloc_pinned("let p = 1; print(p);", &[("p".into(), 'Z')]).unwrap_err();
        assert!(err.message.contains("is not a memory"), "{err}");

        let err = alloc_pinned("let p = 1; print(p);", &[("nope".into(), 'A')]).unwrap_err();
        assert!(err.message.contains("never used"), "{err}");

        let err = alloc_pinned(
            "let p = 1; let q = 2; print(p + q);",
            &[("p".into(), 'A'), ("q".into(), 'A')],
        )
        .unwrap_err();
        assert!(err.message.contains("pinned to both"), "{err}");

        let err = alloc_pinned("const k = 1; print(k);", &[("k".into(), 'A')]).unwrap_err();
        assert!(err.message.contains("`const`"), "{err}");
    }

    #[test]
    fn rejects_let_of_a_compile_time_name() {
        let source = "const k = 1; let k = 2; print(k);";
        let tokens = lex(source).unwrap();
        let program = parse(&tokens, source).unwrap();
        let err = Allocator::collect(&program, source, &[], &Data::default()).unwrap_err();
        assert!(err.message.contains("is a `const`"), "{err}");
    }
}
