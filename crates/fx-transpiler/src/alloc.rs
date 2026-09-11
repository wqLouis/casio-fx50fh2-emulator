//! Map `.fxc` variable names onto the calculator's seven memories.
//!
//! PRGM has exactly seven assignable memories — `A B C D X Y M` — and no more,
//! so this pass decides what each name uses. It keeps a **register table** (which
//! variable occupies each memory) while walking the program in order, and
//! records each variable's binding as a byte range, from where it was declared
//! to where it was released.
//!
//! ## How a program fits
//!
//! * **`const` and `#data` use no memory at all.** Their values are inlined, so
//!   they never reach this module.
//! * **An array element uses one memory per element.** PRGM has no indirect
//!   addressing, so `a[k]` must name a fixed memory while transpiling; that is
//!   why an array index has to be a literal. The payoff is that indexing costs
//!   *no program bytes* — only memories — which matters because the whole
//!   machine has 680 bytes of program storage shared by all four programs.
//! * **`free name;` releases a memory** so a later variable — including the same
//!   name declared again — can use it. Nothing is released implicitly: a
//!   memory's final value is observable (PRGM leaves its answer in one, a later
//!   program or the user can read it), so the transpiler cannot prove a name is
//!   dead. Only the programmer knows.
//!
//! ## `let` declares; assignment does not
//!
//! A `let` (or `const`) **introduces** a name:
//!
//! ```text
//! let x = input();
//! free x;
//! let x = input();   // fine: x is declared again, into a fresh binding
//! ```
//!
//! This is `let`-as-declaration, in the Rust sense, so a second `let` of a name
//! that is *still live* is an error rather than a silent shadow:
//!
//! ```text
//! let x = input();
//! let x = input();   // error: `x` is already declared
//! ```
//!
//! A plain assignment (`x = ...`) refers to an existing variable and never
//! declares: assigning to a freed name is an error.
//!
//! ## Errors the register table catches
//!
//! * **Already declared** — `let x = 1; let x = 2;` without an intervening `free`
//! * **Use after free** — `free x; print(x);`, and `free x; let x = x + 1;`
//!   (a declaration's own initializer cannot see the name being declared)
//! * **Double free** — `free x; free x;`
//! * **Freeing what holds no memory** — a `const`, a `#data` table, or an
//!   unknown name
//! * **Running out of memories** — reported with the registers in use; an array
//!   reports how many memories it needs and how many are free
//! * **Array misuse** — indexing something that is not an array, using an array
//!   without an index, assigning to the array name instead of an element, and
//!   an index outside `0..size`
//!
//! ## `free` under re-entrant control flow: `unsafe_free`
//!
//! The walk above is a single forward pass, which assumes every statement runs
//! at most once. Neither a `goto` nor a loop satisfies that: both can re-enter
//! a region whose memory has since been released and given to another variable.
//! Jumping back to a label after a `free` re-runs code that reads a name whose
//! memory now holds something else, and a loop body does the same on its next
//! iteration — `while (c) { print(x); free x; let y = 1; }` reads `y` as `x` on
//! the second pass, because both `x` and `y` were given `A`.
//!
//! So a checked `free` inside a loop body, and a checked `free` in any program
//! containing `goto`/`label`, is an error. The way to say "I have checked this
//! myself" is `unsafe_free`, borrowing Rust's convention of making the
//! unchecked operation explicit:
//!
//! ```text
//! unsafe_free x;   // no control-flow check; still checked for double free etc.
//! ```
//!
//! Like Rust's `unsafe`, this waives one specific guarantee, not all checking:
//! `unsafe_free` still rejects double frees, unknown names and `const`s. A
//! program with jumps or loops and no `free` at all is unaffected, because then
//! nothing is ever re-used.

use std::collections::BTreeSet;

use crate::ast::{Accessor, Expr, Program, Stmt};
use crate::data::Data;
use crate::error::TranspileError;

/// The seven assignable calculator memories.
pub const VARIABLES: [char; 7] = ['A', 'B', 'C', 'D', 'X', 'Y', 'M'];

/// One variable's occupation of one memory.
///
/// `from`..`to` is the byte range over which the binding is live, so a name
/// declared twice has two bindings — and, if a `free` separated them, two
/// different memories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: String,
    /// `Some(k)` when this binding is element `k` of the array called `name`.
    pub element: Option<usize>,
    pub memory: char,
    /// Byte offset of the declaration, or of the first use when a name is used
    /// before it is declared.
    pub from: usize,
    /// Byte offset of the `free` that released it, if it was released.
    pub to: Option<usize>,
}

impl Binding {
    /// How this binding is written in reports: `a[0]` for an array element.
    pub fn label(&self) -> String {
        match self.element {
            Some(index) => format!("{}[{index}]", self.name),
            None => self.name.clone(),
        }
    }
}

/// Where every variable ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allocation {
    /// Every binding, in program order. A name freed and declared again appears
    /// more than once, and an array appears once per element.
    pub bindings: Vec<Binding>,
    /// Each memory's occupants over time, in calculator order. A memory with
    /// more than one entry was released with `free` and handed on.
    ///
    /// The full [`Binding`] is kept rather than just a label so a report can
    /// show `v[0]` for the occupant while still naming `v` in a
    /// "reused after `free v`" note.
    pub registers: Vec<(char, Vec<Binding>)>,
    /// Names released with `free`, in source order, each listed once.
    pub freed: Vec<String>,
}

impl Allocation {
    /// How many of the seven memories the program uses.
    pub fn used(&self) -> usize {
        self.registers
            .iter()
            .filter(|(_, occupants)| !occupants.is_empty())
            .count()
    }

    /// The memories the program never touches, in calculator order.
    pub fn free(&self) -> Vec<char> {
        self.registers
            .iter()
            .filter(|(_, occupants)| occupants.is_empty())
            .map(|(memory, _)| *memory)
            .collect()
    }

    /// Whether any memory was released and given to another variable.
    pub fn reuses(&self) -> impl Iterator<Item = &(char, Vec<Binding>)> {
        self.registers
            .iter()
            .filter(|(_, occupants)| occupants.len() > 1)
    }
}

/// A resolved name-to-memory table.
#[derive(Debug, Clone)]
pub struct Allocator {
    bindings: Vec<Binding>,
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
            arrays: Vec::new(),
            bindings: Vec::new(),
            occupants: [None; VARIABLES.len()],
            declaring: None,
            has_jump: false,
            first_checked_release: None,
            loop_depth: 0,
            first_loop_release: None,
            reserved: reserve_fixed(program),
            full_check: true,
        };
        collect_consts(program, &mut scanner.consts);
        scanner.stmts(program)?;
        scanner.finish()
    }

    /// Check the program's **binding validity**, without requiring memory to be
    /// available.
    ///
    /// This reports exactly the errors that do not depend on which memory a
    /// variable receives: a double free, a use after free, freeing a
    /// `const`/`#data`/unknown name, re-declaring a live name, a name used in
    /// its own initializer, a checked `free` under re-entrant control flow, and
    /// the array shape mistakes (indexing a scalar, an index out of range).
    ///
    /// It deliberately does **not** report running out of memory, because that
    /// one legitimately improves when the program is optimised: eight variables
    /// nothing reads do fit in seven memories once the stores are gone, and the
    /// tests pin that. So memory pressure is ignored here — the scanner hands
    /// out placeholder registers when it runs short — and [`Allocator::collect`]
    /// still checks it properly on the way to emitting.
    ///
    /// ## Why this exists
    ///
    /// [`crate::transpile`] runs this **before** the optimisation passes, so
    /// diagnostics are computed on the program *as written*. That is what lets
    /// the optimiser be aggressive: without it, deleting the declaration a
    /// diagnostic is about would silently legalise the program. It happened
    /// twice — `let t = 1; free t; free t;` stopped being a double-free error
    /// once the unread `t` was removed, and `let v = (v - v);` stopped being a
    /// self-reference error once `v - v` folded to `0`.
    pub fn validate(program: &Program, source: &str, data: &Data) -> Result<(), TranspileError> {
        let mut scanner = Scanner {
            source,
            data,
            consts: BTreeSet::new(),
            vars: Vec::new(),
            arrays: Vec::new(),
            bindings: Vec::new(),
            occupants: [None; VARIABLES.len()],
            declaring: None,
            has_jump: false,
            first_checked_release: None,
            loop_depth: 0,
            first_loop_release: None,
            reserved: reserve_fixed(program),
            full_check: false,
        };
        collect_consts(program, &mut scanner.consts);
        scanner.stmts(program)?;
        scanner.finish()?;
        Ok(())
    }

    /// The memory for the scalar `name` at byte offset `offset`.
    ///
    /// This is how the emitter resolves a reference: a name can have held more
    /// than one memory over the program's life, and the binding in force is the
    /// one whose byte range contains the reference.
    pub fn register_at(&self, name: &str, offset: usize) -> Option<char> {
        self.binding_at(name, None, offset)
    }

    /// The memory for element `element` of the array `name` at byte offset
    /// `offset`.
    pub fn element_at(&self, name: &str, element: usize, offset: usize) -> Option<char> {
        self.binding_at(name, Some(element), offset)
    }

    fn binding_at(&self, name: &str, element: Option<usize>, offset: usize) -> Option<char> {
        self.bindings
            .iter()
            .filter(|binding| {
                binding.name == name
                    && binding.element == element
                    && binding.from <= offset
                    && binding.to.is_none_or(|to| offset < to)
            })
            .max_by_key(|binding| binding.from)
            .map(|binding| binding.memory)
    }

    /// The memory of `name`'s last binding. Useful for reports and tests; the
    /// emitter uses [`Allocator::register_at`].
    #[allow(dead_code)] // exercised by the unit tests
    pub fn lookup(&self, name: &str) -> Option<char> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.name == name && binding.element.is_none())
            .map(|binding| binding.memory)
    }

    /// The memory of the last binding of element `element` of `name`.
    #[allow(dead_code)] // exercised by the unit tests
    pub fn lookup_element(&self, name: &str, element: usize) -> Option<char> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| binding.name == name && binding.element == Some(element))
            .map(|binding| binding.memory)
    }

    /// Number of bindings.
    #[allow(dead_code)] // exercised by the unit tests
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// True when nothing was bound.
    #[allow(dead_code)] // exercised by the unit tests
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// The full allocation, for reporting.
    pub fn allocation(&self) -> Allocation {
        let registers = (0..VARIABLES.len())
            .map(|register| {
                let names = self
                    .bindings
                    .iter()
                    .filter(|binding| binding.memory == VARIABLES[register])
                    .cloned()
                    .collect();
                (VARIABLES[register], names)
            })
            .collect();
        // An array contributes one binding per element, so dedupe the names to
        // report `free a;` once rather than once per element.
        let mut freed = Vec::new();
        for binding in &self.bindings {
            if binding.to.is_some() && !freed.contains(&binding.name) {
                freed.push(binding.name.clone());
            }
        }
        Allocation {
            bindings: self.bindings.clone(),
            registers,
            freed,
        }
    }
}

/// A variable and its current binding.
struct Var {
    name: String,
    /// `Some(k)` when this is element `k` of the array called `name`.
    element: Option<usize>,
    /// Index into [`VARIABLES`].
    register: usize,
    /// Index into the scanner's `bindings`.
    binding: usize,
    /// Cleared by `free`.
    live: bool,
    /// Whether a `let` declared it.
    ///
    /// A name used before it is declared is allocated on first sight — `.fxc`
    /// has always allowed that, which is what keeps `let a = b; let b = 1;`
    /// working. Such a variable is *implicit*, and a later `let` adopts it
    /// rather than being a second declaration. Only two explicit declarations
    /// of one live name conflict.
    explicit: bool,
}

/// A declared array and the variables holding its elements.
struct ArrayVar {
    name: String,
    size: usize,
    /// Indices into [`Scanner::vars`], element 0 first.
    elements: Vec<usize>,
    /// Cleared by `free`, which releases every element at once.
    live: bool,
}

struct Scanner<'a> {
    source: &'a str,
    data: &'a Data,
    /// Names declared with `const`; they consume no memory.
    consts: BTreeSet<String>,
    /// Variables in declaration order. A name freed and declared again gets a
    /// second entry.
    vars: Vec<Var>,
    /// Arrays in declaration order.
    arrays: Vec<ArrayVar>,
    bindings: Vec<Binding>,
    /// The register table: which variable occupies each memory, if any.
    occupants: [Option<usize>; VARIABLES.len()],
    /// The name whose initializer is being walked, so `let x = x + 1` can be
    /// rejected: a declaration does not introduce its name until after its
    /// initializer.
    declaring: Option<String>,
    has_jump: bool,
    /// Offset of the first `free` that was *not* `unsafe_free`.
    first_checked_release: Option<usize>,
    /// How many loop bodies enclose the statement being walked. A `free` inside
    /// one needs `unsafe_free` for the same reason a `goto` does: the loop can
    /// re-run code whose memory has since been given to another variable.
    loop_depth: usize,
    /// Offset of the first checked `free` seen inside a loop body.
    first_loop_release: Option<usize>,
    /// Memories the program addresses by their fixed PRGM letter (`M`), and
    /// which must therefore stay out of the allocator's pool.
    reserved: [bool; VARIABLES.len()],
    /// Whether this is the full check that the emitter needs, as opposed to
    /// [`Allocator::validate`]'s early validity check.
    ///
    /// The full check is strict about two things a *later* pass may still
    /// resolve, so the early one tolerates both:
    ///
    /// * **Memory pressure.** Eight variables nothing reads do fit in seven
    ///   memories once the stores are gone, so the early check hands out
    ///   placeholder registers instead of failing.
    /// * **A computed array index.** `m[i]` only becomes a literal once
    ///   [`crate::unroll`] has expanded the loop, and the early check runs
    ///   before that, so it leaves the index alone for the full check to
    ///   report.
    full_check: bool,
}

impl Scanner<'_> {
    // -- lookups ------------------------------------------------------------

    /// The live scalar called `name`, if there is one.
    fn live_var(&self, name: &str) -> Option<usize> {
        self.vars
            .iter()
            .rposition(|var| var.name == name && var.element.is_none() && var.live)
    }

    /// Whether `name` has ever been a scalar, live or released.
    fn seen_var(&self, name: &str) -> bool {
        self.vars
            .iter()
            .any(|var| var.name == name && var.element.is_none())
    }

    /// The live array called `name`, if there is one.
    fn live_array(&self, name: &str) -> Option<usize> {
        self.arrays
            .iter()
            .rposition(|array| array.name == name && array.live)
    }

    /// Whether `name` has ever been an array, live or released.
    fn seen_array(&self, name: &str) -> bool {
        self.arrays.iter().any(|array| array.name == name)
    }

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

    // -- binding ------------------------------------------------------------

    /// Give the scalar `name` a memory, starting at `pos`.
    fn allocate(&mut self, name: &str, pos: usize, explicit: bool) -> Result<(), TranspileError> {
        // The lowest free memory keeps the allocation deterministic and easy to
        // read, and naturally re-uses one that was just released.
        let register = match (0..VARIABLES.len())
            .find(|register| self.occupants[*register].is_none() && !self.reserved[*register])
        {
            Some(register) => register,
            // Validity checking does not care where a variable would live, and
            // the `Binding` records the memory directly, so re-using register 0
            // as a placeholder cannot make a later check report something
            // false. `occupants` only decides which register is chosen next and
            // what an out-of-memory message says.
            None if !self.full_check => 0,
            None => return Err(self.out_of_memory(name, pos)),
        };
        self.bindings.push(Binding {
            name: name.to_string(),
            element: None,
            memory: VARIABLES[register],
            from: pos,
            to: None,
        });
        self.occupants[register] = Some(self.vars.len());
        self.vars.push(Var {
            name: name.to_string(),
            element: None,
            register,
            binding: self.bindings.len() - 1,
            live: true,
            explicit,
        });
        Ok(())
    }

    /// Give the array `name` `size` consecutive memories, starting at `pos`.
    ///
    /// Memories are chosen low-first, like a scalar's, so a program that
    /// declares an array before anything else sees it occupy `A B C …`. The
    /// first memory in use is the first error in `out_of_memory`, so this is
    /// easy to explain in a report.
    fn allocate_array(
        &mut self,
        name: &str,
        size: usize,
        pos: usize,
    ) -> Result<(), TranspileError> {
        let available: Vec<usize> = (0..VARIABLES.len())
            .filter(|register| self.occupants[*register].is_none() && !self.reserved[*register])
            .collect();
        if available.len() < size && self.full_check {
            return Err(self.no_room_for_array(name, size, pos, &available));
        }
        // When only validity matters, an array larger than the free memories
        // still has to be given *some* registers so the elements can be looked
        // up by name; they are placeholders.
        let chosen: Vec<usize> = available
            .iter()
            .copied()
            .chain(std::iter::repeat_n(0, size))
            .take(size)
            .collect();
        let mut elements = Vec::with_capacity(size);
        for (element, register) in chosen.into_iter().enumerate() {
            self.bindings.push(Binding {
                name: name.to_string(),
                element: Some(element),
                memory: VARIABLES[register],
                from: pos,
                to: None,
            });
            self.occupants[register] = Some(self.vars.len());
            self.vars.push(Var {
                name: name.to_string(),
                element: Some(element),
                register,
                binding: self.bindings.len() - 1,
                live: true,
                explicit: true,
            });
            elements.push(self.vars.len() - 1);
        }
        self.arrays.push(ArrayVar {
            name: name.to_string(),
            size,
            elements,
            live: true,
        });
        Ok(())
    }

    /// Resolve a *read* of the scalar `name`.
    ///
    /// Reading a `const` or `#data` name is fine — the emitter replaces it with
    /// its value — so only real variables reach the register table. A name seen
    /// for the very first time is declared here, which is what keeps `.fxc`
    /// order-free for simple programs.
    fn read(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        if self.consts.contains(name) || self.data.contains(name) {
            return Ok(());
        }
        if self.declaring.as_deref() == Some(name) {
            return Err(own_initializer(self.source, name, pos));
        }
        if self.live_var(name).is_some() {
            return Ok(());
        }
        if self.live_array(name).is_some() {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is an array; index it, as in `{name}[0]`"),
                pos,
            ));
        }
        if self.seen_var(name) || self.seen_array(name) {
            return Err(use_after_free(
                self.source,
                name,
                pos,
                self.seen_array(name) && !self.seen_var(name),
            ));
        }
        self.allocate(name, pos, false)
    }

    /// Resolve an *assignment target* (`x = ...`, and a `for` variable).
    ///
    /// Assignment never declares: only `let`/`const` do. So assigning to a
    /// released name is an error, while assigning to a name never seen before
    /// declares it, as `.fxc` has always allowed.
    fn assign(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        self.reject_compile_time(name, pos)?;
        if self.declaring.as_deref() == Some(name) {
            return Err(own_initializer(self.source, name, pos));
        }
        if self.live_var(name).is_some() {
            return Ok(());
        }
        if self.live_array(name).is_some() {
            return Err(TranspileError::at(
                self.source,
                format!("`{name}` is an array; assign to an element, as in `{name}[0] = ...`"),
                pos,
            ));
        }
        if self.seen_var(name) || self.seen_array(name) {
            return Err(use_after_free(
                self.source,
                name,
                pos,
                self.seen_array(name) && !self.seen_var(name),
            ));
        }
        self.allocate(name, pos, false)
    }

    /// Start a `let` declaration: take a memory, and check the name is free to
    /// declare.
    ///
    /// The name is bound *before* its initializer is walked, so allocation
    /// follows source order: in `let a = -b * c;` the memories go to `a`, `b`,
    /// `c`. A name released earlier gets a fresh binding — possibly into the
    /// very memory its previous life released.
    ///
    /// Only an existing *explicit* declaration is a conflict. A name that was
    /// merely used earlier was allocated on first sight but never declared, so
    /// this `let` is its declaration and adopts that binding.
    fn begin_declaration(&mut self, name: &str, pos: usize) -> Result<(), TranspileError> {
        self.reject_compile_time(name, pos)?;
        if self.live_array(name).is_some() {
            return Err(already_declared(self.source, name, pos));
        }
        match self.live_var(name) {
            Some(index) => {
                if self.vars[index].explicit {
                    return Err(already_declared(self.source, name, pos));
                }
                self.vars[index].explicit = true;
            }
            None => self.allocate(name, pos, true)?,
        }
        // Set last, so the initializer can be checked against it.
        self.declaring = Some(name.to_string());
        Ok(())
    }

    /// Start a `let name[size]` declaration.
    ///
    /// Unlike a scalar, an array must not adopt an existing implicit variable:
    /// `print(a); let a[3];` would silently change what `a` means. And the
    /// entire array is allocated before its initialisers are walked, so a
    /// literal table ends up in consecutive memories (`A B C …`).
    fn begin_array_declaration(
        &mut self,
        name: &str,
        size: usize,
        pos: usize,
    ) -> Result<(), TranspileError> {
        self.reject_compile_time(name, pos)?;
        if self.live_array(name).is_some() {
            return Err(already_declared(self.source, name, pos));
        }
        if self.live_var(name).is_some() {
            return Err(TranspileError::at(
                self.source,
                format!(
                    "`{name}` is already declared as a single variable; an array needs a \
                     fresh name, or `free {name};` first"
                ),
                pos,
            ));
        }
        self.allocate_array(name, size, pos)?;
        self.declaring = Some(name.to_string());
        Ok(())
    }

    /// Finish a declaration, after its initializer has been walked.
    fn end_declaration(&mut self, _name: &str, _pos: usize) -> Result<(), TranspileError> {
        self.declaring = None;
        Ok(())
    }

    /// Resolve `name[index] = ...`.
    fn assign_element(
        &mut self,
        name: &str,
        index: usize,
        pos: usize,
    ) -> Result<(), TranspileError> {
        self.reject_compile_time(name, pos)?;
        if self.declaring.as_deref() == Some(name) {
            return Err(own_initializer(self.source, name, pos));
        }
        let Some(array) = self.live_array(name) else {
            return Err(self.instead_of_array(name, pos));
        };
        self.check_bounds(name, index, array, pos)
    }

    /// Resolve a *read* of `name[index]` (or any other indexed path).
    ///
    /// `a[0]` is an array element. `c.a`, `c.list[0]` and `c[0][1]` are
    /// compile-time `#data` paths, which the emitter resolves to a number. The
    /// two share one syntax and one AST node, so the distinction is made here,
    /// where the declared arrays and the data tables are both known.
    fn indexed(
        &mut self,
        name: &str,
        accessors: &[Accessor],
        pos: usize,
    ) -> Result<(), TranspileError> {
        // A `#data` table wins: it is resolved to a number, not a memory.
        if self.data.contains(name) {
            return Ok(());
        }
        if let [Accessor::Index { index, pos: at }] = accessors {
            let index = *index;
            let at = *at;
            if self.declaring.as_deref() == Some(name) {
                return Err(own_initializer(self.source, name, at));
            }
            let Some(array) = self.live_array(name) else {
                return Err(self.instead_of_array(name, pos));
            };
            return self.check_bounds(name, index, array, at);
        }
        // A computed index is a run-time value, which PRGM cannot address — but
        // only the *full* check may say so: the early one runs before
        // [`crate::unroll`], which is what turns `m[i]` in a constant loop into
        // `m[0]`…`m[5]`.
        if let [Accessor::IndexExpr { pos: at, .. }] = accessors {
            if !self.full_check {
                return Ok(());
            }
            return Err(crate::error::computed_index_error(self.source, name, *at));
        }
        // Not a plain element reference. An array has no fields, and a scalar
        // has nothing to index at all.
        if self.live_var(name).is_some() || self.seen_var(name) {
            return Err(self.not_an_array(name, pos));
        }
        if self.live_array(name).is_some() || self.seen_array(name) {
            return Err(TranspileError::at(
                self.source,
                format!(
                    "array elements are numbers, so `{name}[…]` cannot be followed by `.field` \
                     or a second index; only a `#data` table has nested values"
                ),
                pos,
            ));
        }
        Err(TranspileError::at(
            self.source,
            format!("unknown data table `{name}`"),
            pos,
        ))
    }

    /// Report an index outside `0..size`.
    fn check_bounds(
        &self,
        name: &str,
        index: usize,
        array: usize,
        pos: usize,
    ) -> Result<(), TranspileError> {
        let size = self.arrays[array].size;
        if index >= size {
            return Err(TranspileError::at(
                self.source,
                format!("index {index} is out of range for `{name}` (length {size})"),
                pos,
            ));
        }
        Ok(())
    }

    /// Report indexing a name that is not a declared array.
    fn instead_of_array(&self, name: &str, pos: usize) -> TranspileError {
        if self.live_array(name).is_some() {
            // Unreachable: the caller checked. Kept for totality's sake.
            return TranspileError::at(self.source, format!("`{name}` is an array"), pos);
        }
        if self.seen_array(name) {
            return use_after_free(self.source, name, pos, true);
        }
        self.not_an_array(name, pos)
    }

    fn not_an_array(&self, name: &str, pos: usize) -> TranspileError {
        let message = if self.live_var(name).is_some() || self.seen_var(name) {
            format!("`{name}` is not an array; it holds a single value")
        } else {
            format!(
                "`{name}` is not declared; declare it with `let {name}[N]` to index it, or use \
                 a `#data` table"
            )
        };
        TranspileError::at(self.source, message, pos)
    }

    /// `free name;` and `unsafe_free name;`.
    ///
    /// Freeing an array releases every element at once. There is deliberately
    /// no way to free one element: the memories in the middle of an array would
    /// be stranded, since the elements after it still need their own.
    fn free(&mut self, name: &str, pos: usize, is_unsafe: bool) -> Result<(), TranspileError> {
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

        if let Some(array) = self.live_array(name) {
            let elements = self.arrays[array].elements.clone();
            for var in elements {
                let register = self.vars[var].register;
                let binding = self.vars[var].binding;
                self.vars[var].live = false;
                self.occupants[register] = None;
                self.bindings[binding].to = Some(pos);
            }
            self.arrays[array].live = false;
        } else {
            let Some(index) = self.live_var(name) else {
                let message = if self.seen_var(name) || self.seen_array(name) {
                    format!(
                        "`free {name}`: `{name}` was already freed (double free); its memory has \
                         been given to another variable"
                    )
                } else {
                    format!("`free {name}`: `{name}` is not a variable")
                };
                return Err(TranspileError::at(self.source, message, pos));
            };
            let register = self.vars[index].register;
            let binding = self.vars[index].binding;
            self.vars[index].live = false;
            self.occupants[register] = None;
            self.bindings[binding].to = Some(pos);
        }

        if !is_unsafe {
            self.first_checked_release.get_or_insert(pos);
            if self.loop_depth > 0 {
                self.first_loop_release.get_or_insert(pos);
            }
        }
        Ok(())
    }

    /// No memory is available for the scalar `name`.
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

    /// Not enough memories for an array that needs `size` of them.
    fn no_room_for_array(
        &self,
        name: &str,
        size: usize,
        pos: usize,
        available: &[usize],
    ) -> TranspileError {
        let free_text = if available.is_empty() {
            "none are free".to_string()
        } else {
            format!(
                "only {} {} free ({})",
                available.len(),
                if available.len() == 1 { "is" } else { "are" },
                available
                    .iter()
                    .map(|register| VARIABLES[*register].to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        let holders = self.holders();
        // Avoid a dangling "are in use by ." when nothing is actually taken.
        let occupied = if holders.is_empty() {
            String::new()
        } else {
            let used = self.occupants.iter().filter(|o| o.is_some()).count();
            format!("; {used} of 7 are in use by {holders}")
        };
        TranspileError::at(
            self.source,
            format!(
                "no room for array `{name}`: it needs {size} memories but {free_text}{occupied}. \
                 Use `const` for fixed values, or `free` a variable you no longer need — \
                 `fx50 regs` shows the plan",
            ),
            pos,
        )
    }

    /// The live occupants of each memory, for an out-of-memory message.
    ///
    /// An array is listed once, as `A B C (`v[…]`)`, rather than once per
    /// element, so a full memory does not produce seven near-identical clauses.
    fn holders(&self) -> String {
        let mut holders: Vec<String> = Vec::new();
        let mut grouped: BTreeSet<String> = BTreeSet::new();
        for (register, occupant) in self.occupants.iter().enumerate() {
            let Some(var) = occupant.map(|index| &self.vars[index]) else {
                continue;
            };
            match var.element {
                None => holders.push(format!("{} (`{}`)", VARIABLES[register], var.name)),
                Some(_) => {
                    if grouped.insert(var.name.clone()) {
                        let memories: Vec<String> = self
                            .occupants
                            .iter()
                            .enumerate()
                            .filter(|(_, occupant)| {
                                occupant.is_some_and(|index| {
                                    let other = &self.vars[index];
                                    other.name == var.name && other.element.is_some()
                                })
                            })
                            .map(|(register, _)| VARIABLES[register].to_string())
                            .collect();
                        holders.push(format!("{} (`{}[…]`)", memories.join(" "), var.name));
                    }
                }
            }
        }
        holders.join(", ")
    }

    // -- walking ------------------------------------------------------------

    fn stmts(&mut self, stmts: &[Stmt]) -> Result<(), TranspileError> {
        for stmt in stmts {
            self.stmt(stmt)?;
        }
        Ok(())
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), TranspileError> {
        match stmt {
            Stmt::Let { name, value, pos } => {
                self.begin_declaration(name, *pos)?;
                self.expr(value)?;
                self.end_declaration(name, *pos)
            }
            Stmt::LetArray {
                name,
                size,
                values,
                pos,
            } => {
                self.begin_array_declaration(name, *size, *pos)?;
                for value in values {
                    self.expr(value)?;
                }
                self.end_declaration(name, *pos)
            }
            Stmt::Const { value, .. } => self.expr(value),
            Stmt::Assign { name, value, pos } => {
                self.assign(name, *pos)?;
                self.expr(value)
            }
            Stmt::AssignElement {
                name,
                index,
                value,
                pos,
            } => {
                self.assign_element(name, *index, *pos)?;
                self.expr(value)
            }
            // A computed element assignment should have been unrolled or
            // folded away; reaching one means the index is a run-time value,
            // which PRGM cannot address.
            Stmt::AssignElementExpr {
                name,
                index: _,
                value: _,
                pos,
            } => {
                if !self.full_check {
                    return Ok(());
                }
                Err(crate::error::computed_index_error(self.source, name, *pos))
            }
            Stmt::Free {
                name,
                pos,
                is_unsafe,
            } => self.free(name, *pos, *is_unsafe),
            // `mplus(x)`/`mminus(x)` read their operand but allocate nothing.
            Stmt::Memory { value, .. } => self.expr(value),
            // Setup and clear keys mention no names.
            Stmt::Setup { .. } | Stmt::ClrMemory | Stmt::ClrStat | Stmt::FreqOn | Stmt::FreqOff => {
                Ok(())
            }
            Stmt::Data { x, y, freq, .. } => {
                self.expr(x)?;
                if let Some(y) = y {
                    self.expr(y)?;
                }
                if let Some(freq) = freq {
                    self.expr(freq)?;
                }
                Ok(())
            }
            Stmt::CondJump { cond, target, .. } => {
                self.expr(cond)?;
                self.stmt(target)
            }
            Stmt::Print(expr) | Stmt::ExprStmt(expr) => self.expr(expr),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.expr(cond)?;
                self.stmts(then_body)?;
                self.stmts(else_body)
            }
            Stmt::While { cond, body } => {
                self.expr(cond)?;
                self.loop_depth += 1;
                let body_result = self.stmts(body);
                self.loop_depth -= 1;
                body_result
            }
            Stmt::For(for_stmt) => {
                if for_stmt.is_decl {
                    self.begin_declaration(&for_stmt.init_name, for_stmt.pos)?;
                    self.expr(&for_stmt.init_value)?;
                    self.end_declaration(&for_stmt.init_name, for_stmt.pos)?;
                } else {
                    self.assign(&for_stmt.init_name, for_stmt.pos)?;
                    self.expr(&for_stmt.init_value)?;
                }
                self.expr(&for_stmt.cond)?;
                self.loop_depth += 1;
                let body_result = self.stmts(&for_stmt.body);
                self.loop_depth -= 1;
                body_result?;
                self.assign(&for_stmt.update_name, for_stmt.pos)?;
                self.expr(&for_stmt.update_value)
            }
            Stmt::Block(stmts) => self.stmts(stmts),
            Stmt::Break => Ok(()),
            Stmt::Goto(..) | Stmt::Label(..) => {
                self.has_jump = true;
                Ok(())
            }
            // Function expansion runs before allocation, so these never appear;
            // walking the value keeps the pass total.
            Stmt::Return { value, .. } => match value {
                Some(value) => self.expr(value),
                None => Ok(()),
            },
            Stmt::Function(_) => Ok(()),
            Stmt::Empty => Ok(()),
        }
    }

    /// Walk an expression, resolving every name it mentions.
    fn expr(&mut self, expr: &Expr) -> Result<(), TranspileError> {
        match expr {
            Expr::Name(name, pos) => self.read(name, *pos),
            // A data path is a compile-time number; an indexed name may be
            // either that or an array element, which `indexed` decides.
            Expr::Data {
                name,
                accessors,
                pos,
            } => self.indexed(name, accessors, *pos),
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
            Expr::Number(_)
            | Expr::BaseLiteral { .. }
            | Expr::Pi(_)
            | Expr::E(_)
            | Expr::Ans(_)
            | Expr::StatVar(..)
            | Expr::Constant(..)
            | Expr::Input(_) => Ok(()),
        }
    }

    fn finish(self) -> Result<Allocator, TranspileError> {
        // A jump can re-enter a region whose memory has since been released and
        // re-used, which a single forward walk cannot describe.
        if let (true, Some(pos)) = (self.has_jump, self.first_checked_release) {
            return Err(TranspileError::at(
                self.source,
                "`free` cannot be used in a program containing `goto`/`label`: a jump can \
                 re-enter code whose memory has since been re-used. Use `unsafe_free` if \
                 you have checked that it cannot",
                pos,
            ));
        }
        // A loop re-enters its body, so a `free` inside one has the same
        // problem as a jump: the memory may be re-used and then read again on
        // the next iteration.
        if let Some(pos) = self.first_loop_release {
            return Err(TranspileError::at(
                self.source,
                "`free` cannot be used inside a loop: the loop can re-enter code whose memory \
                 has since been re-used. Use `unsafe_free` if you have checked that it cannot",
                pos,
            ));
        }
        Ok(Allocator {
            bindings: self.bindings,
        })
    }
}

/// A reference to a name whose memory has already been released.
///
/// `was_array` picks the hint: an array is declared again with a size, a
/// scalar without one.
fn use_after_free(source: &str, name: &str, pos: usize, was_array: bool) -> TranspileError {
    let redeclare = if was_array {
        format!("`let {name}[N] = ...`")
    } else {
        format!("`let {name} = ...`")
    };
    TranspileError::at(
        source,
        format!(
            "`{name}` is not defined here: it was freed, and its memory may now hold another \
             variable. Use {redeclare} to declare it again"
        ),
        pos,
    )
}

/// A name used inside its own declaration.
fn own_initializer(source: &str, name: &str, pos: usize) -> TranspileError {
    TranspileError::at(
        source,
        format!(
            "`{name}` cannot be used in its own initializer; a declaration does not \
             take effect until after the value is computed"
        ),
        pos,
    )
}

/// Declaring a name that is still live.
fn already_declared(source: &str, name: &str, pos: usize) -> TranspileError {
    TranspileError::at(
        source,
        format!(
            "`{name}` is already declared; `free {name};` first if you mean to reuse the \
             name, or pick a different one"
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
            // A `=>` target could name a `const`, though the emitter rejects
            // guarding one.
            Stmt::CondJump { target, .. } => collect_consts(std::slice::from_ref(target), out),
            Stmt::Let { .. }
            | Stmt::LetArray { .. }
            | Stmt::Assign { .. }
            | Stmt::AssignElement { .. }
            | Stmt::AssignElementExpr { .. }
            | Stmt::Free { .. }
            | Stmt::Memory { .. }
            | Stmt::Setup { .. }
            | Stmt::ClrMemory
            | Stmt::ClrStat
            | Stmt::FreqOn
            | Stmt::FreqOff
            | Stmt::Data { .. }
            | Stmt::Print(_)
            | Stmt::ExprStmt(_)
            | Stmt::Break
            | Stmt::Goto(..)
            | Stmt::Label(..)
            | Stmt::Empty
            | Stmt::Function(_)
            | Stmt::Return { .. } => {}
        }
    }
}

/// The `const` names a program declares, sorted, for reports.
pub fn const_names(program: &Program) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_consts(program, &mut names);
    names.into_iter().collect()
}

/// Which fixed PRGM memories the program addresses by letter.
///
/// `mplus`/`mminus`/`mvalue` touch the calculator's fixed `M` memory, so the
/// allocator must not hand `M` to a `.fxc` variable: the two would silently
/// share it.
fn reserve_fixed(program: &Program) -> [bool; VARIABLES.len()] {
    let mut reserved = [false; VARIABLES.len()];
    for stmt in program {
        scan_fixed_stmt(stmt, &mut reserved);
    }
    reserved
}

fn scan_fixed_stmt(stmt: &Stmt, reserved: &mut [bool; VARIABLES.len()]) {
    match stmt {
        Stmt::Memory { value, .. } => {
            reserved[memory_index('M')] = true;
            scan_fixed_expr(value, reserved);
        }
        Stmt::Let { value, .. }
        | Stmt::Const { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::AssignElement { value, .. }
        | Stmt::ExprStmt(value)
        | Stmt::Print(value) => scan_fixed_expr(value, reserved),
        Stmt::AssignElementExpr { index, value, .. } => {
            scan_fixed_expr(index, reserved);
            scan_fixed_expr(value, reserved);
        }
        Stmt::LetArray { values, .. } => {
            for value in values {
                scan_fixed_expr(value, reserved);
            }
        }
        Stmt::Data { x, y, freq, .. } => {
            scan_fixed_expr(x, reserved);
            if let Some(y) = y {
                scan_fixed_expr(y, reserved);
            }
            if let Some(freq) = freq {
                scan_fixed_expr(freq, reserved);
            }
        }
        Stmt::CondJump { cond, target, .. } => {
            scan_fixed_expr(cond, reserved);
            scan_fixed_stmt(target, reserved);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            scan_fixed_expr(cond, reserved);
            for stmt in then_body.iter().chain(else_body) {
                scan_fixed_stmt(stmt, reserved);
            }
        }
        Stmt::While { cond, body } => {
            scan_fixed_expr(cond, reserved);
            for stmt in body {
                scan_fixed_stmt(stmt, reserved);
            }
        }
        Stmt::For(for_stmt) => {
            scan_fixed_expr(&for_stmt.init_value, reserved);
            scan_fixed_expr(&for_stmt.cond, reserved);
            scan_fixed_expr(&for_stmt.update_value, reserved);
            for stmt in &for_stmt.body {
                scan_fixed_stmt(stmt, reserved);
            }
        }
        Stmt::Block(stmts) => {
            for stmt in stmts {
                scan_fixed_stmt(stmt, reserved);
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
        | Stmt::Empty
        | Stmt::Function(_)
        | Stmt::Return { .. } => {}
    }
}

fn scan_fixed_expr(expr: &Expr, reserved: &mut [bool; VARIABLES.len()]) {
    match expr {
        Expr::Call(name, args, _) => {
            if name == "mvalue" {
                reserved[memory_index('M')] = true;
            }
            for arg in args {
                scan_fixed_expr(arg, reserved);
            }
        }
        Expr::Unary(_, inner) => scan_fixed_expr(inner, reserved),
        Expr::Binary(_, left, right) => {
            scan_fixed_expr(left, reserved);
            scan_fixed_expr(right, reserved);
        }
        Expr::Data { accessors, .. } => {
            for accessor in accessors {
                if let Accessor::IndexExpr { expr, .. } = accessor {
                    scan_fixed_expr(expr, reserved);
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

/// The index of `letter` in [`VARIABLES`].
fn memory_index(letter: char) -> usize {
    VARIABLES
        .iter()
        .position(|memory| *memory == letter)
        .expect("the fixed letter is one of the seven memories")
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
    fn assignment_to_a_live_name_reuses_it() {
        let a = alloc("let a = 1; a = a + 1; print(a);").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a.allocation().used(), 1);
    }

    #[test]
    fn two_names_never_share_without_a_free() {
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

    // -- let declares -------------------------------------------------------

    #[test]
    fn let_declares_again_after_a_free() {
        let a = alloc("let x = input(); free x; let x = input();").unwrap();
        assert_eq!(a.lookup("x"), Some('A'), "the fresh binding reuses A");
        assert_eq!(a.len(), 2, "two bindings for the same name");
        assert_eq!(a.allocation().used(), 1);
    }

    #[test]
    fn let_of_a_live_name_is_an_error() {
        let err = alloc("let x = 1; let x = 2;").unwrap_err();
        assert!(err.message.contains("already declared"), "{}", err.message);
        assert!(err.message.contains("free x"), "{}", err.message);
    }

    #[test]
    fn const_of_a_live_variable_name_is_an_error() {
        let err = alloc("let x = 1; const x = 2;").unwrap_err();
        assert!(
            err.message.contains("`const`") || err.message.contains("already declared"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_name_cannot_be_used_in_its_own_initializer() {
        let err = alloc("let x = 1; free x; let x = x + 1;").unwrap_err();
        assert!(
            err.message.contains("its own initializer"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_name_cannot_be_used_in_its_own_initializer_when_new() {
        let err = alloc("let x = x + 1;").unwrap_err();
        assert!(
            err.message.contains("its own initializer"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_freed_name_is_revived_by_let_and_gets_a_fresh_binding() {
        let a = alloc("let t = 1; free t; let t = 2; print(t);").unwrap();
        // The first `t` is closed at the free; the reference resolves to the
        // second binding.
        let allocation = a.allocation();
        assert_eq!(allocation.bindings.len(), 2);
        assert_eq!(allocation.bindings[0].from, 4);
        // `to` is the offset of the `free` statement itself.
        assert_eq!(allocation.bindings[0].to, Some(11), "closed by `free t`");
        assert_eq!(allocation.bindings[1].from, 23);
        assert_eq!(allocation.bindings[1].to, None);
        assert_eq!(a.register_at("t", 35), Some('A'));
    }

    // -- free ---------------------------------------------------------------

    #[test]
    fn a_free_lets_a_later_variable_reuse_the_memory() {
        let a = alloc("let t = 1; print(t); free t; let u = 2; print(u);").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
        assert_eq!(a.lookup("u"), Some('A'), "u reuses the freed memory");
        assert_eq!(a.allocation().used(), 1);
        assert_eq!(a.allocation().freed, vec!["t"]);
        let allocation = a.allocation();
        let reuses: Vec<&(char, Vec<Binding>)> = allocation.reuses().collect();
        assert_eq!(reuses.len(), 1);
        let names: Vec<&str> = reuses[0].1.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["t", "u"]);
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
    fn double_free_is_an_error() {
        let err = alloc("let t = 1; free t; free t;").unwrap_err();
        assert!(err.message.contains("double free"), "{}", err.message);
    }

    #[test]
    fn use_after_free_is_an_error_when_reading() {
        let err = alloc("let t = 1; free t; print(t);").unwrap_err();
        assert!(err.message.contains("not defined here"), "{}", err.message);
    }

    #[test]
    fn use_after_free_is_an_error_when_assigning() {
        let err = alloc("let t = 1; free t; t = 2;").unwrap_err();
        assert!(err.message.contains("not defined here"), "{}", err.message);
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

    // -- free and jumps -----------------------------------------------------

    #[test]
    fn a_checked_free_with_a_jump_is_rejected() {
        let err = alloc("let t = 1; free t; goto 1; label 1;").unwrap_err();
        assert!(err.message.contains("`goto`/`label`"), "{}", err.message);
        assert!(err.message.contains("unsafe_free"), "{}", err.message);
    }

    #[test]
    fn unsafe_free_is_allowed_with_a_jump() {
        // Every release in a program with jumps must say `unsafe_free`.
        let a =
            alloc("let t = 1; unsafe_free t; let u = 2; unsafe_free u; goto 1; label 1;").unwrap();
        assert_eq!(a.allocation().freed, vec!["t", "u"]);
    }

    #[test]
    fn one_checked_free_is_enough_to_reject_a_jump_program() {
        let err =
            alloc("let t = 1; unsafe_free t; let u = 2; free u; goto 1; label 1;").unwrap_err();
        assert!(err.message.contains("`goto`/`label`"), "{}", err.message);
    }

    #[test]
    fn unsafe_free_still_checks_the_name() {
        let err = alloc("unsafe_free nope;").unwrap_err();
        assert!(err.message.contains("is not a variable"), "{}", err.message);
    }

    #[test]
    fn unsafe_free_still_rejects_a_double_free() {
        let err = alloc("let t = 1; free t; unsafe_free t;").unwrap_err();
        assert!(err.message.contains("double free"), "{}", err.message);
    }

    #[test]
    fn a_jump_without_free_is_still_fine() {
        let a = alloc("let t = 1; goto 1; label 1; print(t);").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
    }

    #[test]
    fn free_inside_a_loop_body_is_rejected() {
        let err = alloc("while (1 < 2) { let t = 1; print(t); free t; }").unwrap_err();
        assert!(err.message.contains("inside a loop"), "{}", err.message);
        assert!(err.message.contains("unsafe_free"), "{}", err.message);
    }

    #[test]
    fn free_inside_a_for_body_is_rejected() {
        let err = alloc("for (let i = 0; i < 3; i = i + 1) { let t = 1; free t; }").unwrap_err();
        assert!(err.message.contains("inside a loop"), "{}", err.message);
        assert!(err.message.contains("unsafe_free"), "{}", err.message);
    }

    #[test]
    fn unsafe_free_inside_a_loop_body_is_allowed() {
        let a = alloc("while (1 < 2) { let t = 1; print(t); unsafe_free t; }").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
    }

    #[test]
    fn free_outside_a_loop_is_allowed_even_when_the_program_loops() {
        let a =
            alloc("let t = 1; free t; let u = 2; while (u < 3) { u = u + 1; } print(u);").unwrap();
        assert_eq!(a.lookup("t"), Some('A'));
        assert_eq!(a.lookup("u"), Some('A'), "u reuses the freed memory");
    }

    // -- arrays -------------------------------------------------------------

    #[test]
    fn an_array_occupies_consecutive_memories() {
        let a = alloc("let v[3] = {1, 2, 3};").unwrap();
        assert_eq!(a.lookup_element("v", 0), Some('A'));
        assert_eq!(a.lookup_element("v", 1), Some('B'));
        assert_eq!(a.lookup_element("v", 2), Some('C'));
        assert_eq!(a.allocation().used(), 3);
        assert_eq!(a.len(), 3, "one binding per element");
    }

    #[test]
    fn an_array_declared_late_starts_at_the_first_free_memory() {
        let a = alloc("let x = 1; let v[2] = {1, 2};").unwrap();
        assert_eq!(a.lookup("x"), Some('A'));
        assert_eq!(a.lookup_element("v", 0), Some('B'));
        assert_eq!(a.lookup_element("v", 1), Some('C'));
    }

    #[test]
    fn a_single_element_uses_one_memory() {
        let a = alloc("let v[1] = {7}; print(v[0]);").unwrap();
        assert_eq!(a.lookup_element("v", 0), Some('A'));
        assert_eq!(a.allocation().used(), 1);
    }

    #[test]
    fn a_seven_element_array_fills_every_memory() {
        let a = alloc("let v[7] = {1,2,3,4,5,6,7}; print(v[6]);").unwrap();
        assert_eq!(a.lookup_element("v", 6), Some('M'));
        assert!(a.allocation().free().is_empty());
    }

    #[test]
    fn eight_elements_do_not_fit() {
        let err = alloc("let v[8] = {1,2,3,4,5,6,7,8};").unwrap_err();
        assert!(err.message.contains("needs 8 memories"), "{}", err.message);
        assert!(err.message.contains("only 7"), "{}", err.message);
    }

    #[test]
    fn an_array_that_does_not_fit_reports_how_many_are_free() {
        let err =
            alloc("let a=1; let b=1; let c=1; let d=1; let e=1; let v[3] = {1,2,3};").unwrap_err();
        assert!(
            err.message.contains("no room for array `v`"),
            "{}",
            err.message
        );
        assert!(err.message.contains("only 2 are free"), "{}", err.message);
        assert!(err.message.contains("(Y M)"), "{}", err.message);
    }

    #[test]
    fn an_index_out_of_range_is_an_error() {
        let err = alloc("let v[3] = {1,2,3}; print(v[3]);").unwrap_err();
        assert!(
            err.message
                .contains("index 3 is out of range for `v` (length 3)"),
            "{}",
            err.message
        );
    }

    #[test]
    fn an_index_out_of_range_is_an_error_when_assigning() {
        let err = alloc("let v[2] = {1,2}; v[2] = 9;").unwrap_err();
        assert!(err.message.contains("out of range"), "{}", err.message);
    }

    #[test]
    fn indexing_a_scalar_is_an_error() {
        let err = alloc("let x = 1; print(x[0]);").unwrap_err();
        assert!(err.message.contains("is not an array"), "{}", err.message);
    }

    #[test]
    fn indexing_an_undeclared_name_suggests_let() {
        let err = alloc("print(v[0]);").unwrap_err();
        assert!(err.message.contains("`let v[N]`"), "{}", err.message);
    }

    #[test]
    fn using_an_array_without_an_index_is_an_error() {
        let err = alloc("let v[3] = {1,2,3}; print(v);").unwrap_err();
        assert!(
            err.message.contains("is an array; index it"),
            "{}",
            err.message
        );
    }

    #[test]
    fn assigning_to_the_array_name_is_an_error() {
        let err = alloc("let v[3] = {1,2,3}; v = 1;").unwrap_err();
        assert!(
            err.message.contains("assign to an element"),
            "{}",
            err.message
        );
    }

    #[test]
    fn an_element_can_be_assigned_after_declaration() {
        let a = alloc("let v[2] = {1,2}; v[1] = 9; print(v[1]);").unwrap();
        assert_eq!(a.lookup_element("v", 1), Some('B'));
        assert_eq!(a.allocation().used(), 2);
    }

    #[test]
    fn an_element_reference_uses_the_binding_in_force() {
        // `v` is freed and declared again; `v[0]` resolves through the live
        // binding at each point.
        let source = "let v[2] = {1,2}; free v; let w[2] = {3,4}; print(v[0]);";
        let err = alloc(source).unwrap_err();
        assert!(err.message.contains("not defined here"), "{}", err.message);
    }

    #[test]
    fn freeing_an_array_releases_every_element() {
        let a = alloc("let v[3] = {1,2,3}; free v; let w[3] = {4,5,6};").unwrap();
        assert_eq!(a.lookup_element("w", 0), Some('A'));
        assert_eq!(a.lookup_element("w", 1), Some('B'));
        assert_eq!(a.lookup_element("w", 2), Some('C'));
        assert_eq!(a.allocation().used(), 3);
        assert_eq!(
            a.allocation().freed,
            vec!["v"],
            "reported once, not per element"
        );
    }

    #[test]
    fn an_array_can_be_redeclared_after_a_free() {
        let a = alloc("let v[2] = {1,2}; free v; let v[2] = {3,4}; print(v[1]);").unwrap();
        assert_eq!(a.lookup_element("v", 1), Some('B'));
        assert_eq!(a.len(), 4, "two lives, two bindings each");
    }

    #[test]
    fn double_free_of_an_array_is_an_error() {
        let err = alloc("let v[2] = {1,2}; free v; free v;").unwrap_err();
        assert!(err.message.contains("double free"), "{}", err.message);
    }

    #[test]
    fn use_after_free_of_an_array_is_an_error() {
        let err = alloc("let v[2] = {1,2}; free v; print(v[0]);").unwrap_err();
        assert!(err.message.contains("not defined here"), "{}", err.message);
    }

    #[test]
    fn an_array_needs_a_fresh_name_after_a_scalar() {
        let err = alloc("let x = 1; let x[2] = {1,2};").unwrap_err();
        assert!(err.message.contains("single variable"), "{}", err.message);
    }

    #[test]
    fn a_scalar_needs_a_free_after_an_array() {
        let err = alloc("let v[2] = {1,2}; let v = 1;").unwrap_err();
        assert!(err.message.contains("already declared"), "{}", err.message);
    }

    #[test]
    fn an_element_cannot_be_used_in_its_own_initializer() {
        let err = alloc("let v[2] = {v[0], 2};").unwrap_err();
        assert!(
            err.message.contains("its own initializer"),
            "{}",
            err.message
        );
    }

    #[test]
    fn a_field_of_an_array_element_is_an_error() {
        let err = alloc("let v[2] = {1,2}; print(v[0].size);").unwrap_err();
        assert!(
            err.message.contains("array elements are numbers"),
            "{}",
            err.message
        );
    }

    #[test]
    fn array_element_bindings_report_their_element() {
        let a = alloc("let v[2] = {1,2};").unwrap();
        let allocation = a.allocation();
        assert_eq!(allocation.bindings[0].element, Some(0));
        assert_eq!(allocation.bindings[1].element, Some(1));
        assert_eq!(allocation.registers[0].1[0].label(), "v[0]");
        assert_eq!(allocation.registers[1].1[0].label(), "v[1]");
    }

    #[test]
    fn a_checked_free_of_an_array_with_a_jump_is_rejected() {
        let err = alloc("let v[2] = {1,2}; free v; goto 1; label 1;").unwrap_err();
        assert!(err.message.contains("unsafe_free"), "{}", err.message);
    }

    #[test]
    fn unsafe_free_releases_a_whole_array() {
        let a =
            alloc("let v[2] = {1,2}; unsafe_free v; let w[2] = {3,4}; goto 1; label 1;").unwrap();
        assert_eq!(a.lookup_element("w", 0), Some('A'));
        assert_eq!(a.allocation().freed, vec!["v"]);
    }

    // -- reports ------------------------------------------------------------

    #[test]
    fn register_at_picks_the_binding_in_force() {
        // `x` is A, then released and declared again — still A, but a second
        // binding. Uses before and after resolve through the right one.
        let source = "let x = 1; print(x); free x; let x = 2; print(x);";
        let a = alloc(source).unwrap();
        assert_eq!(a.register_at("x", 10), Some('A'));
        assert_eq!(a.register_at("x", 40), Some('A'));
        assert_eq!(a.register_at("x", 5), Some('A'));
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
