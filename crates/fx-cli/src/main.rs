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

use casio_fx50fh2::{CalcError, Environment, Host, Interpreter, MockHost, compile};

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
                  fx50 run examples/factorial.fx\n  \
                  fx50 build program.fxc > program.fx\n  \
                  printf '3+4' | fx50"
)]
struct Cli {
    /// Emit ASCII aliases (`->`, `disp`, `=>`) instead of calculator glyphs
    #[arg(short, long, global = true)]
    ascii: bool,

    /// Evaluate an expression (flag form of `eval`)
    #[arg(short = 'e', long = "eval", value_name = "EXPR")]
    eval: Option<String>,

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
    }
}

fn dispatch(cli: Cli) -> Result<(), Fail> {
    // Flag forms take priority over subcommands so both spellings work.
    if cli.lsp {
        return start_lsp();
    }
    if let Some(expr) = cli.eval {
        return eval(&expr);
    }
    if let Some(file) = cli.build {
        return build(&file, cli.ascii);
    }

    match cli.command {
        Some(Command::Lsp) => start_lsp(),
        Some(Command::Eval { expression }) => eval(&expression.join(" ")),
        Some(Command::Build { file }) => build(&file, cli.ascii),
        Some(Command::Run { file }) => run_file(&file, cli.ascii),
        Some(Command::Completions { shell }) => {
            write_completions(shell);
            Ok(())
        }
        None => match cli.file {
            Some(file) => run_file(&file, cli.ascii),
            None if io::stdin().is_terminal() => repl(),
            None => run_stdin(),
        },
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations

/// Evaluate one expression through the interpreter's own display path, so
/// complex values, base-n output and `Fix`/`Sci`/`Norm` settings are honoured.
fn eval(source: &str) -> Result<(), Fail> {
    let program = compile(source).map_err(|e| calc_fail(source, "<eval>", e))?;
    let mut interp = Interpreter::new(program, MockHost::default());
    interp.run().map_err(|e| calc_fail(source, "<eval>", e))?;
    for line in &interp.host().output {
        println!("{line}");
    }
    Ok(())
}

fn build(file: &Path, ascii: bool) -> Result<(), Fail> {
    let source = read_source(file)?;
    let prgm = transpile_source(&source, ascii)?;
    print!("{prgm}");
    Ok(())
}

fn run_file(file: &Path, ascii: bool) -> Result<(), Fail> {
    let source = read_source(file)?;
    let name = file.display().to_string();
    let is_c_like = file
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("fxc"));
    let prgm = if is_c_like {
        transpile_source(&source, ascii)?
    } else {
        source
    };
    execute(&prgm, &name)
}

fn run_stdin() -> Result<(), Fail> {
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|e| Fail::Message(e.to_string()))?;
    execute(&source, "<stdin>")
}

/// Compile and run PRGM source, showing a caret diagnostic on failure.
fn execute(source: &str, name: &str) -> Result<(), Fail> {
    let program = compile(source).map_err(|e| calc_fail(source, name, e))?;
    let mut interp = Interpreter::new(program, StdHost { prompt: true });
    interp.run().map_err(|e| calc_fail(source, name, e))
}

fn read_source(file: &Path) -> Result<String, Fail> {
    std::fs::read_to_string(file)
        .map_err(|e| Fail::Message(format!("cannot read `{}`: {e}", file.display())))
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
    match compile(line) {
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

fn transpile_source(source: &str, ascii: bool) -> Result<String, Fail> {
    #[cfg(feature = "transpiler")]
    {
        fx_transpiler::transpile_with(source, fx_transpiler::Options { ascii })
            .map_err(|e| Fail::Message(e.to_string()))
    }
    #[cfg(not(feature = "transpiler"))]
    {
        let _ = (source, ascii);
        Err(Fail::Message(
            "transpiler support is not compiled in (rebuild with `--features transpiler`)"
                .to_string(),
        ))
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
                Label::primary((), start..end).with_message(error.to_string()),
            ]);
        }
        None => {
            diagnostic = diagnostic.with_notes(vec![error.to_string()]);
        }
    }

    let writer = StandardStream::stderr(ColorChoice::Auto);
    let _ = term::emit_to_write_style(&mut writer.lock(), &Config::default(), &file, &diagnostic);
}

/// The byte offset a `CalcError` points at, if it carries one.
fn error_offset(error: &CalcError) -> Option<usize> {
    match error {
        CalcError::Syntax { pos, .. } => *pos,
        _ => None,
    }
}
