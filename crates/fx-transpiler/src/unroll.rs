//! Unroll constant-bound `for` loops so array indices become literals.
//!
//! PRGM has no indirect addressing: `m[i]` cannot be looked up from a variable
//! at run time, so an array element must name a fixed memory while transpiling.
//! That is why the parser accepts a non-literal index only provisionally: a
//! `for` loop whose bounds are known at transpile time is expanded here, and
//! the induction variable is replaced by each of its values. `m[i] = input();`
//! inside `for (let i = 0; i < 6; i = i + 1)` therefore becomes six element
//! assignments, `m[0] = input();` … `m[5] = input();`.
//!
//! Only loops that actually need it are unrolled — that is, loops whose body
//! contains a computed index that mentions the induction variable. A constant
//! loop over a scalar (`for (let i = 0; i < 5; …) print(i);`) keeps its compact
//! native `For`/`Next` form, since unrolling it would cost bytes, not save
//! them.
//!
//! ## What stops an unroll
//!
//! Unrolling is a source-to-source expansion, so it is only valid when the loop
//! body is itself straightforward:
//!
//! * the bounds must be integer literals after folding, and the shape must be
//!   the canonical `i < limit` / `i = i + step` (either direction);
//! * the body must not declare a variable, `free` one, or contain `break`,
//!   `goto` or `label` — each would mean something different once the body is
//!   written out more than once;
//! * the expansion is bounded by [`MAX_STATEMENTS`] and [`MAX_ITERATIONS`], so a
//!   huge loop cannot silently produce a giant program.
//!
//! Anything it cannot unroll is left alone; the allocator then reports the
//! computed index with an explanation.

use std::collections::BTreeMap;

use crate::ast::{Accessor, BinOp, Expr, ForStmt, Program, Stmt};
use crate::fold;

/// The most iterations a single loop may expand to.
const MAX_ITERATIONS: usize = 256;
/// The most statements the whole expansion may produce.
const MAX_STATEMENTS: usize = 512;

/// Unroll the loops in `program` that need it, in place.
pub fn unroll_program(program: &mut Program) {
    let totals = reference_totals(program);
    let mut remaining = MAX_STATEMENTS;
    rewrite_stmts(program, &[], &totals, &mut remaining);
    resolve_indices(program);
}

// ---------------------------------------------------------------------------
// Rewriting

type Env = [(String, f64)];

fn rewrite_stmts(
    stmts: &mut [Stmt],
    env: &Env,
    totals: &BTreeMap<String, usize>,
    remaining: &mut usize,
) {
    for stmt in stmts.iter_mut() {
        rewrite_stmt(stmt, env, totals, remaining);
    }
}

fn rewrite_stmt(
    stmt: &mut Stmt,
    env: &Env,
    totals: &BTreeMap<String, usize>,
    remaining: &mut usize,
) {
    // Apply the enclosing loop's values first, then fold, so the expressions
    // this pass reasons about are already as simple as they can be.
    substitute_stmt(stmt, env);
    fold_stmt(stmt);

    match stmt {
        Stmt::For(for_stmt) => {
            let replacement = try_unroll(for_stmt, totals, remaining);
            match replacement {
                Some(body) => *stmt = Stmt::Block(body),
                None => rewrite_stmts(&mut for_stmt.body, &[], totals, remaining),
            }
        }
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            rewrite_stmts(then_body, &[], totals, remaining);
            rewrite_stmts(else_body, &[], totals, remaining);
        }
        Stmt::While { body, .. } => rewrite_stmts(body, &[], totals, remaining),
        Stmt::Block(stmts) => rewrite_stmts(stmts, &[], totals, remaining),
        _ => {}
    }
}

/// Expand `for_stmt` into a list of statements, or `None` when it must be left
/// as a loop.
fn try_unroll(
    for_stmt: &mut ForStmt,
    totals: &BTreeMap<String, usize>,
    remaining: &mut usize,
) -> Option<Vec<Stmt>> {
    if !body_has_computed_index_of(&for_stmt.body, &for_stmt.init_name) {
        return None;
    }
    if !body_is_unrollable(&for_stmt.body) {
        return None;
    }
    let (values, final_value) = iteration_values(for_stmt)?;

    // Refuse an expansion that does not fit the budget before doing any work.
    let per_iteration = for_stmt.body.len().max(1);
    let needed = values.len().saturating_mul(per_iteration);
    if needed > *remaining {
        return None;
    }

    let mut out = Vec::new();
    for value in &values {
        let mut body = for_stmt.body.clone();
        substitute_stmts(&mut body, &[(for_stmt.init_name.clone(), *value)]);
        fold_stmts(&mut body);
        rewrite_stmts(&mut body, &[], totals, remaining);
        *remaining = remaining.saturating_sub(body.len());
        out.extend(body);
    }

    // A nested `for (let …)` that could not itself be unrolled would be
    // duplicated here and declare the same variable more than once. Leaving
    // this loop alone is safer; the allocator then reports the computed index.
    if contains_declaring_loop(&out) {
        return None;
    }

    // A counter that nothing outside the loop mentions is dropped, exactly as
    // writing the loop out by hand would drop it; that is what lets the
    // expansion be worth the bytes. If it *is* used afterwards (or was an
    // existing variable, not a `let` in the header), the machine's `For` leaves
    // it at the first value past the limit, so the unrolled program records
    // that value.
    let external = !for_stmt.is_decl
        || totals.get(&for_stmt.init_name).copied().unwrap_or(0)
            > count_for(for_stmt, &for_stmt.init_name);
    if external {
        let value = Expr::Number(final_value);
        out.push(if for_stmt.is_decl {
            Stmt::Let {
                name: for_stmt.init_name.clone(),
                value,
                pos: for_stmt.pos,
            }
        } else {
            Stmt::Assign {
                name: for_stmt.init_name.clone(),
                value,
                pos: for_stmt.pos,
            }
        });
    }
    Some(out)
}

/// The values the counter takes, and the value it holds after the loop.
fn iteration_values(for_stmt: &ForStmt) -> Option<(Vec<f64>, f64)> {
    if for_stmt.init_name != for_stmt.update_name {
        return None;
    }
    let start = number(&for_stmt.init_value)?;
    let (cond_op, limit) = match &for_stmt.cond {
        Expr::Binary(op, left, right) if is_name(left, &for_stmt.init_name) => {
            (*op, number(right)?)
        }
        _ => return None,
    };
    let (update_op, step) = match &for_stmt.update_value {
        Expr::Binary(op, left, right) if is_name(left, &for_stmt.update_name) => {
            (*op, number(right)?)
        }
        _ => return None,
    };
    // Integer bounds keep the unrolled counter identical to the machine's, with
    // no floating-point drift across iterations.
    if start.fract() != 0.0 || step.fract() != 0.0 || limit.fract() != 0.0 {
        return None;
    }
    let ascending = match (cond_op, update_op) {
        (BinOp::Lt, BinOp::Add) | (BinOp::Le, BinOp::Add) => true,
        (BinOp::Gt, BinOp::Sub) | (BinOp::Ge, BinOp::Sub) => false,
        _ => return None,
    };
    if step <= 0.0 {
        return None;
    }
    let signed_step = if ascending { step } else { -step };

    let mut value = start;
    let mut values = Vec::new();
    while in_range(cond_op, value, limit) {
        values.push(value);
        if values.len() > MAX_ITERATIONS {
            return None;
        }
        let next = value + signed_step;
        if next == value {
            return None;
        }
        value = next;
    }
    Some((values, value))
}

fn in_range(op: BinOp, value: f64, limit: f64) -> bool {
    match op {
        BinOp::Lt => value < limit,
        BinOp::Le => value <= limit,
        BinOp::Gt => value > limit,
        BinOp::Ge => value >= limit,
        _ => false,
    }
}

fn number(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(value) => Some(*value),
        _ => None,
    }
}

fn is_name(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Name(known, _) if known == name)
}

// ---------------------------------------------------------------------------
// Deciding what to unroll

/// Whether the body contains a computed index that reads `name`.
///
/// This is what makes an unroll necessary: a loop over a plain scalar keeps its
/// native `For`/`Next` form.
fn body_has_computed_index_of(stmts: &[Stmt], name: &str) -> bool {
    stmts
        .iter()
        .any(|stmt| stmt_has_computed_index_of(stmt, name))
}

fn stmt_has_computed_index_of(stmt: &Stmt, name: &str) -> bool {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. } => expr_has_computed_index_of(value, name),
        Stmt::AssignElementExpr { index, value, .. } => {
            expr_has_name(index, name) || expr_has_computed_index_of(value, name)
        }
        Stmt::LetArray { values, .. } => values
            .iter()
            .any(|value| expr_has_computed_index_of(value, name)),
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => expr_has_computed_index_of(expr, name),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_computed_index_of(cond, name)
                || body_has_computed_index_of(then_body, name)
                || body_has_computed_index_of(else_body, name)
        }
        Stmt::While { cond, body } => {
            expr_has_computed_index_of(cond, name) || body_has_computed_index_of(body, name)
        }
        Stmt::For(inner) => {
            expr_has_name(&inner.cond, name)
                || body_has_computed_index_of(&inner.body, name)
                || expr_has_computed_index_of(&inner.update_value, name)
        }
        Stmt::Block(stmts) => body_has_computed_index_of(stmts, name),
        Stmt::Memory { value, .. } => expr_has_computed_index_of(value, name),
        Stmt::Data { x, y, freq, .. } => {
            expr_has_computed_index_of(x, name)
                || y.as_ref()
                    .is_some_and(|y| expr_has_computed_index_of(y, name))
                || freq
                    .as_ref()
                    .is_some_and(|f| expr_has_computed_index_of(f, name))
        }
        Stmt::CondJump { cond, target, .. } => {
            expr_has_computed_index_of(cond, name) || stmt_has_computed_index_of(target, name)
        }
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => false,
    }
}

fn expr_has_computed_index_of(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Data { accessors, .. } => accessors.iter().any(|accessor| {
            matches!(accessor, Accessor::IndexExpr { expr, .. }
                if expr_has_name(expr, name) || expr_has_computed_index_of(expr, name))
        }),
        Expr::Unary(_, inner) => expr_has_computed_index_of(inner, name),
        Expr::Binary(_, left, right) => {
            expr_has_computed_index_of(left, name) || expr_has_computed_index_of(right, name)
        }
        Expr::Call(_, args, _) => args.iter().any(|arg| expr_has_computed_index_of(arg, name)),
        _ => false,
    }
}

fn expr_has_name(expr: &Expr, name: &str) -> bool {
    match expr {
        Expr::Name(known, _) => known == name,
        Expr::Unary(_, inner) => expr_has_name(inner, name),
        Expr::Binary(_, left, right) => expr_has_name(left, name) || expr_has_name(right, name),
        Expr::Call(_, args, _) => args.iter().any(|arg| expr_has_name(arg, name)),
        Expr::Data { accessors, .. } => accessors.iter().any(|accessor| {
            matches!(accessor, Accessor::IndexExpr { expr, .. } if expr_has_name(expr, name))
        }),
        _ => false,
    }
}

/// Whether the body can be copied more than once without changing meaning.
///
/// A declaration would be repeated (and rejected as a re-declaration), a `free`
/// would release a memory the next copy still needs, and `break`/`goto`/`label`
/// have no meaning once the loop marker is gone.
fn body_is_unrollable(stmts: &[Stmt]) -> bool {
    stmts.iter().all(|stmt| match stmt {
        Stmt::Let { .. }
        | Stmt::LetArray { .. }
        | Stmt::Const { .. }
        | Stmt::Free { .. }
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..) => false,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => body_is_unrollable(then_body) && body_is_unrollable(else_body),
        Stmt::While { body, .. } => body_is_unrollable(body),
        Stmt::For(inner) => body_is_unrollable(&inner.body),
        Stmt::Block(stmts) => body_is_unrollable(stmts),
        Stmt::CondJump { target, .. } => body_is_unrollable(std::slice::from_ref(target)),
        Stmt::Assign { .. }
        | Stmt::AssignElement { .. }
        | Stmt::AssignElementExpr { .. }
        | Stmt::Memory { .. }
        | Stmt::Data { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Print(_)
        | Stmt::ExprStmt(_)
        | Stmt::Empty => true,
    })
}

// ---------------------------------------------------------------------------
// Substitution and folding

fn substitute_stmts(stmts: &mut [Stmt], env: &[(String, f64)]) {
    for stmt in stmts {
        substitute_stmt(stmt, env);
    }
}

fn substitute_stmt(stmt: &mut Stmt, env: &[(String, f64)]) {
    walk_exprs(stmt, &mut |expr| substitute_expr(expr, env));
}

fn fold_stmts(stmts: &mut [Stmt]) {
    for stmt in stmts {
        fold_stmt(stmt);
    }
}

fn fold_stmt(stmt: &mut Stmt) {
    walk_exprs(stmt, &mut fold::fold_expr);
}

fn substitute_expr(expr: &mut Expr, env: &[(String, f64)]) {
    match expr {
        Expr::Name(name, _) => {
            if let Some((_, value)) = env.iter().rev().find(|(known, _)| known == name) {
                *expr = Expr::Number(*value);
            }
        }
        Expr::Unary(_, inner) => substitute_expr(inner, env),
        Expr::Binary(_, left, right) => {
            substitute_expr(left, env);
            substitute_expr(right, env);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                substitute_expr(arg, env);
            }
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    substitute_expr(expr, env);
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

/// Apply `f` to every expression in `stmt`, including nested statements.
fn walk_exprs(stmt: &mut Stmt, f: &mut dyn FnMut(&mut Expr)) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. } => f(value),
        Stmt::AssignElementExpr { index, value, .. } => {
            f(index);
            f(value);
        }
        Stmt::LetArray { values, .. } => {
            for value in values {
                f(value);
            }
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => f(expr),
        Stmt::Memory { value, .. } => f(value),
        Stmt::Data { x, y, freq, .. } => {
            f(x);
            if let Some(y) = y {
                f(y);
            }
            if let Some(freq) = freq {
                f(freq);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            f(cond);
            walk_exprs(target, f);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            f(cond);
            for stmt in then_body.iter_mut() {
                walk_exprs(stmt, f);
            }
            for stmt in else_body.iter_mut() {
                walk_exprs(stmt, f);
            }
        }
        Stmt::While { cond, body } => {
            f(cond);
            for stmt in body.iter_mut() {
                walk_exprs(stmt, f);
            }
        }
        Stmt::For(for_stmt) => {
            f(&mut for_stmt.init_value);
            f(&mut for_stmt.cond);
            f(&mut for_stmt.update_value);
            for stmt in for_stmt.body.iter_mut() {
                walk_exprs(stmt, f);
            }
        }
        Stmt::Block(stmts) => {
            for stmt in stmts.iter_mut() {
                walk_exprs(stmt, f);
            }
        }
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => {}
    }
}

// ---------------------------------------------------------------------------
// Resolving indices to literals

/// Rewrite every `[expr]` that folded to a whole number as a literal index.
fn resolve_indices(program: &mut Program) {
    for stmt in program {
        resolve_stmt(stmt);
    }
}

fn resolve_stmt(stmt: &mut Stmt) {
    match stmt {
        Stmt::AssignElementExpr {
            name,
            index,
            value,
            pos,
        } => {
            resolve_expr(index);
            resolve_expr(value);
            if let Some(literal) = as_index(index) {
                *stmt = Stmt::AssignElement {
                    name: name.clone(),
                    index: literal,
                    value: value.clone(),
                    pos: *pos,
                };
            }
        }
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. } => resolve_expr(value),
        Stmt::LetArray { values, .. } => {
            for value in values {
                resolve_expr(value);
            }
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => resolve_expr(expr),
        Stmt::Memory { value, .. } => resolve_expr(value),
        Stmt::Data { x, y, freq, .. } => {
            resolve_expr(x);
            if let Some(y) = y {
                resolve_expr(y);
            }
            if let Some(freq) = freq {
                resolve_expr(freq);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            resolve_expr(cond);
            resolve_stmt(target);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            resolve_expr(cond);
            for stmt in then_body {
                resolve_stmt(stmt);
            }
            for stmt in else_body {
                resolve_stmt(stmt);
            }
        }
        Stmt::While { cond, body } => {
            resolve_expr(cond);
            for stmt in body {
                resolve_stmt(stmt);
            }
        }
        Stmt::For(for_stmt) => {
            resolve_expr(&mut for_stmt.init_value);
            resolve_expr(&mut for_stmt.cond);
            resolve_expr(&mut for_stmt.update_value);
            for stmt in &mut for_stmt.body {
                resolve_stmt(stmt);
            }
        }
        Stmt::Block(stmts) => {
            for stmt in stmts {
                resolve_stmt(stmt);
            }
        }
        Stmt::Free { .. }
        | Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => {}
    }
}

fn resolve_expr(expr: &mut Expr) {
    match expr {
        Expr::Unary(_, inner) => resolve_expr(inner),
        Expr::Binary(_, left, right) => {
            resolve_expr(left);
            resolve_expr(right);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                resolve_expr(arg);
            }
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors.iter_mut() {
                if let Accessor::IndexExpr { expr, pos } = accessor {
                    resolve_expr(expr);
                    if let Some(literal) = as_index(expr) {
                        *accessor = Accessor::Index {
                            index: literal,
                            pos: *pos,
                        };
                    }
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

fn as_index(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Number(value)
            if value.fract() == 0.0 && *value >= 0.0 && *value <= usize::MAX as f64 =>
        {
            Some(*value as usize)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Reference counts, for deciding whether a counter is needed afterwards

fn reference_totals(program: &[Stmt]) -> BTreeMap<String, usize> {
    let mut totals = BTreeMap::new();
    for stmt in program {
        count_stmt(stmt, &mut totals);
    }
    totals
}

fn count_stmt(stmt: &Stmt, totals: &mut BTreeMap<String, usize>) {
    match stmt {
        Stmt::Let { name, value, .. } | Stmt::Const { name, value, .. } => {
            *totals.entry(name.clone()).or_default() += 1;
            count_expr(value, totals);
        }
        Stmt::Assign { name, value, .. } | Stmt::AssignElement { name, value, .. } => {
            *totals.entry(name.clone()).or_default() += 1;
            count_expr(value, totals);
        }
        Stmt::AssignElementExpr {
            name, index, value, ..
        } => {
            *totals.entry(name.clone()).or_default() += 1;
            count_expr(index, totals);
            count_expr(value, totals);
        }
        Stmt::LetArray { name, values, .. } => {
            *totals.entry(name.clone()).or_default() += 1;
            for value in values {
                count_expr(value, totals);
            }
        }
        Stmt::Free { name, .. } => {
            *totals.entry(name.clone()).or_default() += 1;
        }
        Stmt::Memory { value, .. } => count_expr(value, totals),
        Stmt::Data { x, y, freq, .. } => {
            count_expr(x, totals);
            if let Some(y) = y {
                count_expr(y, totals);
            }
            if let Some(freq) = freq {
                count_expr(freq, totals);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            count_expr(cond, totals);
            count_stmt(target, totals);
        }
        Stmt::Print(expr) | Stmt::ExprStmt(expr) => count_expr(expr, totals),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_expr(cond, totals);
            for stmt in then_body {
                count_stmt(stmt, totals);
            }
            for stmt in else_body {
                count_stmt(stmt, totals);
            }
        }
        Stmt::While { cond, body } => {
            count_expr(cond, totals);
            for stmt in body {
                count_stmt(stmt, totals);
            }
        }
        Stmt::For(for_stmt) => {
            *totals.entry(for_stmt.init_name.clone()).or_default() += 1;
            *totals.entry(for_stmt.update_name.clone()).or_default() += 1;
            count_expr(&for_stmt.init_value, totals);
            count_expr(&for_stmt.cond, totals);
            count_expr(&for_stmt.update_value, totals);
            for stmt in &for_stmt.body {
                count_stmt(stmt, totals);
            }
        }
        Stmt::Block(stmts) => {
            for stmt in stmts {
                count_stmt(stmt, totals);
            }
        }
        Stmt::Setup { .. }
        | Stmt::ClrMemory
        | Stmt::ClrStat
        | Stmt::FreqOn
        | Stmt::FreqOff
        | Stmt::Break
        | Stmt::Goto(..)
        | Stmt::Label(..)
        | Stmt::Empty => {}
    }
}

fn count_expr(expr: &Expr, totals: &mut BTreeMap<String, usize>) {
    match expr {
        Expr::Name(name, _) => {
            *totals.entry(name.clone()).or_default() += 1;
        }
        Expr::Unary(_, inner) => count_expr(inner, totals),
        Expr::Binary(_, left, right) => {
            count_expr(left, totals);
            count_expr(right, totals);
        }
        Expr::Call(_, args, _) => {
            for arg in args {
                count_expr(arg, totals);
            }
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    count_expr(expr, totals);
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

fn count_for(for_stmt: &ForStmt, name: &str) -> usize {
    let mut total = 0;
    if for_stmt.init_name == name {
        total += 1;
    }
    if for_stmt.update_name == name {
        total += 1;
    }
    let mut totals = BTreeMap::new();
    count_expr(&for_stmt.init_value, &mut totals);
    count_expr(&for_stmt.cond, &mut totals);
    count_expr(&for_stmt.update_value, &mut totals);
    for stmt in &for_stmt.body {
        count_stmt(stmt, &mut totals);
    }
    total + totals.get(name).copied().unwrap_or(0)
}

/// Whether any statement still declares a loop counter that the unroller could
/// not remove.
fn contains_declaring_loop(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::For(for_stmt) => for_stmt.is_decl || contains_declaring_loop(&for_stmt.body),
        Stmt::If {
            then_body,
            else_body,
            ..
        } => contains_declaring_loop(then_body) || contains_declaring_loop(else_body),
        Stmt::While { body, .. } => contains_declaring_loop(body),
        Stmt::Block(stmts) => contains_declaring_loop(stmts),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse;

    fn transpile(source: &str) -> String {
        let tokens = lex(source).unwrap();
        let mut program = parse(&tokens, source).unwrap();
        fold::fold_program(&mut program);
        unroll_program(&mut program);
        // The emitter is the real output path; this test only needs to know
        // that the computed indices were resolved away.
        format!("{program:?}")
    }

    #[test]
    fn an_input_loop_over_an_array_unrolls() {
        let out = transpile("let m[3];\nfor (let i = 0; i < 3; i = i + 1) { m[i] = input(); }");
        assert!(
            out.contains("AssignElement { name: \"m\", index: 0"),
            "{out}"
        );
        assert!(out.contains("index: 1"), "{out}");
        assert!(out.contains("index: 2"), "{out}");
        assert!(!out.contains("IndexExpr"), "{out}");
        assert!(!out.contains("For("), "{out}");
    }

    #[test]
    fn a_scalar_loop_keeps_its_native_form() {
        let out = transpile("for (let i = 0; i < 5; i = i + 1) { print(i); }");
        assert!(out.contains("For("), "the loop should survive: {out}");
    }

    #[test]
    fn a_counter_used_afterwards_is_kept() {
        let out =
            transpile("let m[2];\nfor (let i = 0; i < 2; i = i + 1) { m[i] = 1; }\nprint(i);");
        // The counter is re-declared with the value it would have had, 2.
        assert!(out.contains("Number(2.0)"), "{out}");
    }

    #[test]
    fn a_variable_bound_is_not_unrolled() {
        let out = transpile(
            "let n = input();\nlet m[2];\nfor (let i = 0; i < n; i = i + 1) { m[i] = 1; }",
        );
        assert!(out.contains("For("), "the loop should survive: {out}");
        assert!(out.contains("AssignElementExpr"), "{out}");
    }
}
