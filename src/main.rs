use clap::{Parser, Subcommand};
use std::ffi::OsStr;
use std::io;
use std::process::{Command as ProcessCommand, ExitCode, Output};

mod git;
mod guard;
mod suppressions;

#[derive(Parser)]
#[command(version, about)]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check added staged lines for personal home-directory paths.
    Guard {
        /// Show exact finding fingerprints for deliberate .muttleyignore entries.
        #[arg(long)]
        show_fingerprints: bool,
    },
}

fn main() -> ExitCode {
    let arguments = Arguments::parse();
    match arguments.command {
        Command::Guard { show_fingerprints } => run_guard(show_fingerprints),
    }
}

fn run_guard(show_fingerprints: bool) -> ExitCode {
    let findings = match git::staged_changes(&run_git).and_then(guard::find_home_paths) {
        Ok(findings) => findings,
        Err(error) => {
            eprintln!("muttley: {error}");
            return ExitCode::from(2);
        }
    };

    for finding in &findings {
        println!(
            "{}:{}: home-path: potential personal home-directory path",
            finding.path.to_string_lossy().escape_debug(),
            finding.line_number
        );
        if show_fingerprints {
            println!("  fingerprint: {}", finding.fingerprint);
        }
    }
    if findings.is_empty() {
        return ExitCode::SUCCESS;
    }
    ExitCode::from(1)
}

fn run_git(arguments: &[&OsStr]) -> io::Result<Output> {
    ProcessCommand::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_LITERAL_PATHSPECS")
        .env_remove("GIT_GLOB_PATHSPECS")
        .env_remove("GIT_NOGLOB_PATHSPECS")
        .env_remove("GIT_ICASE_PATHSPECS")
        .args(arguments)
        .output()
}
