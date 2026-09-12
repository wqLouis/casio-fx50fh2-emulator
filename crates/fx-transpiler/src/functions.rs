//! User-defined functions, expanded while transpiling.
//!
//! PRGM has no call instruction, no stack and no indirect addressing, so a
//! `.fxc` function is **inlined at each call site** and never reaches the
//! calculator as a function. That is what makes functions cheap on a machine
//! with 7 memories and 680 bytes of program storage, and it is also what fixes
//! their semantics.
//!
//! ```c
//! fn hypot(a, b) = sqrt(a*a + b*b);         // expression form
//!
//! fn sum_to(n) {                             // procedure form
//!     let total = 0;
//!     for (let i = 1; i <= n; i = i + 1) { total = total + i; }
//!     return total;
//! }
//!
//! fn main() {
//!     print(hypot(3, 4));        // 3×3+4×4 then √(…) — no memory at all
//!     print(sum_to(5));
//! }
//! ```
//!
//! ## Call by name
//!
//! Arguments are **substituted**, not evaluated once. A parameter reference is
//! replaced by the argument expression at every mention, so:
//!
//! * a variable argument is re-read each time — with `fn inc(x) { x = x + 1; }`
//!   the call `inc(a);` becomes `a = a + 1;`, which is pass-by-reference in
//!   effect;
//! * an expression argument is re-evaluated each time, so `twice(ran())` draws
//!   two different numbers;
//! * an argument mentioned `n` times is emitted `n` times, which costs bytes.
//!
//! Assigning to a parameter requires the argument to be a variable or array
//! element, because the assignment is written back through the name. Assigning
//! an expression is an error.
//!
//! ## Locals and hygiene
//!
//! A `let`/`const`/`for` counter inside a function body is a **local**: it is
//! renamed to a private name per call (`sum_to$total$1`) so two calls, or a call
//! next to a caller variable of the same name, cannot collide. Every function
//! except `main` is also **closed**: a name that is neither a parameter nor
//! declared in the body is an error, so a function cannot read or create a
//! global. See [`expand`] for the one exception.
//!
//! ## `main`
//!
//! Every program is a set of functions with a `fn main()` entry point: the top
//! level holds only `fn` definitions and `const` declarations, and `main`'s body
//! becomes the program. Functions that are never called are dropped.
//!
//! `main` is the one body that is *not* closed. It keeps the classic order-free
//! scoping — using a name declares it — so a program's loose statements move
//! into `main` unchanged, and wrapping them emits exactly the PRGM they emitted
//! at the top level. Closing `main` too would have forced every name to be
//! declared before use; a function called from another file is where isolation
//! matters, and those are exactly the functions that are closed.
//!
//! ## What is rejected
//!
//! * Recursion, direct or through other functions: there is no stack.
//! * A `return` that is not the last statement of its body. Branching returns
//!   are written with an output parameter (`fn split(x, hi, lo) { … }`), which
//!   call-by-name makes natural.
//! * Calling a procedure (block body) from a `while`/`for` condition, where
//!   hoisting its statements would move them out of the loop.
//! * An unknown function name, or the wrong number of arguments.
//! * A loose statement at the top level.
//!
//! ## Libraries
//!
//! A file with `fn` definitions but **no `fn main()`** is a library: it exists to
//! be `#include`d by a program. It has nothing to run, so expanding it yields an
//! empty program rather than an error, which is what lets a library file be
//! built, linted and opened in an editor on its own.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Accessor, Expr, FnDef, Program, Stmt};
use crate::builtins;
use crate::data::Data;
use crate::error::TranspileError;

/// How deeply function calls may nest before we assume a cycle slipped through.
const MAX_DEPTH: usize = 64;

/// Expand every user function call, returning a flat program.
///
/// **Every program is a set of functions with a `fn main()` entry point.**
/// Top-level statements are not allowed; only `fn` definitions and top-level
/// `const` declarations may sit beside `main`, so there is no top-level
/// namespace to pollute:
///
/// * `fn main()` is required.
/// * A name a *function* body uses must be a parameter, a local, a top-level
///   `const`/`#data` value, another function, or a built-in. It cannot read or
///   create a global, so including a library cannot change the caller.
/// * `main` keeps the classic order-free rules: it may introduce a name by
///   using it, and its names are allocated like any other statement list.
///
/// Compile-time `const`s and `#data` tables stay available to every function,
/// because they occupy no memory and cannot be assigned.
pub(crate) fn expand(
    program: Program,
    source: &str,
    data: &Data,
) -> Result<Program, TranspileError> {
    let mut defs: BTreeMap<String, FnDef> = BTreeMap::new();
    let mut rest: Vec<Stmt> = Vec::new();

    for stmt in program {
        match stmt {
            Stmt::Function(def) => {
                check_def(&def, source)?;
                if let Some(previous) = defs.get(&def.name) {
                    return Err(TranspileError::at(
                        source,
                        format!("`{}` is already defined", def.name),
                        previous.pos,
                    ));
                }
                defs.insert(def.name.clone(), def);
            }
            other => rest.push(other),
        }
    }

    // A `return` outside a function has no body to leave; report it before the
    // generic top-level rule so the message is specific.
    reject_stray_return(&rest, source)?;

    // The top level holds only definitions and compile-time values. A loose
    // statement would run in the very namespace the functions are kept out of.
    for stmt in &rest {
        if !matches!(stmt, Stmt::Const { .. }) {
            // With nothing defined at all, the writer was starting a program and
            // has simply not reached `main` yet, so name the missing entry point
            // rather than the top-level rule.
            let message = if defs.is_empty() {
                "a program needs an entry point; add `fn main() { … }`"
            } else {
                "only `fn` definitions and `const` declarations may appear at the top level; \
                 move this statement into `fn main()`"
            };
            return Err(TranspileError::at(
                source,
                message,
                stmt_position(stmt).unwrap_or(0),
            ));
        }
    }

    // Names every function may reach without declaring them: compile-time
    // values only.
    let globals: BTreeSet<String> = rest
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Const { name, .. } => Some(name.clone()),
            _ => None,
        })
        .chain(data.names().map(str::to_string))
        .collect();

    // `main` is where a program's loose code lives, so it keeps the classic
    // order-free rules. Every other function is closed.
    //
    // This runs whether or not there is a `main`, so that a **library is checked
    // on its own**. That is most of the value of a library being a file you can
    // build: a typo in it is reported where it is written, rather than only when
    // some other program happens to include it.
    for (name, def) in &defs {
        if name != "main" {
            check_scope(def, &globals, source)?;
        }
    }

    // No `main`: this file is a **library**.
    //
    // A library defines functions — and optionally compile-time `const`/`#data`
    // values — for another program to `#include`. It has nothing to run, so its
    // expansion is empty, and building it is *not* an error. Libraries used to
    // carry their own loose statements ("fragments"); see `crate::include` for
    // why that is gone.
    let Some(main) = defs.get("main") else {
        return Ok(Vec::new());
    };
    if !main.params.is_empty() {
        return Err(TranspileError::at(
            source,
            "`main` takes no parameters",
            main.pos,
        ));
    }

    let entry = match &main.expr {
        Some(expr) => vec![Stmt::ExprStmt(expr.clone())],
        None => main.body.clone(),
    };
    // Check names, arity and cycles against the definitions' own positions,
    // before inlining rewrites them to the call site.
    precheck(&defs, &entry, source)?;

    let mut inliner = Inliner {
        defs: &defs,
        source,
        counter: 0,
        active: Vec::new(),
    };

    // The compile-time declarations keep their order, ahead of `main`.
    let mut out = rest;
    inliner.lower_into(entry, &mut out)?;
    Ok(out)
}

/// The byte offset of a statement, when it carries one.
fn stmt_position(stmt: &Stmt) -> Option<usize> {
    match stmt {
        Stmt::Let { pos, .. }
        | Stmt::LetArray { pos, .. }
        | Stmt::Const { pos, .. }
        | Stmt::Assign { pos, .. }
        | Stmt::AssignElement { pos, .. }
        | Stmt::AssignElementExpr { pos, .. }
        | Stmt::Free { pos, .. }
        | Stmt::Return { pos, .. }
        | Stmt::Memory { pos, .. }
        | Stmt::Setup { pos, .. }
        | Stmt::Data { pos, .. }
        | Stmt::CondJump { pos, .. } => Some(*pos),
        Stmt::Goto(_, pos) | Stmt::Label(_, pos) => Some(*pos),
        Stmt::For(for_stmt) => Some(for_stmt.pos),
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => expr_position(expr),
        Stmt::If { .. }
        | Stmt::While { .. }
        | Stmt::Block(_)
        | Stmt::Break
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Empty
        | Stmt::Function(_) => None,
    }
}

/// The byte offset of an expression, when it carries one.
fn expr_position(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Name(_, pos)
        | Expr::Pi(pos)
        | Expr::E(pos)
        | Expr::Ans(pos)
        | Expr::StatVar(_, pos)
        | Expr::Constant(_, pos)
        | Expr::Input(pos)
        | Expr::Data { pos, .. }
        | Expr::BaseLiteral { pos, .. }
        | Expr::Call(_, _, pos) => Some(*pos),
        Expr::Unary(_, inner) => expr_position(inner),
        Expr::Binary(_, left, _) => expr_position(left),
        Expr::Number(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Validation of the definitions themselves

/// Check one `fn` definition before it is used.
fn check_def(def: &FnDef, source: &str) -> Result<(), TranspileError> {
    if builtins::lookup(&def.name).is_some() {
        return Err(TranspileError::at(
            source,
            format!(
                "`{}` is a built-in function; choose another name for your `fn`",
                def.name
            ),
            def.pos,
        ));
    }
    if def.name == "input" {
        return Err(TranspileError::at(
            source,
            "`input` is a built-in function; choose another name for your `fn`",
            def.pos,
        ));
    }

    let mut seen = BTreeSet::new();
    for param in &def.params {
        if !seen.insert(param.name.clone()) {
            return Err(TranspileError::at(
                source,
                format!(
                    "`{}` is listed twice in the parameters of `{}`",
                    param.name, def.name
                ),
                def.pos,
            ));
        }
    }

    // A `return` may only be the final statement of the body. Anything else
    // would need a jump out of the middle of the function.
    for (index, stmt) in def.body.iter().enumerate() {
        let is_last = index + 1 == def.body.len();
        if !is_last && stmt_contains_return(stmt) {
            return Err(TranspileError::at(
                source,
                format!(
                    "`return` in `{}` must be the last statement; to return an early value, pass \
                     a variable and assign it, as in `fn f(x, out) {{ out = …; }}`",
                    def.name
                ),
                def.pos,
            ));
        }
        if is_last && let Stmt::Return { .. } = stmt {
            // The only legal position.
        }
    }
    if let Some(last) = def.body.last()
        && !matches!(last, Stmt::Return { .. })
    {
        reject_nested_return(last, def, source)?;
    }
    Ok(())
}

/// Whether `stmt` contains a `return` anywhere inside it.
fn stmt_contains_return(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(stmt_contains_return) || else_body.iter().any(stmt_contains_return)
        }
        Stmt::While { body, .. } => body.iter().any(stmt_contains_return),
        Stmt::For(for_stmt) => for_stmt.body.iter().any(stmt_contains_return),
        Stmt::Block(stmts) => stmts.iter().any(stmt_contains_return),
        _ => false,
    }
}

/// A `return` nested in the final statement's control flow is still not in the
/// tail position we support.
fn reject_nested_return(stmt: &Stmt, def: &FnDef, source: &str) -> Result<(), TranspileError> {
    match stmt {
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            if then_body.iter().any(stmt_contains_return)
                || else_body.iter().any(stmt_contains_return)
            {
                return Err(nested_return_error(def, source));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn nested_return_error(def: &FnDef, source: &str) -> TranspileError {
    TranspileError::at(
        source,
        format!(
            "`return` in `{}` must be the last statement of the body; to return different values \
             from different branches, assign to a variable and `return` it, or pass an output \
             parameter",
            def.name
        ),
        def.pos,
    )
}

/// A `return` that is not inside any function body.
fn reject_stray_return(stmts: &[Stmt], source: &str) -> Result<(), TranspileError> {
    for stmt in stmts {
        if let Some(pos) = find_return(stmt) {
            return Err(TranspileError::at(
                source,
                "`return` is only valid inside a `fn` body",
                pos,
            ));
        }
    }
    Ok(())
}

fn find_return(stmt: &Stmt) -> Option<usize> {
    match stmt {
        Stmt::Return { pos, .. } => Some(*pos),
        Stmt::If {
            then_body,
            else_body,
            ..
        } => then_body
            .iter()
            .find_map(find_return)
            .or_else(|| else_body.iter().find_map(find_return)),
        Stmt::While { body, .. } => body.iter().find_map(find_return),
        Stmt::For(for_stmt) => for_stmt.body.iter().find_map(find_return),
        Stmt::Block(stmts) => stmts.iter().find_map(find_return),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Scoping

/// One name a body reads, writes or frees.
struct Use {
    name: String,
    pos: usize,
}

/// Reject a name a function body uses but never declares.
///
/// A function is closed over its parameters, its locals and compile-time
/// values only. Anything else would be an implicit global — a name created in
/// (or read from) the caller's namespace, which is exactly the pollution the
/// entry-point rules exist to prevent.
fn check_scope(
    def: &FnDef,
    globals: &BTreeSet<String>,
    source: &str,
) -> Result<(), TranspileError> {
    let mut declared = collect_declared(&def.body);
    for param in &def.params {
        declared.insert(param.name.clone());
    }

    let mut uses = Vec::new();
    collect_uses_stmts(&def.body, &mut uses);
    if let Some(expr) = &def.expr {
        collect_uses_expr(expr, &mut uses);
    }

    for used in uses {
        if declared.contains(&used.name) || globals.contains(&used.name) {
            continue;
        }
        return Err(TranspileError::at(
            source,
            format!(
                "`{}` is not defined in `{}`; declare it with `let`/`const`, add it as a \
                 parameter, or make it a top-level `const`",
                used.name, def.name
            ),
            used.pos,
        ));
    }
    Ok(())
}

fn collect_uses_stmts(stmts: &[Stmt], out: &mut Vec<Use>) {
    for stmt in stmts {
        collect_uses_stmt(stmt, out);
    }
}

fn collect_uses_stmt(stmt: &Stmt, out: &mut Vec<Use>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Print(value)
        | Stmt::ExprStmt(value) => collect_uses_expr(value, out),
        Stmt::LetArray { values, .. } => {
            for value in values {
                collect_uses_expr(value, out);
            }
        }
        Stmt::Assign { name, value, pos } => {
            out.push(Use {
                name: name.clone(),
                pos: *pos,
            });
            collect_uses_expr(value, out);
        }
        Stmt::AssignElement {
            name, value, pos, ..
        } => {
            out.push(Use {
                name: name.clone(),
                pos: *pos,
            });
            collect_uses_expr(value, out);
        }
        Stmt::AssignElementExpr {
            name,
            index,
            value,
            pos,
        } => {
            out.push(Use {
                name: name.clone(),
                pos: *pos,
            });
            collect_uses_expr(index, out);
            collect_uses_expr(value, out);
        }
        Stmt::Free { name, pos, .. } => out.push(Use {
            name: name.clone(),
            pos: *pos,
        }),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                collect_uses_expr(value, out);
            }
        }
        Stmt::Memory { value, .. } => collect_uses_expr(value, out),
        Stmt::Data { x, y, freq, .. } => {
            collect_uses_expr(x, out);
            if let Some(y) = y {
                collect_uses_expr(y, out);
            }
            if let Some(freq) = freq {
                collect_uses_expr(freq, out);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            collect_uses_expr(cond, out);
            collect_uses_stmt(target, out);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_uses_expr(cond, out);
            collect_uses_stmts(then_body, out);
            collect_uses_stmts(else_body, out);
        }
        Stmt::While { cond, body } => {
            collect_uses_expr(cond, out);
            collect_uses_stmts(body, out);
        }
        Stmt::For(for_stmt) => {
            // A bare `for (i = …)` header mutates an existing variable, so it
            // is a use; `for (let i = …)` declares one.
            if !for_stmt.is_decl {
                out.push(Use {
                    name: for_stmt.init_name.clone(),
                    pos: for_stmt.pos,
                });
            }
            if for_stmt.update_name != for_stmt.init_name && !for_stmt.is_decl {
                out.push(Use {
                    name: for_stmt.update_name.clone(),
                    pos: for_stmt.pos,
                });
            }
            collect_uses_expr(&for_stmt.init_value, out);
            collect_uses_expr(&for_stmt.cond, out);
            collect_uses_expr(&for_stmt.update_value, out);
            collect_uses_stmts(&for_stmt.body, out);
        }
        Stmt::Block(stmts) => collect_uses_stmts(stmts, out),
        Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty
        | Stmt::Function(_) => {}
    }
}

fn collect_uses_expr(expr: &Expr, out: &mut Vec<Use>) {
    match expr {
        Expr::Name(name, pos) => out.push(Use {
            name: name.clone(),
            pos: *pos,
        }),
        // `v[0]` is an array element; `config.field` is a compile-time data
        // path. The name must be a local array, a parameter, or a data table
        // (which `check_scope` allows through `globals`).
        Expr::Data { name, pos, .. } => out.push(Use {
            name: name.clone(),
            pos: *pos,
        }),
        Expr::Unary(_, inner) => collect_uses_expr(inner, out),
        Expr::Binary(_, left, right) => {
            collect_uses_expr(left, out);
            collect_uses_expr(right, out);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                collect_uses_expr(arg, out);
            }
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Pre-validation

/// One call found while scanning a body.
struct CallSite {
    name: String,
    argc: usize,
    pos: usize,
}

/// Check every call in every body against the definitions, and reject cycles,
/// before inlining moves anything to a call site.
fn precheck(
    defs: &BTreeMap<String, FnDef>,
    entry: &[Stmt],
    source: &str,
) -> Result<(), TranspileError> {
    let mut calls_of: BTreeMap<String, Vec<CallSite>> = BTreeMap::new();
    for def in defs.values() {
        let mut calls = Vec::new();
        collect_calls_stmts(&def.body, &mut calls);
        if let Some(expr) = &def.expr {
            collect_calls_expr(expr, &mut calls);
        }
        for call in &calls {
            check_call(defs, call, source)?;
        }
        calls_of.insert(def.name.clone(), calls);
    }

    let mut entry_calls = Vec::new();
    collect_calls_stmts(entry, &mut entry_calls);
    for call in &entry_calls {
        check_call(defs, call, source)?;
    }

    // A call graph with a cycle has no finite inlining.
    let mut state: BTreeMap<String, u8> = BTreeMap::new();
    let mut stack: Vec<String> = Vec::new();
    for name in defs.keys() {
        visit(defs, &calls_of, name, &mut state, &mut stack, source)?;
    }
    Ok(())
}

fn check_call(
    defs: &BTreeMap<String, FnDef>,
    call: &CallSite,
    source: &str,
) -> Result<(), TranspileError> {
    if builtins::lookup(&call.name).is_some() {
        return Ok(());
    }
    let Some(def) = defs.get(&call.name) else {
        return Err(TranspileError::at(
            source,
            format!(
                "unknown function `{}`; define it with `fn {}(…)` or `#include` a file that does",
                call.name, call.name
            ),
            call.pos,
        ));
    };
    if call.argc != def.params.len() {
        return Err(TranspileError::at(
            source,
            format!(
                "`{}` expects {} argument(s), got {}",
                call.name,
                def.params.len(),
                call.argc
            ),
            call.pos,
        ));
    }
    Ok(())
}

fn visit(
    defs: &BTreeMap<String, FnDef>,
    calls_of: &BTreeMap<String, Vec<CallSite>>,
    name: &str,
    state: &mut BTreeMap<String, u8>,
    stack: &mut Vec<String>,
    source: &str,
) -> Result<(), TranspileError> {
    match state.get(name).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => {
            // A back-edge to something on the stack is a cycle.
            let start = stack.iter().position(|entry| entry == name).unwrap_or(0);
            let chain: Vec<String> = stack[start..]
                .iter()
                .cloned()
                .chain(std::iter::once(name.to_string()))
                .collect();
            let pos = defs.get(name).map(|def| def.pos).unwrap_or(0);
            return Err(TranspileError::at(
                source,
                format!(
                    "`{name}` is recursive ({}); PRGM has no call stack, so a function cannot \
                     call itself",
                    chain.join(" \u{2192} ")
                ),
                pos,
            ));
        }
        _ => {}
    }
    state.insert(name.to_string(), 1);
    stack.push(name.to_string());
    if let Some(calls) = calls_of.get(name) {
        for call in calls {
            if defs.contains_key(&call.name) {
                visit(defs, calls_of, &call.name, state, stack, source)?;
            }
        }
    }
    stack.pop();
    state.insert(name.to_string(), 2);
    Ok(())
}

fn collect_calls_stmts(stmts: &[Stmt], out: &mut Vec<CallSite>) {
    for stmt in stmts {
        collect_calls_stmt(stmt, out);
    }
}

fn collect_calls_stmt(stmt: &Stmt, out: &mut Vec<CallSite>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. }
        | Stmt::Print(value)
        | Stmt::ExprStmt(value) => collect_calls_expr(value, out),
        Stmt::LetArray { values, .. } => {
            for value in values {
                collect_calls_expr(value, out);
            }
        }
        Stmt::AssignElementExpr { index, value, .. } => {
            collect_calls_expr(index, out);
            collect_calls_expr(value, out);
        }
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                collect_calls_expr(value, out);
            }
        }
        Stmt::Memory { value, .. } => collect_calls_expr(value, out),
        Stmt::Data { x, y, freq, .. } => {
            collect_calls_expr(x, out);
            if let Some(y) = y {
                collect_calls_expr(y, out);
            }
            if let Some(freq) = freq {
                collect_calls_expr(freq, out);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            collect_calls_expr(cond, out);
            collect_calls_stmt(target, out);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_calls_expr(cond, out);
            collect_calls_stmts(then_body, out);
            collect_calls_stmts(else_body, out);
        }
        Stmt::While { cond, body } => {
            collect_calls_expr(cond, out);
            collect_calls_stmts(body, out);
        }
        Stmt::For(for_stmt) => {
            collect_calls_expr(&for_stmt.init_value, out);
            collect_calls_expr(&for_stmt.cond, out);
            collect_calls_expr(&for_stmt.update_value, out);
            collect_calls_stmts(&for_stmt.body, out);
        }
        Stmt::Block(stmts) => collect_calls_stmts(stmts, out),
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty
        | Stmt::Function(_) => {}
    }
}

fn collect_calls_expr(expr: &Expr, out: &mut Vec<CallSite>) {
    match expr {
        Expr::Call(name, args, pos) => {
            out.push(CallSite {
                name: name.clone(),
                argc: args.len(),
                pos: *pos,
            });
            for arg in args {
                collect_calls_expr(arg, out);
            }
        }
        Expr::Unary(_, inner) => collect_calls_expr(inner, out),
        Expr::Binary(_, left, right) => {
            collect_calls_expr(left, out);
            collect_calls_expr(right, out);
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    collect_calls_expr(expr, out);
                }
            }
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Name(..)
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => {}
    }
}

// ---------------------------------------------------------------------------
// The inliner

struct Inliner<'a> {
    defs: &'a BTreeMap<String, FnDef>,
    source: &'a str,
    /// Fresh-id counter, so every expansion gets its own local and temp names.
    counter: usize,
    /// Functions currently being expanded, for recursion detection.
    active: Vec<String>,
}

impl Inliner<'_> {
    fn error(&self, message: impl Into<String>, pos: usize) -> TranspileError {
        TranspileError::at(self.source, message, pos)
    }

    /// Lower `stmts`, appending the result to `out`.
    fn lower_into(&mut self, stmts: Vec<Stmt>, out: &mut Vec<Stmt>) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.lower_stmt(stmt, out)?;
        }
        Ok(())
    }

    fn lower_stmts(&mut self, stmts: Vec<Stmt>) -> Result<Vec<Stmt>, TranspileError> {
        let mut out = Vec::new();
        self.lower_into(stmts, &mut out)?;
        Ok(out)
    }

    fn lower_stmt(&mut self, stmt: Stmt, out: &mut Vec<Stmt>) -> Result<(), TranspileError> {
        // `f(x);` on its own is a call for effect; inlining it as statements
        // avoids a throwaway temporary for the return value.
        if let Stmt::ExprStmt(Expr::Call(name, args, pos)) = &stmt
            && builtins::lookup(name).is_none()
            && let Some(def) = self.defs.get(name).cloned()
        {
            self.check_arity(&def, args.len(), *pos)?;
            self.inline_call(&def, args.clone(), *pos, out, false)?;
            return Ok(());
        }

        let mut prefix = Vec::new();
        let lowered = match stmt {
            Stmt::Let { name, value, pos } => Stmt::Let {
                name,
                value: self.lower_expr(value, &mut prefix, true)?,
                pos,
            },
            Stmt::LetArray {
                name,
                size,
                values,
                pos,
            } => {
                let mut lowered = Vec::with_capacity(values.len());
                for value in values {
                    lowered.push(self.lower_expr(value, &mut prefix, true)?);
                }
                Stmt::LetArray {
                    name,
                    size,
                    values: lowered,
                    pos,
                }
            }
            Stmt::Const { name, value, pos } => Stmt::Const {
                name,
                value: self.lower_expr(value, &mut prefix, true)?,
                pos,
            },
            Stmt::Assign { name, value, pos } => Stmt::Assign {
                name,
                value: self.lower_expr(value, &mut prefix, true)?,
                pos,
            },
            Stmt::AssignElement {
                name,
                index,
                value,
                pos,
            } => Stmt::AssignElement {
                name,
                index,
                value: self.lower_expr(value, &mut prefix, true)?,
                pos,
            },
            Stmt::AssignElementExpr {
                name,
                index,
                value,
                pos,
            } => Stmt::AssignElementExpr {
                name,
                index: self.lower_expr(index, &mut prefix, true)?,
                value: self.lower_expr(value, &mut prefix, true)?,
                pos,
            },
            Stmt::Free {
                name,
                pos,
                is_unsafe,
            } => Stmt::Free {
                name,
                pos,
                is_unsafe,
            },
            Stmt::Print(expr) => Stmt::Print(self.lower_expr(expr, &mut prefix, true)?),
            Stmt::ExprStmt(expr) => Stmt::ExprStmt(self.lower_expr(expr, &mut prefix, true)?),
            Stmt::Return { value, pos } => Stmt::Return {
                value: match value {
                    Some(expr) => Some(self.lower_expr(expr, &mut prefix, true)?),
                    None => None,
                },
                pos,
            },
            Stmt::Memory { value, op, pos } => Stmt::Memory {
                value: self.lower_expr(value, &mut prefix, true)?,
                op,
                pos,
            },
            Stmt::Data { x, y, freq, pos } => Stmt::Data {
                x: self.lower_expr(x, &mut prefix, true)?,
                y: match y {
                    Some(y) => Some(self.lower_expr(y, &mut prefix, true)?),
                    None => None,
                },
                freq: match freq {
                    Some(f) => Some(self.lower_expr(f, &mut prefix, true)?),
                    None => None,
                },
                pos,
            },
            Stmt::CondJump { cond, target, pos } => Stmt::CondJump {
                cond: self.lower_expr(cond, &mut prefix, true)?,
                target: Box::new(self.lower_stmt_value(*target)?),
                pos,
            },
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                // The condition runs once, so hoisting before the `If` is safe.
                let cond = self.lower_expr(cond, &mut prefix, true)?;
                let then_body = self.lower_stmts(then_body)?;
                let else_body = self.lower_stmts(else_body)?;
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                }
            }
            Stmt::While { cond, body } => {
                // The condition runs every iteration, so a procedure call in it
                // cannot be hoisted out of the loop.
                let cond = self.lower_expr(cond, &mut prefix, false)?;
                let body = self.lower_stmts(body)?;
                Stmt::While { cond, body }
            }
            Stmt::For(mut for_stmt) => {
                // The initialiser runs once (hoistable); the condition and the
                // update run every iteration (not hoistable).
                self.lower_in_place(&mut for_stmt.init_value, &mut prefix, true)?;
                self.lower_in_place(&mut for_stmt.cond, &mut prefix, false)?;
                self.lower_in_place(&mut for_stmt.update_value, &mut prefix, false)?;
                for_stmt.body = self.lower_stmts(for_stmt.body)?;
                Stmt::For(for_stmt)
            }
            Stmt::Block(stmts) => Stmt::Block(self.lower_stmts(stmts)?),
            Stmt::Setup { setup, pos } => Stmt::Setup { setup, pos },
            Stmt::ClrMemory => Stmt::ClrMemory,
            Stmt::ClrStat => Stmt::ClrStat,
            Stmt::FreqOn => Stmt::FreqOn,
            Stmt::FreqOff => Stmt::FreqOff,
            Stmt::Break => Stmt::Break,
            Stmt::Goto(label, pos) => Stmt::Goto(label, pos),
            Stmt::Label(label, pos) => Stmt::Label(label, pos),
            Stmt::Empty => Stmt::Empty,
            Stmt::Function(def) => {
                return Err(self.error(format!("nested `fn {}` is not allowed", def.name), def.pos));
            }
        };
        out.extend(prefix);
        out.push(lowered);
        Ok(())
    }

    /// Lower one statement that is not part of a list (a `=>` target).
    fn lower_stmt_value(&mut self, stmt: Stmt) -> Result<Stmt, TranspileError> {
        let mut out = Vec::new();
        self.lower_stmt(stmt, &mut out)?;
        Ok(match out.len() {
            1 => out.pop().expect("one statement"),
            _ => Stmt::Block(out),
        })
    }

    /// Lower the expression in `slot`, replacing it in place.
    fn lower_in_place(
        &mut self,
        slot: &mut Expr,
        prefix: &mut Vec<Stmt>,
        allow_hoist: bool,
    ) -> Result<(), TranspileError> {
        let expr = std::mem::replace(slot, Expr::Number(0.0));
        *slot = self.lower_expr(expr, prefix, allow_hoist)?;
        Ok(())
    }

    /// Lower `expr`, appending any hoisted statements to `prefix`.
    ///
    /// `allow_hoist` is false inside a loop condition, where moving a
    /// procedure's statements out of the loop would change what runs.
    fn lower_expr(
        &mut self,
        expr: Expr,
        prefix: &mut Vec<Stmt>,
        allow_hoist: bool,
    ) -> Result<Expr, TranspileError> {
        match expr {
            Expr::Call(name, args, pos) => {
                if builtins::lookup(&name).is_some() {
                    let mut lowered = Vec::with_capacity(args.len());
                    for arg in args {
                        lowered.push(self.lower_expr(arg, prefix, allow_hoist)?);
                    }
                    return Ok(Expr::Call(name, lowered, pos));
                }
                let Some(def) = self.defs.get(&name).cloned() else {
                    return Err(self.unknown_function(&name, pos));
                };
                self.check_arity(&def, args.len(), pos)?;
                if def.expr.is_none() && !allow_hoist {
                    return Err(self.error(
                        format!(
                            "`{name}` has a statement body and cannot be called from a loop \
                             condition; use an expression function (`fn {name}(…) = …;`) or assign \
                             it to a variable before the loop"
                        ),
                        pos,
                    ));
                }
                match self.inline_call(&def, args, pos, prefix, true)? {
                    Some(value) => Ok(value),
                    None => Err(self.error(
                        format!("`{name}` does not return a value; call it as a statement"),
                        pos,
                    )),
                }
            }
            Expr::Unary(op, inner) => Ok(Expr::Unary(
                op,
                Box::new(self.lower_expr(*inner, prefix, allow_hoist)?),
            )),
            Expr::Binary(op, left, right) => Ok(Expr::Binary(
                op,
                Box::new(self.lower_expr(*left, prefix, allow_hoist)?),
                Box::new(self.lower_expr(*right, prefix, allow_hoist)?),
            )),
            Expr::Data {
                name,
                accessors,
                pos,
            } => {
                let mut lowered = Vec::with_capacity(accessors.len());
                for accessor in accessors {
                    lowered.push(match accessor {
                        Accessor::IndexExpr { expr, pos } => Accessor::IndexExpr {
                            expr: self.lower_expr(expr, prefix, allow_hoist)?,
                            pos,
                        },
                        other => other,
                    });
                }
                Ok(Expr::Data {
                    name,
                    accessors: lowered,
                    pos,
                })
            }
            other => Ok(other),
        }
    }

    /// Inline one call.
    ///
    /// `out` receives the function body's statements. When `want_value` is set
    /// the returned expression is the call's value; otherwise the value (if
    /// any) is discarded and `None` comes back.
    fn inline_call(
        &mut self,
        def: &FnDef,
        args: Vec<Expr>,
        call_pos: usize,
        out: &mut Vec<Stmt>,
        want_value: bool,
    ) -> Result<Option<Expr>, TranspileError> {
        // Real cycles are rejected by `precheck`; this is only a depth guard.
        // It must not key on the function being already active, because a
        // legitimate `f(f(x))` lowers the argument while `f` is still on the
        // stack and would be mistaken for recursion.
        if self.active.len() >= MAX_DEPTH {
            return Err(self.error(
                format!("function calls nested more than {MAX_DEPTH} deep"),
                def.pos,
            ));
        }

        self.active.push(def.name.clone());
        let result = self.inline_call_inner(def, args, call_pos, out, want_value);
        self.active.pop();
        result
    }

    fn inline_call_inner(
        &mut self,
        def: &FnDef,
        args: Vec<Expr>,
        call_pos: usize,
        out: &mut Vec<Stmt>,
        want_value: bool,
    ) -> Result<Option<Expr>, TranspileError> {
        if let Some(expr) = &def.expr {
            let map = bind_params(def, args);
            let mut body = expr.clone();
            // The allocator resolves names by byte offset, so the body has to
            // speak in the caller's coordinates before its names are looked up.
            reposition_expr(&mut body, call_pos);
            substitute(&mut body, &map, self.source)?;
            let body = self.lower_expr(body, out, true)?;
            return if want_value {
                Ok(Some(body))
            } else {
                out.push(Stmt::ExprStmt(body));
                Ok(None)
            };
        }

        let mut body = def.body.clone();
        reposition_stmts(&mut body, call_pos);
        self.rename_locals(def, &mut body)?;

        let map = bind_params(def, args);
        substitute_stmts(&mut body, &map, self.source)?;

        let mut lowered = self.lower_stmts(body)?;
        let tail = match lowered.last() {
            Some(Stmt::Return { .. }) => lowered.pop(),
            _ => None,
        };

        match tail {
            Some(Stmt::Return { value, pos }) => {
                if want_value {
                    let Some(value) = value else {
                        return Err(self.error(
                            format!(
                                "`{}` returns no value here; use `return expr;` or call it as a \
                                 statement",
                                def.name
                            ),
                            pos,
                        ));
                    };
                    let temp = self.fresh_temp(&def.name);
                    lowered.push(Stmt::Assign {
                        name: temp.clone(),
                        value,
                        pos,
                    });
                    out.extend(lowered);
                    Ok(Some(Expr::Name(temp, pos)))
                } else {
                    if let Some(value) = value {
                        lowered.push(Stmt::ExprStmt(value));
                    }
                    out.extend(lowered);
                    Ok(None)
                }
            }
            _ => {
                if want_value {
                    return Err(self.error(
                        format!(
                            "`{}` never returns a value; add `return expr;` as its last statement, \
                             or call it as a statement",
                            def.name
                        ),
                        def.pos,
                    ));
                }
                out.extend(lowered);
                Ok(None)
            }
        }
    }

    /// Rename a body's locals so two expansions cannot collide.
    ///
    /// Parameters are handled by substitution, so a name that is both a
    /// parameter and a local is rejected rather than silently shadowed.
    fn rename_locals(&mut self, def: &FnDef, body: &mut [Stmt]) -> Result<(), TranspileError> {
        let locals = collect_declared(body);
        for param in &def.params {
            if locals.contains(&param.name) {
                return Err(self.error(
                    format!(
                        "`{}` is both a parameter and a local of `{}`; rename one of them",
                        param.name, def.name
                    ),
                    def.pos,
                ));
            }
        }
        if locals.is_empty() {
            return Ok(());
        }
        self.counter += 1;
        let tag = self.counter;
        let map: BTreeMap<String, String> = locals
            .into_iter()
            .map(|local| {
                let renamed = format!("{}${}${}", def.name, local, tag);
                (local, renamed)
            })
            .collect();
        rename_stmts(body, &map);
        Ok(())
    }

    fn fresh_temp(&mut self, owner: &str) -> String {
        self.counter += 1;
        format!("${owner}${}", self.counter)
    }

    fn check_arity(&self, def: &FnDef, got: usize, pos: usize) -> Result<(), TranspileError> {
        if got == def.params.len() {
            return Ok(());
        }
        Err(self.error(
            format!(
                "`{}` expects {} argument(s), got {got}",
                def.name,
                def.params.len()
            ),
            pos,
        ))
    }

    fn unknown_function(&self, name: &str, pos: usize) -> TranspileError {
        self.error(
            format!(
                "unknown function `{name}`; define it with `fn {name}(…)` or `#include` a file \
                 that does"
            ),
            pos,
        )
    }
}

// ---------------------------------------------------------------------------
// Repositioning

// The allocator binds a name to a byte range in one expanded source, so an
// inlined body has to be expressed in the caller's coordinates before any name
// is resolved. Every position in the body becomes the call site.

fn reposition_stmts(stmts: &mut [Stmt], pos: usize) {
    for stmt in stmts {
        reposition_stmt(stmt, pos);
    }
}

fn reposition_stmt(stmt: &mut Stmt, pos: usize) {
    match stmt {
        Stmt::Let { value, pos: at, .. } => {
            *at = pos;
            reposition_expr(value, pos);
        }
        Stmt::LetArray {
            values, pos: at, ..
        } => {
            *at = pos;
            for value in values {
                reposition_expr(value, pos);
            }
        }
        Stmt::Const { value, pos: at, .. } => {
            *at = pos;
            reposition_expr(value, pos);
        }
        Stmt::Assign { value, pos: at, .. } => {
            *at = pos;
            reposition_expr(value, pos);
        }
        Stmt::AssignElement { value, pos: at, .. } => {
            *at = pos;
            reposition_expr(value, pos);
        }
        Stmt::AssignElementExpr {
            index,
            value,
            pos: at,
            ..
        } => {
            *at = pos;
            reposition_expr(index, pos);
            reposition_expr(value, pos);
        }
        Stmt::Free { pos: at, .. } => *at = pos,
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => reposition_expr(expr, pos),
        Stmt::Return { value, pos: at } => {
            *at = pos;
            if let Some(value) = value {
                reposition_expr(value, pos);
            }
        }
        Stmt::Memory { value, pos: at, .. } => {
            *at = pos;
            reposition_expr(value, pos);
        }
        Stmt::Data {
            x,
            y,
            freq,
            pos: at,
        } => {
            *at = pos;
            reposition_expr(x, pos);
            if let Some(y) = y {
                reposition_expr(y, pos);
            }
            if let Some(freq) = freq {
                reposition_expr(freq, pos);
            }
        }
        Stmt::CondJump {
            cond,
            target,
            pos: at,
        } => {
            *at = pos;
            reposition_expr(cond, pos);
            reposition_stmt(target, pos);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            reposition_expr(cond, pos);
            reposition_stmts(then_body, pos);
            reposition_stmts(else_body, pos);
        }
        Stmt::While { cond, body } => {
            reposition_expr(cond, pos);
            reposition_stmts(body, pos);
        }
        Stmt::For(for_stmt) => {
            for_stmt.pos = pos;
            reposition_expr(&mut for_stmt.init_value, pos);
            reposition_expr(&mut for_stmt.cond, pos);
            reposition_expr(&mut for_stmt.update_value, pos);
            reposition_stmts(&mut for_stmt.body, pos);
        }
        Stmt::Block(stmts) => reposition_stmts(stmts, pos),
        Stmt::Setup { pos: at, .. } => *at = pos,
        Stmt::Goto(_, at) | Stmt::Label(_, at) => *at = pos,
        Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Empty
        | Stmt::Function(_) => {}
    }
}

fn reposition_expr(expr: &mut Expr, pos: usize) {
    match expr {
        Expr::Name(_, at) => *at = pos,
        Expr::BaseLiteral { pos: at, .. } => *at = pos,
        Expr::Pi(at) | Expr::E(at) | Expr::Ans(at) | Expr::Input(at) => *at = pos,
        Expr::StatVar(_, at) => *at = pos,
        Expr::Constant(_, at) => *at = pos,
        Expr::Unary(_, inner) => reposition_expr(inner, pos),
        Expr::Binary(_, left, right) => {
            reposition_expr(left, pos);
            reposition_expr(right, pos);
        }
        Expr::Call(_, args, at) => {
            *at = pos;
            for arg in args {
                reposition_expr(arg, pos);
            }
        }
        Expr::Data {
            accessors, pos: at, ..
        } => {
            *at = pos;
            for accessor in accessors {
                match accessor {
                    Accessor::Field { pos: at, .. }
                    | Accessor::Index { pos: at, .. }
                    | Accessor::IndexExpr { pos: at, .. } => *at = pos,
                }
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    reposition_expr(expr, pos);
                }
            }
        }
        Expr::Number(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Name collection

/// Every name declared as a local by `body`: `let`, `const`, array declarations
/// and `let`-form `for` counters.
fn collect_declared(stmts: &[Stmt]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for stmt in stmts {
        collect_declared_stmt(stmt, &mut names);
    }
    names
}

fn collect_declared_stmt(stmt: &Stmt, names: &mut BTreeSet<String>) {
    match stmt {
        Stmt::Let { name, .. } | Stmt::LetArray { name, .. } | Stmt::Const { name, .. } => {
            names.insert(name.clone());
        }
        Stmt::For(for_stmt) => {
            if for_stmt.is_decl {
                names.insert(for_stmt.init_name.clone());
            }
            collect_declared_stmt_body(&for_stmt.body, names);
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            collect_declared_stmt_body(then_body, names);
            collect_declared_stmt_body(else_body, names);
        }
        Stmt::While { body, .. } => collect_declared_stmt_body(body, names),
        Stmt::CondJump { target, .. } => collect_declared_stmt(target, names),
        Stmt::Block(stmts) => collect_declared_stmt_body(stmts, names),
        _ => {}
    }
}

fn collect_declared_stmt_body(stmts: &[Stmt], names: &mut BTreeSet<String>) {
    for stmt in stmts {
        collect_declared_stmt(stmt, names);
    }
}

// ---------------------------------------------------------------------------
// Renaming

fn rename_stmts(stmts: &mut [Stmt], map: &BTreeMap<String, String>) {
    for stmt in stmts {
        rename_stmt(stmt, map);
    }
}

fn rename_stmt(stmt: &mut Stmt, map: &BTreeMap<String, String>) {
    match stmt {
        Stmt::Let { name, value, .. } => {
            rename_name(name, map);
            rename_expr(value, map);
        }
        Stmt::LetArray { name, values, .. } => {
            rename_name(name, map);
            for value in values {
                rename_expr(value, map);
            }
        }
        Stmt::Const { name, value, .. } => {
            rename_name(name, map);
            rename_expr(value, map);
        }
        Stmt::Assign { name, value, .. } => {
            rename_name(name, map);
            rename_expr(value, map);
        }
        Stmt::AssignElement { name, value, .. } => {
            rename_name(name, map);
            rename_expr(value, map);
        }
        Stmt::AssignElementExpr {
            name, index, value, ..
        } => {
            rename_name(name, map);
            rename_expr(index, map);
            rename_expr(value, map);
        }
        Stmt::Free { name, .. } => rename_name(name, map),
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => rename_expr(expr, map),
        Stmt::Return { value, .. } => {
            if let Some(value) = value {
                rename_expr(value, map);
            }
        }
        Stmt::Memory { value, .. } => rename_expr(value, map),
        Stmt::Data { x, y, freq, .. } => {
            rename_expr(x, map);
            if let Some(y) = y {
                rename_expr(y, map);
            }
            if let Some(freq) = freq {
                rename_expr(freq, map);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            rename_expr(cond, map);
            rename_stmt(target, map);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            rename_expr(cond, map);
            rename_stmts(then_body, map);
            rename_stmts(else_body, map);
        }
        Stmt::While { cond, body } => {
            rename_expr(cond, map);
            rename_stmts(body, map);
        }
        Stmt::For(for_stmt) => {
            rename_name(&mut for_stmt.init_name, map);
            rename_expr(&mut for_stmt.init_value, map);
            rename_expr(&mut for_stmt.cond, map);
            rename_name(&mut for_stmt.update_name, map);
            rename_expr(&mut for_stmt.update_value, map);
            rename_stmts(&mut for_stmt.body, map);
        }
        Stmt::Block(stmts) => rename_stmts(stmts, map),
        Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty
        | Stmt::Function(_) => {}
    }
}

fn rename_name(name: &mut String, map: &BTreeMap<String, String>) {
    if let Some(renamed) = map.get(name) {
        *name = renamed.clone();
    }
}

fn rename_expr(expr: &mut Expr, map: &BTreeMap<String, String>) {
    match expr {
        Expr::Name(name, _) => rename_name(name, map),
        Expr::Unary(_, inner) => rename_expr(inner, map),
        Expr::Binary(_, left, right) => {
            rename_expr(left, map);
            rename_expr(right, map);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                rename_expr(arg, map);
            }
        }
        Expr::Data {
            name, accessors, ..
        } => {
            rename_name(name, map);
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    rename_expr(expr, map);
                }
            }
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Parameter substitution (call by name)

/// A parameter paired with the argument it was called with.
///
/// The argument is substituted at each mention rather than read once, so this is
/// a *name* for whatever the caller wrote, not a slot holding a value. `array` is
/// the declared size when the parameter was written `v[…]`, which is what makes
/// `v[0]` legal inside the body and a bare `v` illegal.
struct Binding {
    arg: Expr,
    array: Option<usize>,
}

fn bind_params(def: &FnDef, args: Vec<Expr>) -> BTreeMap<String, Binding> {
    def.params
        .iter()
        .zip(args)
        .map(|(param, arg)| {
            (
                param.name.clone(),
                Binding {
                    arg,
                    array: param.array,
                },
            )
        })
        .collect()
}

fn substitute_stmts(
    stmts: &mut [Stmt],
    map: &BTreeMap<String, Binding>,
    source: &str,
) -> Result<(), TranspileError> {
    for stmt in stmts {
        substitute_stmt(stmt, map, source)?;
    }
    Ok(())
}

fn substitute_stmt(
    stmt: &mut Stmt,
    map: &BTreeMap<String, Binding>,
    source: &str,
) -> Result<(), TranspileError> {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Print(value)
        | Stmt::ExprStmt(value) => substitute(value, map, source),
        // Assignment to a parameter writes back through the argument, so the
        // argument has to be a variable or an array element.
        Stmt::Assign { name, value, pos } => {
            substitute(value, map, source)?;
            let Some(binding) = map.get(name) else {
                return Ok(());
            };
            let replacement = match target_of(&binding.arg, name, *pos, source)? {
                Target::Name(target) => Stmt::Assign {
                    name: target,
                    value: value.clone(),
                    pos: *pos,
                },
                Target::Element { name, index } => Stmt::AssignElement {
                    name,
                    index,
                    value: value.clone(),
                    pos: *pos,
                },
            };
            *stmt = replacement;
            Ok(())
        }
        Stmt::AssignElementExpr {
            name,
            index,
            value,
            pos,
        } => {
            substitute(index, map, source)?;
            substitute(value, map, source)?;
            if let Some(binding) = map.get(name) {
                let target = match target_of(&binding.arg, name, *pos, source)? {
                    Target::Name(target) => target,
                    Target::Element { .. } => {
                        return Err(TranspileError::at(
                            source,
                            format!(
                                "`{name}` is a parameter indexed as an array; pass the array's \
                                 name, not an element"
                            ),
                            *pos,
                        ));
                    }
                };
                *name = target;
            }
            Ok(())
        }
        Stmt::LetArray { values, .. } => {
            for value in values {
                substitute(value, map, source)?;
            }
            Ok(())
        }
        Stmt::Return { value, .. } => match value {
            Some(value) => substitute(value, map, source),
            None => Ok(()),
        },
        Stmt::Memory { value, .. } => substitute(value, map, source),
        // A parameter that is the target of `name[i] = …` must be an array
        // name passed by name.
        Stmt::AssignElement {
            name, value, pos, ..
        } => {
            substitute(value, map, source)?;
            if let Some(binding) = map.get(name) {
                match target_of(&binding.arg, name, *pos, source)? {
                    Target::Name(target) => *name = target,
                    Target::Element { .. } => {
                        return Err(TranspileError::at(
                            source,
                            format!(
                                "`{name}` is a parameter indexed as an array; pass the array's name"
                            ),
                            *pos,
                        ));
                    }
                }
            }
            Ok(())
        }
        Stmt::Free { name, pos, .. } => {
            if map.contains_key(name) {
                return Err(TranspileError::at(
                    source,
                    format!("`free {name}` cannot release a parameter; pass an ordinary variable"),
                    *pos,
                ));
            }
            Ok(())
        }
        Stmt::Data { x, y, freq, .. } => {
            substitute(x, map, source)?;
            if let Some(y) = y {
                substitute(y, map, source)?;
            }
            if let Some(freq) = freq {
                substitute(freq, map, source)?;
            }
            Ok(())
        }
        Stmt::CondJump { cond, target, .. } => {
            substitute(cond, map, source)?;
            substitute_stmt(target, map, source)
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            substitute(cond, map, source)?;
            substitute_stmts(then_body, map, source)?;
            substitute_stmts(else_body, map, source)
        }
        Stmt::While { cond, body } => {
            substitute(cond, map, source)?;
            substitute_stmts(body, map, source)
        }
        Stmt::For(for_stmt) => {
            substitute(&mut for_stmt.init_value, map, source)?;
            substitute(&mut for_stmt.cond, map, source)?;
            substitute(&mut for_stmt.update_value, map, source)?;
            // A `for` whose header assigns a parameter writes back through it.
            for (name, pos) in [
                (&mut for_stmt.init_name, for_stmt.pos),
                (&mut for_stmt.update_name, for_stmt.pos),
            ] {
                if let Some(binding) = map.get(name) {
                    match target_of(&binding.arg, name, pos, source)? {
                        Target::Name(target) => *name = target,
                        Target::Element { .. } => {
                            return Err(TranspileError::at(
                                source,
                                format!(
                                    "`{name}` is a `for` variable and must be a plain variable"
                                ),
                                pos,
                            ));
                        }
                    }
                }
            }
            substitute_stmts(&mut for_stmt.body, map, source)
        }
        Stmt::Block(stmts) => substitute_stmts(stmts, map, source),
        Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty
        | Stmt::Function(_) => Ok(()),
    }
}

/// A parameter's argument reduced to something assignable.
enum Target {
    Name(String),
    Element { name: String, index: usize },
}

/// Turn an argument into an assignment target, or explain why it cannot be one.
fn target_of(arg: &Expr, param: &str, pos: usize, source: &str) -> Result<Target, TranspileError> {
    match arg {
        Expr::Name(name, _) => Ok(Target::Name(name.clone())),
        Expr::Data {
            name, accessors, ..
        } => match accessors.as_slice() {
            [Accessor::Index { index, .. }] => Ok(Target::Element {
                name: name.clone(),
                index: *index,
            }),
            _ => Err(TranspileError::at(
                source,
                format!(
                    "`{param}` is assigned inside the function, so its argument must be a variable \
                     or an array element, not an expression"
                ),
                pos,
            )),
        },
        _ => Err(TranspileError::at(
            source,
            format!(
                "`{param}` is assigned inside the function, so its argument must be a variable or \
                 an array element, not an expression"
            ),
            pos,
        )),
    }
}

fn substitute(
    expr: &mut Expr,
    map: &BTreeMap<String, Binding>,
    source: &str,
) -> Result<(), TranspileError> {
    match expr {
        Expr::Name(name, _) => {
            // A bare array parameter is substituted like any other name, even
            // though it has no value of its own. It has to be: `g(v)` passes the
            // array on to another function's array parameter, and that is only
            // decidable once `g` is known. Using one as a value (`print(v)`) is
            // caught after substitution, by the check that rejects any array
            // name in a value position — the same error a direct `print(a)` gets.
            if let Some(binding) = map.get(name) {
                *expr = binding.arg.clone();
            }
        }
        Expr::Unary(_, inner) => substitute(inner, map, source)?,
        Expr::Binary(_, left, right) => {
            substitute(left, map, source)?;
            substitute(right, map, source)?;
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                substitute(arg, map, source)?;
            }
        }
        Expr::Data {
            name,
            accessors,
            pos,
        } => {
            let Some(binding) = map.get(name) else {
                return Ok(());
            };
            let Some(size) = binding.array else {
                return Err(TranspileError::at(
                    source,
                    format!(
                        "`{name}` is a value parameter used as an array (`{name}[…]`); declare it \
                         `fn f({name}[n])` to pass an array, or pass the elements as separate \
                         parameters"
                    ),
                    *pos,
                ));
            };
            // Call by name: `v[0]` becomes `a[0]` in the caller, so the array is
            // the caller's own and each access is bounds-checked against it
            // there. Nothing is copied and the parameter costs no memory.
            let Expr::Name(target, _) = &binding.arg else {
                return Err(TranspileError::at(
                    source,
                    format!(
                        "the argument for the array parameter `{name}[{size}]` must be the name \
                         of an array"
                    ),
                    *pos,
                ));
            };
            // The declared size is the extent the body may index, so a literal
            // beyond it is reportable here — and naming the parameter is more
            // use than naming whichever array the caller happened to pass.
            for accessor in accessors.iter() {
                if let Accessor::Index { index, pos: at } = accessor
                    && *index >= size
                {
                    return Err(TranspileError::at(
                        source,
                        format!(
                            "`{name}[{index}]` is out of bounds: `{name}` is declared with \
                             {size} element(s)"
                        ),
                        *at,
                    ));
                }
            }
            *expr = Expr::Data {
                name: target.clone(),
                accessors: accessors.clone(),
                pos: *pos,
            };
        }
        Expr::Number(_)
        | Expr::BaseLiteral { .. }
        | Expr::Pi(_)
        | Expr::E(_)
        | Expr::Ans(_)
        | Expr::StatVar(..)
        | Expr::Constant(..)
        | Expr::Input(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    /// Expand user functions in `source` and return the flat program's debug
    /// form, which is enough to see what was inlined.
    fn expanded(source: &str) -> Result<String, TranspileError> {
        let tokens = lex(source)?;
        let program = parse(&tokens, source)?;
        Ok(format!("{:?}", expand(program, source, &Data::default())?))
    }

    #[test]
    fn an_expression_function_inlines_without_a_temporary() {
        let out = expanded("fn sq(x) = x * x;\nfn main() { let a = 3; print(sq(a)); }").unwrap();
        assert!(out.contains("Binary(Mul"), "{out}");
        assert!(!out.contains("$sq$"), "no temp should be needed: {out}");
    }

    #[test]
    fn a_procedure_inlines_its_body_and_a_temporary() {
        let out = expand_source(
            "fn add(a, b) { let s = a + b; return s; }\nfn main() { print(add(1, 2)); }",
        );
        assert!(out.contains("add$s$"), "the local is renamed: {out}");
        assert!(!out.contains("Function"), "{out}");
    }

    #[test]
    fn locals_are_renamed_per_call() {
        let out = expand_source(
            "fn add(a, b) { let s = a + b; return s; }\nfn main() { print(add(1, 2)); print(add(3, 4)); }",
        );
        // Each call gets its own `add$s$N`; the two tags differ.
        let tags: std::collections::BTreeSet<&str> = out
            .match_indices("add$s$")
            .map(|(start, _)| &out[start..start + 7])
            .collect();
        assert_eq!(tags.len(), 2, "one local name per call: {out}");
    }

    #[test]
    fn main_becomes_the_program() {
        let out = expanded("fn main() { print(1); }").unwrap();
        assert!(out.contains("Print(Number(1.0))"), "{out}");
    }

    #[test]
    fn a_file_without_main_is_a_library() {
        // No `fn main()`: this is a library for another program to `#include`,
        // so there is nothing to run and the expansion is empty. Building it is
        // therefore not an error. (`expanded` reports the `Debug` form, so an
        // empty program is `"[]"`.)
        assert_eq!(expanded("fn f(n) = n + 1;").unwrap(), "[]");
        assert_eq!(expanded("const k = 1;").unwrap(), "[]");
        assert_eq!(expanded("").unwrap(), "[]");
    }

    #[test]
    fn a_loose_statement_needs_an_entry_point() {
        // Nothing is defined, so the writer was starting a program and has not
        // reached `main`; naming the missing entry point is more useful than the
        // top-level rule.
        let err = expanded("print(1);").unwrap_err();
        assert!(
            err.message.contains("needs an entry point"),
            "{}",
            err.message
        );
    }

    #[test]
    fn top_level_statements_are_rejected_when_functions_exist() {
        let err = expanded("fn main() { print(1); }\nprint(2);").unwrap_err();
        assert!(err.message.contains("top level"), "{}", err.message);
    }

    #[test]
    fn a_top_level_const_is_allowed_and_reaches_functions() {
        let out = expanded("const k = 2;\nfn f(x) = x * k;\nfn main() { print(f(3)); }").unwrap();
        // The const survives to the inlined body for `const` inlining to fold.
        assert!(out.contains("Const"), "{out}");
        assert!(out.contains("Name(\"k\""), "{out}");
    }

    #[test]
    fn a_function_cannot_read_an_undeclared_global() {
        let err = expanded("fn f(x) = x + a;\nfn main() { print(f(1)); }").unwrap_err();
        assert!(
            err.message.contains("`a` is not defined in `f`"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_function_cannot_create_an_implicit_global() {
        let err = expanded("fn f() { let x = 1; y = x + 1; }\nfn main() { f(); }").unwrap_err();
        assert!(
            err.message.contains("`y` is not defined in `f`"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_forward_reference_inside_a_function_still_works() {
        // The order-free rule is scoped to the function, not removed.
        let out = expanded("fn f() { let a = b; let b = 1; return a; }\nfn main() { print(f()); }")
            .unwrap();
        assert!(!out.contains("not defined"), "{out}");
    }

    #[test]
    fn a_duplicate_definition_is_rejected() {
        let err = expanded("fn f(x) = x;\nfn f(x) = x;\nfn main() { print(f(1)); }").unwrap_err();
        assert!(err.message.contains("already defined"), "{}", err.message);
    }

    #[test]
    fn recursion_is_rejected() {
        let err = expanded("fn f(n) = f(n);\nfn main() { print(f(1)); }").unwrap_err();
        assert!(err.message.contains("recursive"), "{}", err.message);
    }

    #[test]
    fn unknown_calls_are_reported() {
        let err = expanded("fn main() { print(nope(1)); }").unwrap_err();
        assert!(
            err.message.contains("unknown function `nope`"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_stray_return_is_rejected() {
        let err = expanded("return 1;").unwrap_err();
        assert!(err.message.contains("only valid inside"), "{}", err.message);
    }

    #[test]
    fn a_non_tail_return_is_rejected() {
        let err =
            expanded("fn f(n) { if (n) { return 1; } return 2; }\nfn main() { print(f(1)); }")
                .unwrap_err();
        assert!(err.message.contains("last statement"), "{}", err.message);
    }

    #[test]
    fn a_procedure_in_a_loop_condition_is_rejected() {
        let source = "fn f(n) { let t = n; return t; }\n\
             fn main() { let a = 0; while (f(a) > 0) { print(a); } }";
        let err = expanded(source).unwrap_err();
        assert!(err.message.contains("loop condition"), "{}", err.message);
    }

    #[test]
    fn call_by_name_substitutes_the_argument() {
        // A variable argument is mentioned once per parameter reference.
        let ok = expand_source("fn twice(x) = x + x;\nfn main() { let a = 1; print(twice(a)); }");
        let count = ok.matches("Name(\"a\"").count();
        assert_eq!(count, 2, "the argument is mentioned twice: {ok}");
    }

    /// Expand and render, unwrapping.
    fn expand_source(source: &str) -> String {
        expanded(source).unwrap_or_else(|e| panic!("expand failed: {e}\n{source}"))
    }
}
