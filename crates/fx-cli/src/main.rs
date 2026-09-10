//! `fx50` — the unified CASIO fx-50FH II command-line tool.
//!
//! With no arguments it behaves like Python's interpreter: an interactive REPL
//! when stdin is a terminal, or "run the program piped into me" otherwise.
//! Everything else is a subcommand (or the equivalent flag).
//!
//! ```text
//! fx50                       interactive REPL (like `python`)
//! fx50 program.fx            run a PRGM file (like `python script.py`)
//! fx50 run program.fxc       transpile C-like source, then run it
//! fx50 eval "2+3×4"          evaluate one expression
//! fx50 build program.fxc     transpile to PRGM on stdout
//! fx50 lsp                   language server over stdio
//! fx50 completions bash      emit a shell completion script
//! ```
//!
//! Argument parsing is [`clap`]; the REPL uses [`rustyline`] for history and
//! line editing; errors are rendered with [`codespan_reporting`] so the byte
//! offsets carried by [`CalcError`] show up as a caret under the source.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

use casio_fx50fh2::token::TokenKind;
use casio_fx50fh2::{CalcError, Environment, Host, Interpreter, MockHost, Mode, compile_with};

// ---------------------------------------------------------------------------
// Command-line surface

#[derive(Debug, Parser)]
#[command(
    name = "fx50",
    version,
    about = "CASIO fx-50FH II interpreter, transpiler and language server",
    long_about = "Interprets fx-50FH II PRGM programs. With no arguments it starts an \
                  interactive REPL when stdin is a terminal, or runs whatever is piped in.",
    after_help = "EXAMPLES:\n  \
                  fx50 eval \"2+3×4\"\n  \
                  fx50 eval --mode CMPLX \"(3+4i)×(1-2i)\"\n  \
                  fx50 run examples/factorial.fx\n  \
                  fx50 build program.fxc > program.fx\n  \
                  fx50 test examples/factorial.fxc\n  \
                  printf '3+4' | fx50"
)]
struct Cli {
    /// Emit ASCII aliases (`->`, `disp`, `=>`) instead of calculator glyphs
    #[arg(short, long, global = true)]
    ascii: bool,

    /// Evaluate an expression (flag form of `eval`)
    #[arg(short = 'e', long = "eval", value_name = "EXPR")]
    eval: Option<String>,

    /// Operating mode: COMP, CMPLX, BASE, SD or REG.
    ///
    /// Overrides any `#mode` header in the program.  Without either, programs
    /// run in COMP, which has no complex numbers, statistics or base-n.
    #[arg(short = 'm', long = "mode", value_name = "MODE", global = true)]
    mode: Option<String>,

    /// Transpile C-like source to PRGM (flag form of `build`)
    #[arg(short = 'b', long = "build", value_name = "FILE")]
    build: Option<PathBuf>,

    /// Start the language server (flag form of `lsp`)
    #[arg(short = 'l', long = "lsp")]
    lsp: bool,

    /// A program file to run, like `python script.py`
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a PRGM (.fx) or C-like (.fxc) program
    Run {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Evaluate an expression and print the result
    Eval {
        #[arg(value_name = "EXPR", required = true, num_args = 1.., allow_hyphen_values = true)]
        expression: Vec<String>,
    },
    /// Transpile C-like source to PRGM on stdout
    Build {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
    /// Start the language server over stdio
    Lsp,
    /// Generate a shell completion script
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Run JSON test cases against a program
    Test {
        /// A `.fxc` program (its sibling `<name>.tests.json` is used) or a
        /// `.tests.json` suite directly
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Only run cases whose name contains this text
        #[arg(long, value_name = "TEXT")]
        filter: Option<String>,
        /// Print the report as JSON instead of a human summary
        #[arg(long)]
        json: bool,
    },
}

// ---------------------------------------------------------------------------
// Errors

enum Fail {
    /// A calculator error, tied to the source it came from so it can be
    /// rendered with a caret at the right offset.
    Calc {
        source: String,
        name: String,
        error: CalcError,
    },
    Message(String),
    /// The command already reported its own failure (for example a test suite
    /// with failing cases); exit non-zero without printing anything further.
    Silent,
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Fail::Message(message)
    }
}

fn calc_fail(source: &str, name: &str, error: CalcError) -> Fail {
    Fail::Calc {
        source: source.to_string(),
        name: name.to_string(),
        error,
    }
}

// ---------------------------------------------------------------------------
// Entry point

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail::Message(message)) => {
            eprintln!("fx50: {message}");
            ExitCode::FAILURE
        }
        Err(Fail::Calc {
            source,
            name,
            error,
        }) => {
            render_calc_error(&source, &name, &error);
            ExitCode::FAILURE
        }
        Err(Fail::Silent) => ExitCode::FAILURE,
    }
}

fn dispatch(cli: Cli) -> Result<(), Fail> {
    let mode = match &cli.mode {
        Some(name) => Some(Mode::parse(name).ok_or_else(|| {
            Fail::Message(format!(
                "unknown mode `{name}`; expected COMP, CMPLX, BASE, SD or REG"
            ))
        })?),
        None => None,
    };

    // Flag forms take priority over subcommands so both spellings work.
    if cli.lsp {
        return start_lsp();
    }
    if let Some(expr) = cli.eval {
        return eval(&expr, mode);
    }
    if let Some(file) = cli.build {
        return build(&file, cli.ascii, mode);
    }

    match cli.command {
        Some(Command::Lsp) => start_lsp(),
        Some(Command::Eval { expression }) => eval(&expression.join(" "), mode),
        Some(Command::Build { file }) => build(&file, cli.ascii, mode),
        Some(Command::Test { file, filter, json }) => test_command(&file, filter.as_deref(), json),
        Some(Command::Run { file }) => run_file(&file, cli.ascii, mode),
        Some(Command::Completions { shell }) => {
            write_completions(shell);
            Ok(())
        }
        None => match cli.file {
            Some(file) => run_file(&file, cli.ascii, mode),
            None if io::stdin().is_terminal() => repl(),
            None => run_stdin(mode),
        },
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations

/// Evaluate one expression through the interpreter's own display path, so
/// complex values, base-n output and `Fix`/`Sci`/`Norm` settings are honoured.
fn eval(source: &str, mode: Option<Mode>) -> Result<(), Fail> {
    let program = compile_with(source, mode).map_err(|e| calc_fail(source, "<eval>", e))?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().map_err(|e| calc_fail(source, "<eval>", e))?;
    for line in &interp.host().output {
        println!("{line}");
    }
    Ok(())
}

fn build(file: &Path, ascii: bool, mode: Option<Mode>) -> Result<(), Fail> {
    let prgm = transpile_file(file, ascii, mode)?;
    print!("{prgm}");
    Ok(())
}

fn run_file(file: &Path, ascii: bool, mode: Option<Mode>) -> Result<(), Fail> {
    let name = file.display().to_string();
    let is_c_like = file
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("fxc"));
    let prgm = if is_c_like {
        transpile_file(file, ascii, mode)?
    } else {
        read_source(file)?
    };
    execute(&prgm, &name, mode)
}

fn run_stdin(mode: Option<Mode>) -> Result<(), Fail> {
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| Fail::Message(e.to_string()))?;
    execute(&source, "<stdin>", mode)
}

/// Compile and run PRGM source, showing a caret diagnostic on failure.
fn execute(source: &str, name: &str, mode: Option<Mode>) -> Result<(), Fail> {
    let program = compile_with(source, mode).map_err(|e| calc_fail(source, name, e))?;
    let mut interp = Interpreter::new(program, StdHost { prompt: true });
    interp.run().map_err(|e| calc_fail(source, name, e))
}

fn read_source(file: &Path) -> Result<String, Fail> {
    std::fs::read_to_string(file)
        .map_err(|e| Fail::Message(format!("cannot read `{}`: {e}", file.display())))
}

// ---------------------------------------------------------------------------
// JSON test suites

/// `fx50 test FILE` — run a JSON suite against a program.
///
/// `FILE` may be a `.fxc` program (whose sibling `<name>.tests.json` is used)
/// or a `.tests.json` suite directly.
#[cfg(feature = "transpiler")]
fn test_command(file: &Path, filter: Option<&str>, as_json: bool) -> Result<(), Fail> {
    use fx_transpiler::testing;

    let is_json = file
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let (suite_path, fallback) = if is_json {
        (file.to_path_buf(), None)
    } else {
        let suite = testing::sibling_suite_path(file);
        if !suite.is_file() {
            return Err(Fail::Message(format!(
                "no test suite at `{}`; create it, or pass a `.tests.json` file",
                suite.display()
            )));
        }
        (suite, Some(file.to_path_buf()))
    };

    let mut suite = testing::load_suite_file(&suite_path, fallback.as_deref())
        .map_err(|e| Fail::Message(e.to_string()))?;

    if let Some(filter) = filter {
        let needle = filter.to_ascii_lowercase();
        suite
            .cases
            .retain(|case| case.name.to_ascii_lowercase().contains(&needle));
        if suite.cases.is_empty() {
            return Err(Fail::Message(format!("no test case matches `{filter}`")));
        }
    }

    let report = testing::run_suite(&suite);
    if as_json {
        println!("{}", report.to_json_pretty());
    } else {
        print_test_report(&report);
    }

    if report.is_success() {
        Ok(())
    } else {
        // The report has already said what failed; just exit non-zero.
        Err(Fail::Silent)
    }
}

#[cfg(not(feature = "transpiler"))]
fn test_command(_file: &Path, _filter: Option<&str>, _as_json: bool) -> Result<(), Fail> {
    Err(Fail::Message(
        "test suites are not compiled in (rebuild with `--features transpiler`)".to_string(),
    ))
}

#[cfg(feature = "transpiler")]
fn print_test_report(report: &fx_transpiler::testing::SuiteReport) {
    println!("{}", report.name);
    for case in &report.cases {
        if case.passed {
            println!("  ok    {}", case.name);
        } else {
            println!("  FAIL  {}", case.name);
            print_test_field("expected", &case.expected);
            print_test_field("actual", &case.actual);
        }
    }
    println!("{} passed, {} failed", report.passed(), report.failed());
}

/// Print a labelled value, indenting any continuation lines under the label.
#[cfg(feature = "transpiler")]
fn print_test_field(label: &str, text: &str) {
    let pad = " ".repeat(8 + label.len() + 2);
    let mut lines = text.lines();
    match lines.next() {
        Some(first) => println!("        {label}: {first}"),
        None => println!("        {label}: <empty>"),
    }
    for rest in lines {
        println!("{pad}{rest}");
    }
}

// ---------------------------------------------------------------------------
// REPL

fn repl() -> Result<(), Fail> {
    let mut editor = rustyline::DefaultEditor::new().map_err(|e| Fail::Message(e.to_string()))?;
    let history = history_path();
    if let Some(path) = &history {
        let _ = editor.load_history(path);
    }

    println!(
        "fx50 {} — fx-50FH II interpreter. Type `exit` to quit.",
        env!("CARGO_PKG_VERSION")
    );

    let mut env = Environment::default();
    loop {
        match editor.readline("fx50> ") {
            Ok(line) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(line.as_str());
                if matches!(line.as_str(), "exit" | "quit") {
                    break;
                }
                run_repl_line(&line, &mut env);
            }
            Err(rustyline::error::ReadlineError::Interrupted) => continue,
            Err(rustyline::error::ReadlineError::Eof) => break,
            Err(e) => return Err(Fail::Message(e.to_string())),
        }
    }

    if let Some(path) = &history {
        let _ = editor.save_history(path);
    }
    Ok(())
}

/// Run one REPL line, persisting memory between lines.
fn run_repl_line(line: &str, env: &mut Environment) {
    // A line without its own `#mode` header inherits the mode established by
    // an earlier line, so `#mode CMPLX` followed by `3+4i` works interactively.
    let inherited = if declares_mode(line) {
        None
    } else {
        Some(env.mode)
    };
    match compile_with(line, inherited) {
        Ok(program) => {
            let mut interp = Interpreter::new(program, StdHost { prompt: true });
            *interp.environment_mut() = env.clone();
            match interp.run() {
                Ok(()) => *env = interp.environment().clone(),
                Err(e) => render_calc_error(line, "<repl>", &e),
            }
        }
        Err(e) => render_calc_error(line, "<repl>", &e),
    }
}

/// Does the line carry its own `#mode` directive?
fn declares_mode(line: &str) -> bool {
    casio_fx50fh2::lexer::lex(line)
        .map(|tokens| {
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::ModeDirective(_)))
        })
        .unwrap_or(false)
}

fn history_path() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("fx50");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("history.txt"))
}

// ---------------------------------------------------------------------------
// Completions

fn write_completions(shell: Shell) {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    clap_complete::generate(shell, &mut command, name, &mut io::stdout());
}

// ---------------------------------------------------------------------------
// Optional integrations (compiled in with the `transpiler` / `lsp` features)

/// Transpile a `.fxc` file, resolving `#include` relative to that file.
#[cfg(feature = "transpiler")]
fn transpile_file(file: &Path, ascii: bool, mode: Option<Mode>) -> Result<String, Fail> {
    let options = fx_transpiler::Options {
        ascii,
        mode: mode.map(to_transpiler_mode),
    };
    fx_transpiler::transpile_file(file, options).map_err(|e| Fail::Message(e.to_string()))
}

#[cfg(not(feature = "transpiler"))]
fn transpile_file(_file: &Path, _ascii: bool, _mode: Option<Mode>) -> Result<String, Fail> {
    Err(Fail::Message(
        "transpiler support is not compiled in (rebuild with `--features transpiler`)".to_string(),
    ))
}

/// The transpiler keeps its own `Mode` so it can build without the core crate.
#[cfg(feature = "transpiler")]
fn to_transpiler_mode(mode: Mode) -> fx_transpiler::Mode {
    match mode {
        Mode::Comp => fx_transpiler::Mode::Comp,
        Mode::Cmplx => fx_transpiler::Mode::Cmplx,
        Mode::Base => fx_transpiler::Mode::Base,
        Mode::Sd => fx_transpiler::Mode::Sd,
        Mode::Reg => fx_transpiler::Mode::Reg,
    }
}

fn start_lsp() -> Result<(), Fail> {
    #[cfg(feature = "lsp")]
    {
        fx_lsp::run_server().map_err(|e| Fail::Message(e.to_string()))
    }
    #[cfg(not(feature = "lsp"))]
    {
        Err(Fail::Message(
            "LSP support is not compiled in (rebuild with `--features lsp`)".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Input/output host

struct StdHost {
    prompt: bool,
}

impl Host for StdHost {
    fn read_number(&mut self) -> Result<f64, CalcError> {
        if self.prompt {
            print!("? ");
            io::stdout().flush().ok();
        }
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|e| CalcError::Arg(e.to_string()))?;
        let text = line.trim();
        text.parse::<f64>()
            .map_err(|_| CalcError::Arg(format!("invalid numeric input `{text}`")))
    }

    fn display(&mut self, text: String) {
        println!("{text}");
    }
}

// ---------------------------------------------------------------------------
// Error rendering

/// Print a calculator error with a caret under the offending source range.
fn render_calc_error(source: &str, name: &str, error: &CalcError) {
    use codespan_reporting::diagnostic::{Diagnostic, Label};
    use codespan_reporting::files::SimpleFile;
    use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
    use codespan_reporting::term::{self, Config};

    let file = SimpleFile::new(name, source);
    let mut diagnostic = Diagnostic::error().with_message(error.label());

    match error_offset(error).filter(|_| !source.is_empty()) {
        Some(offset) => {
            let len = source.len();
            let start = offset.min(len - 1);
            let end = (start + 1).min(len);
            diagnostic = diagnostic.with_labels(vec![
                Label::primary((), start..end).with_message(error.detail()),
            ]);
        }
        None => {
            diagnostic = diagnostic.with_notes(vec![error.detail()]);
        }
    }

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let _ = term::emit_to_write_style(&mut writer.lock(), &Config::default(), &file, &diagnostic);
}

/// The byte offset a `CalcError` points at, if it carries one.
fn error_offset(error: &CalcError) -> Option<usize> {
    error.pos()
}
