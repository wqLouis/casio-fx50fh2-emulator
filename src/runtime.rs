//! The tree-walking interpreter.
//!
//! Programs are flattened to a `Vec<Stmt>` and executed with an explicit
//! program counter, which is what makes `Goto`/`Lbl` and the structured
//! `If`/`While`/`For` blocks (which are just marker statements) work.
//!
//! Every expression evaluates to a [`Value`] that may be real or complex.

use std::collections::HashMap;

use crate::ast::{Expr, MemOp, Setup, Stmt, UnaryOp};
use crate::bases::Base;
use crate::error::CalcError;
use crate::format::format_number;
use crate::mode::Mode;
use crate::precision::normalize;
use crate::stats::{StatVar, Stats};
use crate::token::{BinOp, ConstName, FuncName, VarName};
use crate::value::{ComplexFormat, ComplexPart, Value, format_cartesian, format_polar};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleMode {
    Deg,
    Rad,
    Gra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// `Norm 1` / `Norm 2`
    Norm(u8),
    /// `Fix n`
    Fix(u8),
    /// `Sci n`
    Sci(u8),
}

/// The calculator's memory and setup state.
#[derive(Debug, Clone)]
pub struct Environment {
    vars: [Value; 7],
    ans: Value,
    /// The "hidden" result memory that `?` writes to.
    hidden: Value,
    pub angle: AngleMode,
    pub display: DisplayMode,
    /// `None` means ordinary real/complex arithmetic.
    pub base: Option<Base>,
    /// How complex results are rendered.
    pub complex_format: ComplexFormat,
    /// Which part of a complex result the display shows (`Re⇔Im`).
    pub complex_part: ComplexPart,
    /// The operating mode the program declared with `#mode` (default COMP).
    pub mode: Mode,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            vars: [Value::Real(0.0); 7],
            ans: Value::Real(0.0),
            hidden: Value::Real(0.0),
            angle: AngleMode::Deg,
            display: DisplayMode::Norm(1),
            base: None,
            complex_format: ComplexFormat::Cartesian,
            complex_part: ComplexPart::Both,
            mode: Mode::default(),
        }
    }
}

impl Environment {
    /// Real part of a variable.  Kept for API compatibility; prefer
    /// [`Environment::get_value`] when a complex result is possible.
    pub fn get(&self, var: VarName) -> f64 {
        self.get_value(var).re()
    }

    pub fn get_value(&self, var: VarName) -> Value {
        match var {
            VarName::Ans => self.ans,
            other => self.vars[other.slot().expect("Ans handled above")],
        }
    }

    pub fn set(&mut self, var: VarName, value: f64) {
        self.set_value(var, Value::Real(value));
    }

    pub fn set_value(&mut self, var: VarName, value: Value) {
        match var {
            VarName::Ans => self.ans = value,
            other => self.vars[other.slot().expect("Ans handled above")] = value,
        }
    }

    pub fn ans(&self) -> f64 {
        self.ans.re()
    }

    pub fn ans_value(&self) -> Value {
        self.ans
    }

    pub fn hidden(&self) -> f64 {
        self.hidden.re()
    }

    pub fn hidden_value(&self) -> Value {
        self.hidden
    }

    pub fn reset_memory(&mut self) {
        self.vars = [Value::Real(0.0); 7];
        self.ans = Value::Real(0.0);
        self.hidden = Value::Real(0.0);
    }

    /// Format a real value the way the lower display line would show it.
    pub fn format(&self, value: f64) -> String {
        if let Some(base) = self.base {
            return base.format(value);
        }
        format_number(value, self.display)
    }

    /// Format a value that may be complex.
    pub fn format_value(&self, value: Value) -> String {
        match value {
            Value::Real(x) => self.format(x),
            Value::Sexagesimal(x) => crate::value::format_sexagesimal(x),
            Value::Complex(re, im) => match self.complex_format {
                ComplexFormat::Cartesian => match self.complex_part {
                    ComplexPart::Both => format_cartesian(re, im, &|x| self.format(x)),
                    ComplexPart::Real => self.format(re),
                    ComplexPart::Imaginary => {
                        format!("{}{}", self.format(im), crate::value::IMAGINARY_UNIT)
                    }
                },
                ComplexFormat::Polar => {
                    let r = (re * re + im * im).sqrt();
                    let theta = from_rad(im.atan2(re), self.angle);
                    format_polar(r, theta, &|x| self.format(x))
                }
            },
        }
    }
}

/// Everything the interpreter needs from the outside world.
pub trait Host {
    /// Read one value for a `?` prompt.
    fn read_number(&mut self) -> Result<f64, CalcError>;
    /// Emit one display line (`◢`).
    fn display(&mut self, text: String);
}

/// A silent host that feeds a fixed list of inputs and records displays.
/// Handy for tests.
#[derive(Debug, Default)]
pub struct MockHost {
    pub inputs: std::collections::VecDeque<f64>,
    pub output: Vec<String>,
}

impl MockHost {
    pub fn with_inputs(inputs: impl IntoIterator<Item = f64>) -> Self {
        MockHost {
            inputs: inputs.into_iter().collect(),
            output: Vec::new(),
        }
    }
}

impl Host for MockHost {
    fn read_number(&mut self) -> Result<f64, CalcError> {
        self.inputs
            .pop_front()
            .ok_or_else(|| CalcError::Arg("no input available for `?`".to_string()))
    }

    fn display(&mut self, text: String) {
        self.output.push(text);
    }
}

#[derive(Debug, Clone, Copy)]
struct IfInfo {
    else_idx: Option<usize>,
    end_idx: usize,
}

#[derive(Debug, Clone, Copy)]
struct ForState {
    var: VarName,
    to: f64,
    step: f64,
}

#[derive(Debug, Clone, Copy)]
struct LoopFrame {
    start: usize,
    end: usize,
    for_state: Option<ForState>,
}

#[derive(Debug)]
pub struct Interpreter<H: Host> {
    program: Vec<Stmt>,
    labels: HashMap<u8, usize>,
    if_map: HashMap<usize, IfInfo>,
    else_map: HashMap<usize, usize>,
    while_map: HashMap<usize, usize>,
    for_map: HashMap<usize, usize>,
    env: Environment,
    stats: Stats,
    host: H,
    rand_state: u64,
    if_stack: Vec<bool>,
    loop_stack: Vec<LoopFrame>,
    last_displayed: bool,
    /// Whether any statement has produced a value yet.  A program that only
    /// sets up state (for example a lone `#mode CMPLX`) has nothing to show.
    produced_value: bool,
}

impl<H: Host> Interpreter<H> {
    pub fn new(program: Vec<Stmt>, host: H) -> Self {
        Interpreter {
            program,
            labels: HashMap::new(),
            if_map: HashMap::new(),
            else_map: HashMap::new(),
            while_map: HashMap::new(),
            for_map: HashMap::new(),
            env: Environment::default(),
            stats: Stats::default(),
            host,
            rand_state: 0x2545_F491_4F6C_DD1D,
            if_stack: Vec::new(),
            loop_stack: Vec::new(),
            last_displayed: false,
            produced_value: false,
        }
    }

    pub fn environment(&self) -> &Environment {
        &self.env
    }

    pub fn environment_mut(&mut self) -> &mut Environment {
        &mut self.env
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn stats_mut(&mut self) -> &mut Stats {
        &mut self.stats
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    pub fn into_host(self) -> H {
        self.host
    }

    /// Index labels and match up `If`/`While`/`For` markers.
    fn build_indexes(&mut self) -> Result<(), CalcError> {
        self.labels.clear();
        self.if_map.clear();
        self.else_map.clear();
        self.while_map.clear();
        self.for_map.clear();

        let mut if_stack = Vec::new();
        let mut while_stack = Vec::new();
        let mut for_stack = Vec::new();

        for (i, stmt) in self.program.iter().enumerate() {
            match stmt {
                Stmt::Label(n) => {
                    self.labels.entry(*n).or_insert(i);
                }
                Stmt::If { .. } => if_stack.push(i),
                Stmt::Else => {
                    if let Some(&top) = if_stack.last() {
                        self.else_map.insert(i, top);
                        self.if_map
                            .entry(top)
                            .or_insert(IfInfo {
                                else_idx: None,
                                end_idx: usize::MAX,
                            })
                            .else_idx = Some(i);
                    }
                }
                Stmt::IfEnd => {
                    let top = if_stack
                        .pop()
                        .ok_or_else(|| CalcError::Nesting("IfEnd without If".to_string()))?;
                    self.if_map
                        .entry(top)
                        .or_insert(IfInfo {
                            else_idx: None,
                            end_idx: i,
                        })
                        .end_idx = i;
                }
                Stmt::While { .. } => while_stack.push(i),
                Stmt::WhileEnd => {
                    let start = while_stack
                        .pop()
                        .ok_or_else(|| CalcError::Nesting("WhileEnd without While".to_string()))?;
                    self.while_map.insert(start, i);
                    self.while_map.insert(i, start);
                }
                Stmt::For { .. } => for_stack.push(i),
                Stmt::Next => {
                    let start = for_stack
                        .pop()
                        .ok_or_else(|| CalcError::Nesting("Next without For".to_string()))?;
                    self.for_map.insert(start, i);
                    self.for_map.insert(i, start);
                }
                _ => {}
            }
        }

        if !if_stack.is_empty() {
            return Err(CalcError::Nesting("If without IfEnd".to_string()));
        }
        if !while_stack.is_empty() {
            return Err(CalcError::Nesting("While without WhileEnd".to_string()));
        }
        if !for_stack.is_empty() {
            return Err(CalcError::Nesting("For without Next".to_string()));
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), CalcError> {
        self.build_indexes()?;
        self.if_stack.clear();
        self.loop_stack.clear();

        let mut pc = 0usize;
        self.last_displayed = false;
        self.produced_value = false;
        while pc < self.program.len() {
            let stmt = self.program[pc].clone();
            let mut next = pc + 1;
            match stmt {
                Stmt::Noop | Stmt::Then | Stmt::Label(_) => {}
                Stmt::Mode(mode) => {
                    // The parser guarantees this is the first statement; the
                    // interpreter applies it before anything else runs.
                    self.env.mode = mode;
                }
                Stmt::Expr { expr, display } => {
                    let value = self.eval(&expr)?;
                    self.produced_value = true;
                    self.env.ans = value;
                    self.env.hidden = value;
                    if display {
                        self.show_value(value);
                    } else {
                        self.last_displayed = false;
                    }
                }
                Stmt::Assign {
                    target,
                    value,
                    display,
                } => {
                    let v = self.eval(&value)?;
                    self.produced_value = true;
                    self.env.set_value(target, v);
                    self.env.ans = v;
                    self.env.hidden = v;
                    if display {
                        self.show_value(v);
                    } else {
                        self.last_displayed = false;
                    }
                }
                Stmt::Memory { expr, op, display } => {
                    let v = self.eval(&expr)?;
                    self.produced_value = true;
                    let m = self.env.get_value(VarName::M);
                    let updated = match op {
                        MemOp::Plus => m.add(v),
                        MemOp::Minus => m.sub(v),
                    };
                    self.env.set_value(VarName::M, updated);
                    self.env.ans = v;
                    self.env.hidden = v;
                    if display {
                        self.show_value(v);
                    } else {
                        self.last_displayed = false;
                    }
                }
                Stmt::DataEntry { x, y, freq } => {
                    let xv = require_real(self.eval(&x)?, "DT")?;
                    self.produced_value = true;
                    let yv = match &y {
                        Some(e) => require_real(self.eval(e)?, "DT")?,
                        None => 0.0,
                    };
                    let fv = match &freq {
                        Some(e) => require_real(self.eval(e)?, "DT")?,
                        None => 1.0,
                    };
                    self.stats.add(xv, yv, fv)?;
                    self.last_displayed = false;
                }
                Stmt::CondJump { condition, target } => {
                    let cond = self.eval(&condition)?;
                    if self.truthy(cond)?
                        && let Some(target_pc) = self.exec_simple(&target)?
                    {
                        self.if_stack.clear();
                        self.loop_stack.clear();
                        next = target_pc;
                    }
                }
                Stmt::Goto(label) => {
                    let target = *self.labels.get(&label).ok_or(CalcError::Go(label))?;
                    self.if_stack.clear();
                    self.loop_stack.clear();
                    next = target;
                }
                Stmt::If { condition } => {
                    let info = self
                        .if_map
                        .get(&pc)
                        .copied()
                        .ok_or_else(|| CalcError::Nesting("If without IfEnd".to_string()))?;
                    let cond = self.eval(&condition)?;
                    let taken = self.truthy(cond)?;
                    self.if_stack.push(taken);
                    if !taken {
                        next = match info.else_idx {
                            Some(else_idx) => else_idx + 1,
                            None => info.end_idx,
                        };
                    }
                }
                Stmt::Else => {
                    if self.if_stack.last().copied().unwrap_or(false) {
                        let if_idx = *self
                            .else_map
                            .get(&pc)
                            .ok_or_else(|| CalcError::Nesting("Else without If".to_string()))?;
                        let info = self.if_map[&if_idx];
                        next = info.end_idx;
                    }
                }
                Stmt::IfEnd => {
                    self.if_stack
                        .pop()
                        .ok_or_else(|| CalcError::Nesting("IfEnd without If".to_string()))?;
                }
                Stmt::While { condition } => {
                    let end = *self
                        .while_map
                        .get(&pc)
                        .ok_or_else(|| CalcError::Nesting("While without WhileEnd".to_string()))?;
                    let cond = self.eval(&condition)?;
                    if self.truthy(cond)? {
                        self.loop_stack.push(LoopFrame {
                            start: pc,
                            end,
                            for_state: None,
                        });
                    } else {
                        next = end + 1;
                    }
                }
                Stmt::WhileEnd => {
                    let frame = self
                        .loop_stack
                        .pop()
                        .filter(|f| f.for_state.is_none())
                        .ok_or_else(|| CalcError::Nesting("WhileEnd without While".to_string()))?;
                    next = frame.start;
                }
                Stmt::For {
                    var,
                    from,
                    to,
                    step,
                } => {
                    let end = *self
                        .for_map
                        .get(&pc)
                        .ok_or_else(|| CalcError::Nesting("For without Next".to_string()))?;
                    let from_v = require_real(self.eval(&from)?, "For")?;
                    let to_v = require_real(self.eval(&to)?, "For")?;
                    let step_v = match &step {
                        Some(s) => require_real(self.eval(s)?, "For")?,
                        None => 1.0,
                    };
                    if step_v == 0.0 {
                        return Err(CalcError::Math("For Step cannot be zero".to_string()));
                    }
                    self.env.set(var, from_v);
                    let runs = if step_v > 0.0 {
                        from_v <= to_v
                    } else {
                        from_v >= to_v
                    };
                    if runs {
                        self.loop_stack.push(LoopFrame {
                            start: pc,
                            end,
                            for_state: Some(ForState {
                                var,
                                to: to_v,
                                step: step_v,
                            }),
                        });
                    } else {
                        next = end + 1;
                    }
                }
                Stmt::Next => {
                    let frame = self
                        .loop_stack
                        .last()
                        .copied()
                        .filter(|f| f.for_state.is_some())
                        .ok_or_else(|| CalcError::Nesting("Next without For".to_string()))?;
                    let state = frame.for_state.unwrap();
                    let current = self.env.get(state.var) + state.step;
                    self.env.set(state.var, current);
                    let keep_going = if state.step > 0.0 {
                        current <= state.to
                    } else {
                        current >= state.to
                    };
                    if keep_going {
                        next = frame.start + 1;
                    } else {
                        self.loop_stack.pop();
                    }
                }
                Stmt::Break => {
                    let frame = self
                        .loop_stack
                        .pop()
                        .ok_or_else(|| CalcError::Arg("Break outside a loop".to_string()))?;
                    next = frame.end + 1;
                }
                Stmt::Setup(setup) => self.apply_setup(setup),
                Stmt::ClrMemory => self.env.reset_memory(),
                Stmt::ClrStat => self.stats.clr(),
                Stmt::FreqOn => self.stats.freq_on = true,
                Stmt::FreqOff => self.stats.freq_on = false,
            }
            pc = next;
        }
        // The machine shows the last computed value when the program ends
        // without an explicit `◢`.
        if self.produced_value && !self.last_displayed {
            let value = self.env.hidden;
            self.show_value(value);
        }
        Ok(())
    }

    fn apply_setup(&mut self, setup: Setup) {
        match setup {
            Setup::Deg => self.env.angle = AngleMode::Deg,
            Setup::Rad => self.env.angle = AngleMode::Rad,
            Setup::Gra => self.env.angle = AngleMode::Gra,
            Setup::Fix(n) => self.env.display = DisplayMode::Fix(n),
            Setup::Sci(n) => self.env.display = DisplayMode::Sci(n),
            Setup::Norm(n) => self.env.display = DisplayMode::Norm(n),
            Setup::Dec => self.env.base = Some(Base::Dec),
            Setup::Hex => self.env.base = Some(Base::Hex),
            Setup::Bin => self.env.base = Some(Base::Bin),
            Setup::Oct => self.env.base = Some(Base::Oct),
            Setup::ComplexCartesian => self.env.complex_format = ComplexFormat::Cartesian,
            Setup::ComplexPolar => self.env.complex_format = ComplexFormat::Polar,
            // `Re⇔Im`: the first press shows the imaginary part (the `𝑖`
            // suffix the manual mentions), the next the real part, and so on.
            Setup::ReIm => {
                self.env.complex_part = if self.env.complex_part == ComplexPart::Imaginary {
                    ComplexPart::Real
                } else {
                    ComplexPart::Imaginary
                };
            }
            // Choosing a regression model is a REG-mode setting, like the
            // number base in BASE mode.
            Setup::Reg(reg) => self.stats.reg_type = reg,
            // `°′″`: convert the displayed value between decimal and
            // sexagesimal.
            Setup::Sexagesimal => {
                self.env.ans = self.env.ans.toggle_sexagesimal();
                self.env.hidden = self.env.hidden.toggle_sexagesimal();
            }
        }
    }

    /// Execute a `⇒` target statement immediately. Returns a jump target if
    /// the statement was a `Goto`.
    fn exec_simple(&mut self, stmt: &Stmt) -> Result<Option<usize>, CalcError> {
        match stmt {
            Stmt::Noop => Ok(None),
            Stmt::Expr { expr, display } => {
                let v = self.eval(expr)?;
                self.env.ans = v;
                self.env.hidden = v;
                if *display {
                    self.show_value(v);
                }
                Ok(None)
            }
            Stmt::Assign {
                target,
                value,
                display,
            } => {
                let v = self.eval(value)?;
                self.env.set_value(*target, v);
                self.env.hidden = v;
                if *display {
                    self.show_value(v);
                }
                Ok(None)
            }
            Stmt::Memory { expr, op, display } => {
                let v = self.eval(expr)?;
                let m = self.env.get_value(VarName::M);
                let updated = match op {
                    MemOp::Plus => m.add(v),
                    MemOp::Minus => m.sub(v),
                };
                self.env.set_value(VarName::M, updated);
                self.env.hidden = v;
                if *display {
                    self.show_value(v);
                }
                Ok(None)
            }
            Stmt::DataEntry { x, y, freq } => {
                let xv = require_real(self.eval(x)?, "DT")?;
                let yv = match y {
                    Some(e) => require_real(self.eval(e)?, "DT")?,
                    None => 0.0,
                };
                let fv = match freq {
                    Some(e) => require_real(self.eval(e)?, "DT")?,
                    None => 1.0,
                };
                self.stats.add(xv, yv, fv)?;
                Ok(None)
            }
            Stmt::Goto(label) => {
                let target = *self.labels.get(label).ok_or(CalcError::Go(*label))?;
                Ok(Some(target))
            }
            Stmt::Break => {
                let frame = self
                    .loop_stack
                    .pop()
                    .ok_or_else(|| CalcError::Arg("Break outside a loop".to_string()))?;
                Ok(Some(frame.end + 1))
            }
            Stmt::Setup(setup) => {
                self.apply_setup(*setup);
                Ok(None)
            }
            Stmt::ClrMemory => {
                self.env.reset_memory();
                Ok(None)
            }
            Stmt::ClrStat => {
                self.stats.clr();
                Ok(None)
            }
            Stmt::FreqOn => {
                self.stats.freq_on = true;
                Ok(None)
            }
            Stmt::FreqOff => {
                self.stats.freq_on = false;
                Ok(None)
            }
            Stmt::Label(_) => Ok(None),
            _ => Err(CalcError::Arg(
                "unsupported statement after `⇒`".to_string(),
            )),
        }
    }

    fn show_value(&mut self, value: Value) {
        let text = self.env.format_value(value);
        self.host.display(text);
        self.last_displayed = true;
    }

    fn truthy(&self, value: Value) -> Result<bool, CalcError> {
        match value {
            Value::Real(x) | Value::Sexagesimal(x) => Ok(x != 0.0),
            Value::Complex(..) => Err(CalcError::Math(
                "a complex condition cannot be tested".to_string(),
            )),
        }
    }

    fn next_rand(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.rand_state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rand_state = x;
        let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        (v >> 11) as f64 / (1u64 << 53) as f64
    }

    // -- expression evaluation ---------------------------------------------

    /// Evaluate an expression to a real or complex value.
    pub fn eval(&mut self, expr: &Expr) -> Result<Value, CalcError> {
        let value = match expr {
            Expr::Number(v) => Value::Real(*v),
            Expr::Sexagesimal(value, _) => Value::Sexagesimal(*value),
            Expr::BaseLiteral { value, .. } => Value::Real(*value),
            Expr::Var(v) => self.env.get_value(*v),
            Expr::Const(c) => match c {
                ConstName::Pi => Value::Real(std::f64::consts::PI),
                ConstName::E => Value::Real(std::f64::consts::E),
                ConstName::I => {
                    self.require_complex_mode("the imaginary unit `i`")?;
                    Value::Complex(0.0, 1.0)
                }
                ConstName::Physical(code) => {
                    Value::Real(crate::constants::by_code(*code).map_or(f64::NAN, |c| c.value))
                }
            },
            Expr::Ran => Value::Real(self.next_rand()),
            Expr::StatVar(var) => self.stat_value(*var)?,
            Expr::Input => {
                let v = self.host.read_number()?;
                self.env.hidden = Value::Real(v);
                Value::Real(v)
            }
            Expr::Assign { target, value } => {
                let v = self.eval(value)?;
                self.env.set_value(*target, v);
                v
            }
            Expr::Unary { op, expr } => {
                let v = self.eval(expr)?;
                match op {
                    UnaryOp::Neg => self.wrap_result(v.neg()),
                    UnaryOp::Inverse => {
                        let r = v.inverse()?;
                        self.wrap_result(r)
                    }
                    UnaryOp::Square => self.wrap_result(v.square()),
                    UnaryOp::Cube => self.wrap_result(v.cube()),
                    UnaryOp::Fact => {
                        let x = require_real(v, "!")?;
                        Value::Real(factorial(x)?)
                    }
                    UnaryOp::Percent => {
                        let x = require_real(v, "%")?;
                        Value::Real(normalize(x / 100.0))
                    }
                }
            }
            Expr::ImplicitMul(a, b)
            | Expr::Binary {
                left: a,
                op: BinOp::Mul,
                right: b,
            } => {
                let l = self.eval(a)?;
                let r = self.eval(b)?;
                self.wrap_result(l.mul(r))
            }
            Expr::Binary { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                self.binary(*op, l, r)?
            }
            Expr::Pow { base, exp } => {
                let b = self.eval(base)?;
                let e = self.eval(exp)?;
                self.pow_value(b, e)?
            }
            Expr::Root { index, radicand } => {
                let n = require_real(self.eval(index)?, "x√")?;
                let x = require_real(self.eval(radicand)?, "x√")?;
                Value::Real(normalize(root(x, n)?))
            }
            Expr::Call { func, args } => {
                let values = args
                    .iter()
                    .map(|a| self.eval(a))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(*func, &values)?
            }
        };
        // Outside CMPLX mode the machine can never produce a complex value;
        // `√(-4)` is a `Math ERROR` rather than `2i`.
        if value.is_complex() {
            self.require_complex_mode("a complex result")?;
        }
        Ok(value)
    }

    /// Reject complex-valued constructs outside CMPLX mode.
    fn require_complex_mode(&self, what: &str) -> Result<(), CalcError> {
        if self.env.mode.allows_complex() {
            Ok(())
        } else {
            Err(CalcError::mode(
                self.env.mode,
                format!("{what} needs CMPLX mode"),
                None,
            ))
        }
    }

    /// Apply base-n word wrapping to a real result when a base is selected.
    fn wrap_result(&self, value: Value) -> Value {
        match (self.env.base, value) {
            (Some(base), Value::Real(x)) => Value::Real(base.wrap(x)),
            (_, other) => other,
        }
    }

    fn binary(&mut self, op: BinOp, l: Value, r: Value) -> Result<Value, CalcError> {
        // Bitwise operators only exist in base-n mode.
        if matches!(op, BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor) {
            let base = self.env.base.ok_or_else(|| {
                CalcError::Math("bitwise operators need a number base".to_string())
            })?;
            let a = require_real(l, "bitwise operator")?;
            let b = require_real(r, "bitwise operator")?;
            let wa = base.to_word(a);
            let wb = base.to_word(b);
            let word = match op {
                BinOp::And => wa & wb,
                BinOp::Or => wa | wb,
                BinOp::Xor => wa ^ wb,
                BinOp::Xnor => !(wa ^ wb),
                _ => unreachable!(),
            };
            return Ok(Value::Real(base.from_word(word)));
        }

        let value = match op {
            BinOp::Add => self.wrap_result(l.add(r)),
            BinOp::Sub => self.wrap_result(l.sub(r)),
            BinOp::Mul => self.wrap_result(l.mul(r)),
            BinOp::Div | BinOp::Frac => {
                if let Some(base) = self.env.base {
                    let a = require_real(l, "÷")?;
                    let b = require_real(r, "÷")?;
                    let bv = base.wrap(b) as i64;
                    if bv == 0 {
                        return Err(CalcError::Math("division by zero".to_string()));
                    }
                    let av = base.wrap(a) as i64;
                    Value::Real(base.wrap((av / bv) as f64))
                } else {
                    l.div(r)?
                }
            }
            BinOp::Perm => {
                let n = require_real(l, "nPr")?;
                let k = require_real(r, "nPr")?;
                Value::Real(permutation(n, k)?)
            }
            BinOp::Comb => {
                let n = require_real(l, "nCr")?;
                let k = require_real(r, "nCr")?;
                Value::Real(combination(n, k)?)
            }
            BinOp::Eq => Value::Real(bool_num(l.equals(r))),
            BinOp::Ne => Value::Real(bool_num(!l.equals(r))),
            BinOp::Gt | BinOp::Lt | BinOp::Ge | BinOp::Le => {
                let a = require_real(l, "comparison")?;
                let b = require_real(r, "comparison")?;
                Value::Real(match op {
                    BinOp::Gt => bool_num(a > b),
                    BinOp::Lt => bool_num(a < b),
                    BinOp::Ge => bool_num(a >= b),
                    BinOp::Le => bool_num(a <= b),
                    _ => unreachable!(),
                })
            }
            BinOp::Polar => {
                let radius = require_real(l, "∠")?;
                let theta = require_real(r, "∠")?;
                Value::from_polar(radius, to_rad(theta, self.env.angle))
            }
            BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Xnor => unreachable!(),
        };
        Ok(value)
    }

    fn pow_value(&mut self, base: Value, exp: Value) -> Result<Value, CalcError> {
        if base.is_real() && exp.is_real() {
            let b = base.re();
            let e = exp.re();
            if self.env.base.is_some() {
                if e.fract() != 0.0 || e < 0.0 {
                    return Err(CalcError::Math(
                        "base-n powers need a non-negative integer exponent".to_string(),
                    ));
                }
                let mut acc = Value::Real(1.0);
                let mut count = e.trunc() as u64;
                while count > 0 {
                    acc = self.wrap_result(acc.mul(base));
                    count -= 1;
                }
                return Ok(acc);
            }
            return Ok(Value::Real(normalize(pow(b, e)?)));
        }
        // General complex power: `base^exp = e^(exp · ln base)`.
        if base.is_zero() {
            if exp.is_zero() {
                return Ok(Value::Real(1.0));
            }
            if exp.re() > 0.0 && exp.im() == 0.0 {
                return Ok(Value::Real(0.0));
            }
            return Err(CalcError::Math(
                "0 raised to a non-positive power".to_string(),
            ));
        }
        let log = base.ln()?;
        Ok(exp.mul(log).exp())
    }

    fn call(&mut self, func: FuncName, args: &[Value]) -> Result<Value, CalcError> {
        // Functions that accept (or require) complex arguments.
        match func {
            FuncName::Abs => return Ok(Value::Real(normalize(args[0].abs()))),
            FuncName::Arg => {
                let radians = args[0].arg();
                return Ok(Value::Real(normalize(from_rad(radians, self.env.angle))));
            }
            FuncName::Conjg => return Ok(args[0].conjg()),
            FuncName::Sqrt => return Ok(self.wrap_result(args[0].sqrt())),
            FuncName::Not | FuncName::Neg => {
                let base = self
                    .env
                    .base
                    .ok_or_else(|| CalcError::Math("`Not`/`Neg` need a number base".to_string()))?;
                let x = require_real(args[0], "Not/Neg")?;
                let word = base.to_word(x);
                let result = match func {
                    FuncName::Not => !word,
                    FuncName::Neg => (!word).wrapping_add(1),
                    _ => unreachable!(),
                };
                return Ok(Value::Real(base.from_word(result)));
            }
            _ => {}
        }

        let x = require_real(args[0], func_name(func))?;
        let value = match func {
            FuncName::Sin => to_rad(x, self.env.angle).sin(),
            FuncName::Cos => to_rad(x, self.env.angle).cos(),
            FuncName::Tan => to_rad(x, self.env.angle).tan(),
            FuncName::Asin => {
                if !(-1.0..=1.0).contains(&x) {
                    return Err(CalcError::Math("sin⁻¹ domain".to_string()));
                }
                from_rad(x.asin(), self.env.angle)
            }
            FuncName::Acos => {
                if !(-1.0..=1.0).contains(&x) {
                    return Err(CalcError::Math("cos⁻¹ domain".to_string()));
                }
                from_rad(x.acos(), self.env.angle)
            }
            FuncName::Atan => from_rad(x.atan(), self.env.angle),
            FuncName::Sinh => x.sinh(),
            FuncName::Cosh => x.cosh(),
            FuncName::Tanh => x.tanh(),
            FuncName::Asinh => x.asinh(),
            FuncName::Acosh => {
                if x < 1.0 {
                    return Err(CalcError::Math("cosh⁻¹ domain".to_string()));
                }
                x.acosh()
            }
            FuncName::Atanh => {
                if x.abs() >= 1.0 {
                    return Err(CalcError::Math("tanh⁻¹ domain".to_string()));
                }
                x.atanh()
            }
            FuncName::Log => {
                if args.len() == 2 {
                    // Casio's `log(base, x)`.
                    let (base, val) = (args[0].re(), args[1].re());
                    if base <= 0.0 || base == 1.0 || val <= 0.0 {
                        return Err(CalcError::Math("log domain".to_string()));
                    }
                    val.ln() / base.ln()
                } else {
                    if x <= 0.0 {
                        return Err(CalcError::Math("log domain".to_string()));
                    }
                    x.log10()
                }
            }
            FuncName::Ln => {
                if x <= 0.0 {
                    return Err(CalcError::Math("ln domain".to_string()));
                }
                x.ln()
            }
            FuncName::Sqrt => unreachable!("handled above"),
            FuncName::Cbrt => x.cbrt(),
            FuncName::TenPow => 10f64.powf(x),
            FuncName::EPow => x.exp(),
            FuncName::Abs => unreachable!("handled above"),
            FuncName::Rnd => round_sig(x, 10),
            FuncName::Pol => {
                let y = require_real(args[1], "Pol")?;
                let radius = (x * x + y * y).sqrt();
                self.env.set(VarName::X, radius);
                self.env
                    .set(VarName::Y, from_rad(y.atan2(x), self.env.angle));
                radius
            }
            FuncName::Rec => {
                let theta = require_real(args[1], "Rec")?;
                let xr = x * to_rad(theta, self.env.angle).cos();
                let yr = x * to_rad(theta, self.env.angle).sin();
                self.env.set(VarName::X, xr);
                self.env.set(VarName::Y, yr);
                xr
            }
            FuncName::Arg | FuncName::Conjg | FuncName::Not | FuncName::Neg => {
                unreachable!("handled above")
            }
        };
        Ok(self.wrap_result(Value::Real(normalize(value))))
    }

    fn stat_value(&self, var: StatVar) -> Result<Value, CalcError> {
        // Statistical variables only exist in SD and REG mode.
        if !self.env.mode.allows_stats() {
            return Err(CalcError::mode(
                self.env.mode,
                "statistical variables need SD or REG mode",
                None,
            ));
        }
        let n = self.stats.n();
        let needs_data = matches!(
            var,
            StatVar::MeanX | StatVar::MeanY | StatVar::SigmaX | StatVar::SigmaY
        );
        let needs_sample = matches!(
            var,
            StatVar::Sx
                | StatVar::Sy
                | StatVar::RegA
                | StatVar::RegB
                | StatVar::RegC
                | StatVar::RegR
        );
        if needs_data && n == 0.0 {
            return Err(CalcError::Math("no statistical data".to_string()));
        }
        if needs_sample && n < 2.0 {
            return Err(CalcError::Math("not enough statistical data".to_string()));
        }
        let value = self.stats.value(var);
        if !value.is_finite() {
            return Err(CalcError::Math("no regression for this data".to_string()));
        }
        Ok(Value::Real(normalize(value)))
    }
}

fn require_real(value: Value, what: &str) -> Result<f64, CalcError> {
    match value {
        Value::Real(x) | Value::Sexagesimal(x) => Ok(x),
        Value::Complex(..) => Err(CalcError::Math(format!("`{what}` needs a real argument"))),
    }
}

fn bool_num(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

/// Round to 15 significant digits and apply the machine's autocorrection.
///
/// Kept under its historical name for API compatibility.
pub fn r15(x: f64) -> f64 {
    normalize(x)
}

fn round_sig(x: f64, digits: i32) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let text = format!("{:.*e}", (digits - 1).max(0) as usize, x);
    text.parse().unwrap_or(x)
}

fn to_rad(x: f64, mode: AngleMode) -> f64 {
    match mode {
        AngleMode::Deg => x.to_radians(),
        AngleMode::Rad => x,
        AngleMode::Gra => x * std::f64::consts::PI / 200.0,
    }
}

fn from_rad(x: f64, mode: AngleMode) -> f64 {
    match mode {
        AngleMode::Deg => x.to_degrees(),
        AngleMode::Rad => x,
        AngleMode::Gra => x * 200.0 / std::f64::consts::PI,
    }
}

fn pow(base: f64, exp: f64) -> Result<f64, CalcError> {
    if base < 0.0 && exp.fract() != 0.0 {
        return Err(CalcError::Math("fractional power of negative".to_string()));
    }
    Ok(base.powf(exp))
}

fn root(x: f64, n: f64) -> Result<f64, CalcError> {
    if n == 0.0 {
        return Err(CalcError::Math("0th root".to_string()));
    }
    if x < 0.0 {
        if n.fract() == 0.0 && (n as i64) % 2 != 0 {
            return Ok(-((-x).powf(1.0 / n)));
        }
        return Err(CalcError::Math("even root of negative".to_string()));
    }
    Ok(x.powf(1.0 / n))
}

fn factorial(x: f64) -> Result<f64, CalcError> {
    if x < 0.0 || x.fract() != 0.0 {
        return Err(CalcError::Math(
            "factorial needs a non-negative integer".to_string(),
        ));
    }
    let mut result = 1.0f64;
    let n = x as u64;
    for i in 2..=n {
        result *= i as f64;
    }
    Ok(normalize(result))
}

fn permutation(n: f64, r: f64) -> Result<f64, CalcError> {
    if n < 0.0 || r < 0.0 || n.fract() != 0.0 || r.fract() != 0.0 || r > n {
        return Err(CalcError::Math("nPr needs integers 0 ≤ r ≤ n".to_string()));
    }
    let mut result = 1.0f64;
    let mut i = n;
    let count = r as u64;
    for _ in 0..count {
        result *= i;
        i -= 1.0;
    }
    Ok(normalize(result))
}

fn combination(n: f64, r: f64) -> Result<f64, CalcError> {
    if n < 0.0 || r < 0.0 || n.fract() != 0.0 || r.fract() != 0.0 || r > n {
        return Err(CalcError::Math("nCr needs integers 0 ≤ r ≤ n".to_string()));
    }
    let r = r.min(n - r);
    let mut result = 1.0f64;
    for i in 0..(r as u64) {
        result = result * (n - i as f64) / (i as f64 + 1.0);
    }
    Ok(normalize(result))
}

fn func_name(func: FuncName) -> &'static str {
    match func {
        FuncName::Sin => "sin",
        FuncName::Cos => "cos",
        FuncName::Tan => "tan",
        FuncName::Asin => "sin⁻¹",
        FuncName::Acos => "cos⁻¹",
        FuncName::Atan => "tan⁻¹",
        FuncName::Sinh => "sinh",
        FuncName::Cosh => "cosh",
        FuncName::Tanh => "tanh",
        FuncName::Asinh => "sinh⁻¹",
        FuncName::Acosh => "cosh⁻¹",
        FuncName::Atanh => "tanh⁻¹",
        FuncName::Log => "log",
        FuncName::Ln => "ln",
        FuncName::Sqrt => "√",
        FuncName::Cbrt => "∛",
        FuncName::TenPow => "10^",
        FuncName::EPow => "e^",
        FuncName::Abs => "Abs",
        FuncName::Pol => "Pol",
        FuncName::Rec => "Rec",
        FuncName::Rnd => "Rnd",
        FuncName::Arg => "arg",
        FuncName::Conjg => "Conjg",
        FuncName::Not => "Not",
        FuncName::Neg => "Neg",
    }
}
