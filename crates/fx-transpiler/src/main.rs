//! `fxc` — transpile a small C-like language into CASIO fx-50FH II PRGM.
//!
//! ```text
//! fxc build [--ascii] program.fxc   # print PRGM on stdout
//! fxc run   [--ascii] program.fxc   # transpile and execute (feature `execute`)
//! ```
//!
//! `run` reads `?` prompts from stdin and prints `◢` displays to stdout. It is
//! only available when the crate is built with the default `execute` feature;
//! without it the `build` path still has no dependency on the interpreter.

use std::process::ExitCode;

use fx_transpiler::{Options, transpile_with};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("fxc: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut ascii = false;
    let mut positionals: Vec<String> = Vec::new();

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--ascii" | "-a" => ascii = true,
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--version" | "-V" => {
                println!("fxc {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option `{other}` (try `fxc --help`)"));
            }
            other => positionals.push(other.to_string()),
        }
    }

    let (command, file) = match positionals.as_slice() {
        [] => {
            print_help();
            return Ok(());
        }
        [command] => (command.as_str(), None),
        [command, file, ..] => (command.as_str(), Some(file.as_str())),
    };

    match command {
        "build" => build(require_file(file, "build")?, ascii),
        "run" => run_file(require_file(file, "run")?, ascii),
        other => Err(format!(
            "unknown command `{other}` (expected `build` or `run`)"
        )),
    }
}

fn require_file<'a>(file: Option<&'a str>, command: &str) -> Result<&'a str, String> {
    file.ok_or_else(|| format!("`fxc {command}` needs a `.fxc` file"))
}

fn read(file: &str) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("cannot read `{file}`: {e}"))
}

fn build(file: &str, ascii: bool) -> Result<(), String> {
    let source = read(file)?;
    let prgm = transpile_with(&source, Options { ascii }).map_err(|e| e.to_string())?;
    print!("{prgm}");
    Ok(())
}

#[cfg(feature = "execute")]
fn run_file(file: &str, ascii: bool) -> Result<(), String> {
    let source = read(file)?;
    let prgm = transpile_with(&source, Options { ascii }).map_err(|e| e.to_string())?;
    let program = casio_fx50fh2::compile(&prgm).map_err(|e| e.to_string())?;
    let mut interp = casio_fx50fh2::Interpreter::new(program, StdHost);
    interp.run().map_err(|e| e.to_string())
}

#[cfg(not(feature = "execute"))]
fn run_file(file: &str, _ascii: bool) -> Result<(), String> {
    Err(format!(
        "`fxc run` is unavailable: this build was made without the `execute` feature (asked to run `{file}`)"
    ))
}

fn print_help() {
    println!(
        "fxc {} — transpile C-like source to fx-50FH II PRGM\n\
         \n\
         USAGE:\n    fxc build [--ascii] <file.fxc>   Print PRGM to stdout\n    fxc run   [--ascii] <file.fxc>   Transpile and execute\n\
         \n\
         OPTIONS:\n    -a, --ascii    Emit ASCII aliases (->, disp, <=, ...)\n    -h, --help     Show this help\n    -V, --version  Show the version",
        env!("CARGO_PKG_VERSION")
    );
}

/// Reads `?` prompts from stdin and prints `◢` displays to stdout.
#[cfg(feature = "execute")]
struct StdHost;

#[cfg(feature = "execute")]
impl casio_fx50fh2::Host for StdHost {
    fn read_number(&mut self) -> Result<f64, casio_fx50fh2::CalcError> {
        use std::io::Write;

        print!("? ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| casio_fx50fh2::CalcError::Arg(e.to_string()))?;
        let text = line.trim();
        text.parse::<f64>()
            .map_err(|_| casio_fx50fh2::CalcError::Arg(format!("invalid numeric input `{text}`")))
    }

    fn display(&mut self, text: String) {
        println!("{text}");
    }
}
