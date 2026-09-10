//! Map `.fxc` variable names onto the calculator's seven memories.
//!
//! PRGM has exactly seven assignable memories — `A B C D X Y M` — and no more,
//! so this pass decides what each name uses. It keeps a **register table**
//! (each memory, and the variable currently occupying it) while walking the
//! program in order, which is what lets it detect `free` mistakes at transpile
//! time rather than leaving them for the calculator.
//!
//! ## How a program fits
//!
//! * **`const` and `#data` use no memory at all.** Their values are inlined, so
//!   they never reach this module.
//! * **`free name;` releases a memory** so a later variable can use it. The
//!   programmer states when a value stops being needed — the transpiler cannot
//!   work it out, because a memory's final value is observable: PRGM leaves its
//!   answer in one, a later program or the user can read it. So nothing is
//!   released implicitly, and `let a = 1; let b = 2;` keeps two memories.
//!
//! With no `free` anywhere, every name holds its memory for the whole program
//! and a program needs at most seven names. Each `free` gives one memory back.
//!
//! ## Errors
//!
//! The register table makes the classic mistakes detectable before anything is
//! emitted:
//!
//! * **Double free** — `free a; free a;`
//! * **Use after free** — `free a; print(a);`, including using the name again
//!   as an assignment target; `free` ends the name's life, so a fresh name (or a
//!   second `let a = ...`, which re-declares it) is required.
//! * **Freeing something that holds no memory** — a `const`, a `#data` table,
//!   or a name that was never declared.
//! * **Running out of memories** — reported with the registers in use.
//!
//! `goto`/`label` are the one construct that makes the walk unsound: a jump can
//! re-enter a region whose registers have since been released and re-used, so a
//! program that contains both a jump and a `free` is rejected rather than
//! quietly miscompiled. Programs with jumps but no `free` are unaffected, since
//! nothing is ever re-used.

use std::collections::BTreeSet;

use crate::ast::{Expr, Program, Stmt};
use crate::data::Data;
use crate::error::TranspileError;

/// The seven assignable calculator memories.
pub const VARIABLES: [char; 7] = ['A', 'B', 'C', 'D', 'X', 'Y', 'M'];

/// Where each variable ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    /// Variable name and the memory it uses, in declaration order.
    pub entries: Vec<(String, char)>,
    /// Each memory's occupants over time, in calculator order. A memory that
    /// held more than one name was released with `free` and re-used.
    pub registers: Vec<(char, Vec<String>)>,
    /// Names released with `free`, in source order.
    pub freed: Vec<String>,
}

impl Allocation {
    /// How many of the seven memories the program uses.
    pub fn used(&self) -> usize {
        self.registers
            .iter()
            .filter(|(_, names)| !names.is_empty())
            .count()
    }

    /// The memories the program never touches, in calculator order.
    pub fn free(&self) -> Vec<char> {
        self.registers
            .iter()
            .filter(|(_, names)| names.is_empty())
            .map(|(memory, _)| *memory)
            .collect()
    }

    /// Whether any memory was released and given to another variable.
    pub fn reuses(&self) -> impl Iterator<Item = &(char, Vec<String>)> {
        self.registers.iter().filter(|(_, names)| names.len() > 1)
    }
}

/// A resolved name-to-memory table.
#[derive(Debug, Clone)]
pub struct Allocator {
    names: Vec<(String, char)>,
    allocation: Allocation,
}

impl Allocator {
    /// Scan `program` and assign every name a memory.
    ///
    /// `data` names the compile-time tables declared with `#data`, which are
    /// never memories.
    pub fn collect(program: &Program, source: &str, data: &Data) -> Result<Self, TranspileError> {
        let mut scanner = Scanner {
            source,
            data,
            consts: BTreeSet::new(),
            vars: Vec::new(),
            occupants: [None; VARIABLES.len()],
            freed: Vec::new(),
            first_release: None,
            has_jump: false,
        };
        collect_consts(program, &mut scanner.consts);
        scanner.stmts(program)?;
        scanner.finish()
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
        self.allocation.clone()
    }
}

/// One variable, and the memory it holds while it is live.
struct Var {
    name: String,
    /// Index into [`VARIABLES`].
    register: usize,
    /// Cleared by `free`.
    live: bool,
}

struct Scanner<'a> {
    source: &'a str,
    data: &'a Data,
    /// Names declared with `const`; they consume no memory.
    consts: BTreeSet<String>,
    /// Variables in declaration order.
    vars: Vec<Var>,
    /// The register table: which variable occupies each memory, if any.
    occupants: [Option<usize>; VARIABLES.len()],
    /// Names released with `free`, in source order.
    freed: Vec<String>,
    /// Offset of the first `free`, used to reject it alongside jumps.
    first_release: Option<usize>,
    has_jump: bool,
}

impl Scanner<'_> {
    fn index_of(&self, name: &str) -> Option<usize> {
        self.vars.iter().position(|var| var.name == name)
    }

    /// Check that `name` is not a compile-time name.
    fn reject_compile_time(&self, name: &str, pos: usize) -> Result<(), TranspileError> {
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
        Ok(())
    }

    /// Resolve a *read* of `name`.
    ///
    /// Reading a `const` or `#data` name is fine — the emitter replaces it with
    /// its value — so only real variables reach the register table.
    fn read(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        if self.consts.contains(name) || self.data.contains(name) {
            return Ok(());
        }
        self.resolve(name, pos)
    }

    /// Resolve an *assignment target* (or the variable of a `for`).
    ///
    /// Assigning to a compile-time name is an error, because there is no
    /// memory to assign to.
    fn assign(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        self.reject_compile_time(name, pos)?;
        self.resolve(name, pos)
    }

    /// Find `name`'s memory, declaring it on first sight.
    ///
    /// A reference to a freed name is an error: `free` ends the name's life, so
    /// there is nothing left to read or write. The name is not revived by a
    /// later `let`, because a second `t` would need a second register while the
    /// emitter resolves names to one — a fresh name keeps that unambiguous.
    fn resolve(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        if let Some(index) = self.index_of(name) {
            if !self.vars[index].live {
                return Err(use_after_free(self.source, name, pos));
            }
            return Ok(());
        }

        // A new name needs a memory. The lowest free one keeps the allocation
        // deterministic and easy to read.
        let register = (0..VARIABLES.len())
            .find(|register| self.occupants[*register].is_none())
            .ok_or_else(|| self.out_of_memory(name, pos))?;
        self.occupants[register] = Some(self.vars.len());
        self.vars.push(Var {
            name: name.to_string(),
            register,
            live: true,
        });
        Ok(())
    }

    /// `free name;`
    fn free(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        if self.consts.contains(name) {
            return Err(TranspileError::at(
                self.source,
                format!("`free {name}`: `{name}` is a `const`, which uses no memory"),
                pos,
            ));
        }
        if self.data.contains(name) {
            return Err(TranspileError::at(
                self.source,
                format!(
                    "`free {name}`: `{name}` is a compile-time data table, which uses no memory"
                ),
                pos,
            ));
        }

        let Some(index) = self.index_of(name) else {
            return Err(TranspileError::at(
                self.source,
                format!("`free {name}`: `{name}` is not a variable"),
                pos,
            ));
        };
        if !self.vars[index].live {
            return Err(TranspileError::at(
                self.source,
                format!(
                    "`free {name}`: `{name}` was already freed (double free); its memory has \
                     been given to another variable"
                ),
                pos,
            ));
        }

        self.vars[index].live = false;
        let register = self.vars[index].register;
        self.occupants[register] = None;
        self.freed.push(name.to_string());
        self.first_release.get_or_insert(pos);
        Ok(())
    }

    /// No memory is available for `name`.
    fn out_of_memory(&self, name: &str, pos: usize) -> TranspileError {
        let holders: Vec<String> = self
            .vars
            .iter()
            .filter(|var| var.live)
            .map(|var| format!("{} (`{}`)", VARIABLES[var.register], var.name))
            .collect();
        TranspileError::at(
            self.source,
            format!(
                "no free memory for `{name}`: all of {} are in use by {}. Use `const` for \
                 fixed values, or `free` a variable you no longer need — `fx50 regs` shows \
                 the plan",
                memory_list(),
                holders.join(", "),
            ),
            pos,
        )
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
                self.assign(name, *pos)?;
                self.expr(value)?;
            }
            Stmt::Const { value, .. } => self.expr(value)?,
            Stmt::Free { name, pos } => self.free(name, *pos)?,
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr)?,
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond)?;
                self.stmts(then_body)?;
                self.stmts(else_body)?;
            }
            Stmt::While { cond, body } => {
                self.expr(cond)?;
                self.stmts(body)?;
            }
            Stmt::For(for_stmt) => {
                self.assign(&for_stmt.init_name, for_stmt.pos)?;
                self.expr(&for_stmt.init_value)?;
                self.expr(&for_stmt.cond)?;
                self.stmts(&for_stmt.body)?;
                self.assign(&for_stmt.update_name, for_stmt.pos)?;
                self.expr(&for_stmt.update_value)?;
            }
            Stmt::Block(stmts) => self.stmts(stmts)?,
            Stmt::Break => {}
            Stmt::Goto(..) | Stmt::Label(..) => self.has_jump = true,
            Stmt::Empty => {}
        }
        Ok(())
    }

    /// Walk an expression, resolving every name it mentions.
    fn expr(&mut self, expr: &Expr) -> Result<(), TranspileError> {
        match expr {
            Expr::Name(name, pos) => self.read(name, *pos),
            // A data path is a compile-time number, not a variable.
            Expr::Data { .. } => Ok(()),
            Expr::Unary(_, inner) => self.expr(inner),
            Expr::Binary(_, left, right) => {
                self.expr(left)?;
                self.expr(right)
            }
            Expr::Call(_, args, _) => {
                for arg in args {
                    self.expr(arg)?;
                }
                Ok(())
            }
            Expr::Number(_) | Expr::Pi(_) | Expr::E(_) | Expr::Constant(..) | Expr::Input(_) => {
                Ok(())
            }
        }
    }

    fn finish(self) -> Result<Allocator, TranspileError> {
        // A jump makes the walk unsound once anything is released: it can
        // re-enter a region whose memory has since been re-used.
        if let (true, Some(pos)) = (self.has_jump, self.first_release) {
            return Err(TranspileError::at(
                self.source,
                "`free` cannot be used in a program containing `goto`/`label`: a jump can \
                 re-enter code whose memory has since been re-used, so the allocation cannot \
                 be verified",
                pos,
            ));
        }
        let entries: Vec<(String, char)> = self
            .vars
            .iter()
            .map(|var| (var.name.clone(), VARIABLES[var.register]))
            .collect();

        let registers: Vec<(char, Vec<String>)> = (0..VARIABLES.len())
            .map(|register| {
                let names = self
                    .vars
                    .iter()
                    .filter(|var| var.register == register)
                    .map(|var| var.name.clone())
                    .collect();
                (VARIABLES[register], names)
            })
            .collect();

        let allocation = Allocation {
            entries,
            registers,
            freed: self.freed,
        };
        Ok(Allocator {
            names: allocation.entries.clone(),
            allocation,
        })
    }
}

/// A reference to a name whose memory has already been released.
fn use_after_free(source: &str, name: &str, pos: usize) -> TranspileError {
    TranspileError::at(
        source,
        format!(
            "`{name}` was freed and cannot be used again; its memory may now hold another \
             variable. Use a new name for the next value"
        ),
        pos,
    )
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
            | Stmt::Free { .. }
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
        let tokens = lex(source)?;
        let program = parse(&tokens, source)?;
        Allocator::collect(&program, source, &Data::default())
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
    fn two_names_never_share_without_a_free() {
        // The memories' final values are observable, so `a` must keep its own.
        let a = alloc("let a = 1; let b = 2;").unwrap();
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.lookup("b"), Some('B'));
        assert_eq!(a.allocation().used(), 2);
    }

    #[test]
    fn seventh_name_is_the_last_one_allowed_without_a_free() {
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
        assert!(err.message.contains("`free`"), "{}", err.message);
    }

    #[test]
    fn a_free_lets_a_later_variable_reuse_the_memory() {
        let a = alloc("let t = 1; print(t); free t; let u = 2; print(u);").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
        assert_eq!(a.lookup("u"), Some('A'), "u reuses the freed memory");
        assert_eq!(a.allocation().used(), 1);
        assert_eq!(a.allocation().freed, vec!["t"]);
        let allocation = a.allocation();
        let reuses: Vec<&(char, Vec<String>)> = allocation.reuses().collect();
        assert_eq!(reuses.len(), 1);
        assert_eq!(reuses[0].0, 'A');
        assert_eq!(reuses[0].1, vec!["t", "u"]);
    }

    #[test]
    fn freed_memory_is_reused_before_untouched_ones() {
        let a = alloc("let a = 1; free a; let z = 2;").unwrap();
        assert_eq!(a.lookup("z"), Some('A'));
    }

    #[test]
    fn freeing_every_memory_lets_a_ninth_name_in() {
        let source = "
            let a=1; free a; let b=1; free b; let c=1; free c; let d=1; free d;
            let e=1; free e; let f=1; free f; let g=1; free g; let h=1;
        ";
        let a = alloc(source).unwrap();
        assert_eq!(a.len(), 8);
        assert_eq!(
            a.allocation().used(),
            1,
            "each was released before the next"
        );
    }

    #[test]
    fn a_freed_name_may_not_be_revived() {
        // A second `t` would need a second register, but a name resolves to one
        // memory, so reviving is refused rather than silently ambiguous.
        let err = alloc("let t = 1; free t; let t = 2;").unwrap_err();
        assert!(err.message.contains("was freed"), "{}", err.message);
    }

    #[test]
    fn double_free_is_an_error() {
        let err = alloc("let t = 1; free t; free t;").unwrap_err();
        assert!(err.message.contains("double free"), "{}", err.message);
    }

    #[test]
    fn use_after_free_is_an_error_when_reading() {
        let err = alloc("let t = 1; free t; print(t);").unwrap_err();
        assert!(err.message.contains("was freed"), "{}", err.message);
        assert!(err.message.contains("Use a new name"), "{}", err.message);
    }

    #[test]
    fn use_after_free_is_an_error_when_assigning() {
        let err = alloc("let t = 1; free t; t = 2;").unwrap_err();
        assert!(err.message.contains("was freed"), "{}", err.message);
    }

    #[test]
    fn use_after_free_is_an_error_inside_a_later_expression() {
        let err = alloc("let t = 1; free t; print(t + 1);").unwrap_err();
        assert!(err.message.contains("was freed"), "{}", err.message);
    }

    #[test]
    fn freeing_an_unknown_name_is_an_error() {
        let err = alloc("free nope;").unwrap_err();
        assert!(err.message.contains("is not a variable"), "{}", err.message);
    }

    #[test]
    fn freeing_a_compile_time_name_is_an_error() {
        let err = alloc("const k = 1; free k;").unwrap_err();
        assert!(err.message.contains("uses no memory"), "{}", err.message);
    }

    #[test]
    fn a_free_before_the_declaration_is_an_error() {
        let err = alloc("free t; let t = 1;").unwrap_err();
        assert!(err.message.contains("is not a variable"), "{}", err.message);
    }

    #[test]
    fn free_with_a_jump_is_rejected() {
        let err = alloc("let t = 1; free t; goto 1; label 1;").unwrap_err();
        assert!(err.message.contains("`goto`/`label`"), "{}", err.message);
    }

    #[test]
    fn a_jump_without_free_is_still_fine() {
        let a = alloc("let t = 1; goto 1; label 1; print(t);").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
    }

    #[test]
    fn free_inside_a_loop_body_is_allowed() {
        let a = alloc("while (1 < 2) { let t = 1; print(t); free t; }").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
    }

    #[test]
    fn consts_and_data_use_no_memory() {
        let source = "#data tbl = 3;\nconst k = 2;\nlet a = k + tbl; print(a);";
        let expanded = crate::include::expand(source, None, std::path::Path::new(".")).unwrap();
        let (text, data) = crate::data::extract(&expanded, std::path::Path::new(".")).unwrap();
        let tokens = lex(&text).unwrap();
        let program = parse(&tokens, &text).unwrap();
        let a = Allocator::collect(&program, &text, &data).unwrap();
        assert_eq!(a.lookup("k"), None);
        assert_eq!(a.lookup("tbl"), None);
        assert_eq!(a.lookup("a"), Some('A'));
        assert_eq!(a.allocation().used(), 1);
    }
}
