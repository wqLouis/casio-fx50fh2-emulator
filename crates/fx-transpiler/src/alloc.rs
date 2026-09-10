//! Map `.fxc` variable names onto the calculator's seven memories.
//!
//! PRGM only has `A B C D X Y M`, so every distinct `.fxc` name is assigned one
//! of those letters in **first-seen order**. The whole program is scanned up
//! front (in source order) so the allocation is deterministic and independent
//! of the order in which the emitter happens to render derived expressions.
//!
//! Names are never reused: once a name owns a letter it keeps it for the whole
//! program. This is always sound (no scope analysis needed) at the cost of
//! rejecting programs that use more than seven distinct names.

use crate::ast::{Expr, Program, Stmt};
use crate::error::TranspileError;

/// The seven assignable calculator memories, in allocation order.
pub const VARIABLES: [char; 7] = ['A', 'B', 'C', 'D', 'X', 'Y', 'M'];

/// A resolved name-to-memory table.
#[derive(Debug, Clone)]
pub struct Allocator {
    names: Vec<(String, char)>,
}

impl Allocator {
    /// Scan `program` and assign every distinct name a memory.
    ///
    /// Returns an error pointing at the eighth distinct name when the program
    /// needs more variables than the calculator has.
    pub fn collect(program: &Program, source: &str) -> Result<Self, TranspileError> {
        let mut allocator = Allocator { names: Vec::new() };
        allocator.scan_stmts(program, source)?;
        Ok(allocator)
    }

    /// The memory assigned to `name`, if it was seen during the scan.
    pub fn lookup(&self, name: &str) -> Option<char> {
        self.names
            .iter()
            .find(|(known, _)| known == name)
            .map(|(_, letter)| *letter)
    }

    /// Number of distinct names assigned so far.
    #[allow(dead_code)] // used by the unit tests and kept for introspection
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// True when no names have been assigned.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    fn register(&mut self, name: &str, pos: usize, source: &str) -> Result<(), TranspileError> {
        if self.lookup(name).is_some() {
            return Ok(());
        }
        if self.names.len() >= VARIABLES.len() {
            return Err(TranspileError::at(
                source,
                format!(
                    "too many variables: PRGM only has {} memories ({}), but `{}` is the {}th distinct name",
                    VARIABLES.len(),
                    VARIABLES
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                    name,
                    self.names.len() + 1,
                ),
                pos,
            ));
        }
        self.names
            .push((name.to_string(), VARIABLES[self.names.len()]));
        Ok(())
    }

    fn scan_stmts(&mut self, stmts: &[Stmt], source: &str) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.scan_stmt(stmt, source)?;
        }
        Ok(())
    }

    fn scan_stmt(&mut self, stmt: &Stmt, source: &str) -> Result<(), TranspileError> {
        match stmt {
            // `let x = ...` and `x = ...` both declare/own `x`.
            Stmt::Let { name, value, pos } | Stmt::Assign { name, value, pos } => {
                self.register(name, *pos, source)?;
                self.scan_expr(value, source)?;
            }
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.scan_expr(expr, source)?,
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.scan_expr(cond, source)?;
                self.scan_stmts(then_body, source)?;
                self.scan_stmts(else_body, source)?;
            }
            Stmt::While { cond, body } => {
                self.scan_expr(cond, source)?;
                self.scan_stmts(body, source)?;
            }
            Stmt::For(for_stmt) => {
                self.register(&for_stmt.init_name, for_stmt.pos, source)?;
                self.scan_expr(&for_stmt.init_value, source)?;
                self.scan_expr(&for_stmt.cond, source)?;
                self.register(&for_stmt.update_name, for_stmt.pos, source)?;
                self.scan_expr(&for_stmt.update_value, source)?;
                self.scan_stmts(&for_stmt.body, source)?;
            }
            Stmt::Block(stmts) => self.scan_stmts(stmts, source)?,
            Stmt::Break | Stmt::Goto(..) | Stmt::Label(..) | Stmt::Empty => {}
        }
        Ok(())
    }

    fn scan_expr(&mut self, expr: &Expr, source: &str) -> Result<(), TranspileError> {
        match expr {
            Expr::Name(name, pos) => self.register(name, *pos, source)?,
            Expr::Unary(_, inner) => self.scan_expr(inner, source)?,
            Expr::Binary(_, left, right) => {
                self.scan_expr(left, source)?;
                self.scan_expr(right, source)?;
            }
            Expr::Call(_, args, _) => {
                for arg in args {
                    self.scan_expr(arg, source)?;
                }
            }
            Expr::Number(_) | Expr::Pi(_) | Expr::E(_) | Expr::Constant(..) | Expr::Input(_) => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn alloc(source: &str) -> Result<Allocator, TranspileError> {
        let tokens = lex(source)?;
        let program = parse(&tokens, source)?;
        Allocator::collect(&program, source)
    }

    #[test]
    fn assigns_in_first_seen_order() {
        let a = alloc("let b = 1; let a = 2; print(a + b);").unwrap();
        assert_eq!(a.lookup("b"), Some('A'));
        assert_eq!(a.lookup("a"), Some('B'));
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn reuses_the_same_letter_for_a_repeated_name() {
        let a = alloc("let a = 1; a = a + 1; print(a);").unwrap();
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn seventh_name_is_the_last_one_allowed() {
        let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1;";
        let a = alloc(source).unwrap();
        assert_eq!(a.lookup("m"), Some('M'));
        assert_eq!(a.len(), 7);
    }

    #[test]
    fn eighth_name_errors_at_its_position() {
        let source = "let a=1; let b=1; let c=1; let d=1; let x=1; let y=1; let m=1; let z=1;";
        let err = alloc(source).unwrap_err();
        assert!(
            err.message.contains("too many variables"),
            "{}",
            err.message
        );
        assert_eq!((err.line, err.column), (1, 68));
    }
}
