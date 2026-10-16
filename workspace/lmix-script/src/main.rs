//! lmix-script: cue lists from Markdown scripts (show mode, IMP).
//!
//! ```text
//!   lmix-script check <folder>            problems with file and line
//!   lmix-script load <folder> [--watch]   lists into lmix-core, again on change
//!   lmix-script schema                    JSON schema of a cue block (IMP.8)
//! ```
//!
//! A tool of its own beside lmix-core, which knows no Markdown (S-E.2):
//! it only talks to the API, like any remote control.

mod load;
mod parse;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use clap::{Parser, Subcommand};
use notify::Watcher;

/// Changes of a folder that come together (an editor saving) load once.
const SETTLE: Duration = Duration::from_millis(300);

#[derive(Parser)]
#[command(about = "Cue lists from Markdown scripts for lmix")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read the folder and print its problems.
    Check { folder: PathBuf },
    /// Load the folder's lists into lmix-core.
    Load {
        folder: PathBuf,
        /// lmix-core's address.
        #[arg(long, env = "LMIX_CORE", default_value = "http://127.0.0.1:8200")]
        core: String,
        /// lmix-core's password (`--password` there).
        #[arg(long, env = "LMIX_PASSWORD", hide_env_values = true)]
        password: Option<String>,
        /// Load again whenever a file changes (IMP.5).
        #[arg(long)]
        watch: bool,
    },
    /// Print the JSON schema of a cue block.
    Schema,
}

fn main() -> ExitCode {
    match Args::parse().command {
        Command::Check { folder } => match parse::read_folder(&folder) {
            Ok(script) => report(&script),
            Err(e) => fail(&folder, e),
        },
        Command::Load {
            folder,
            core,
            password,
            watch,
        } => {
            let api = load::Api::new(&core, password);
            let code = load_once(&api, &folder);
            if !watch {
                return code;
            }
            if let Err(e) = watch_folder(&api, &folder) {
                eprintln!("{}: cannot watch: {e}", folder.display());
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Command::Schema => {
            let schema = schemars::schema_for!(lmix_types::Cue);
            println!("{}", serde_json::to_string_pretty(&schema).expect("schema"));
            ExitCode::SUCCESS
        }
    }
}

fn fail(folder: &std::path::Path, e: std::io::Error) -> ExitCode {
    eprintln!("{}: {e}", folder.display());
    ExitCode::FAILURE
}

/// Print problems; failure when one is fatal.
fn report(script: &parse::Script) -> ExitCode {
    for problem in &script.problems {
        eprintln!("{problem}");
    }
    for list in &script.lists {
        let broken = script
            .problems
            .iter()
            .any(|p| p.fatal && p.file == list.file);
        let state = if broken {
            ", not loaded (errors above)"
        } else {
            ""
        };
        println!(
            "{}: list \"{}\", {} cues{state}",
            list.file.display(),
            list.name,
            list.cues.len()
        );
    }
    if script.problems.iter().any(|p| p.fatal) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn load_once(api: &load::Api, folder: &std::path::Path) -> ExitCode {
    let script = match parse::read_folder(folder) {
        Ok(script) => script,
        Err(e) => return fail(folder, e),
    };
    let mut code = report(&script);
    match load::apply(api, folder, &script) {
        Ok(done) => {
            for line in done.loaded {
                println!("{line}");
            }
            for problem in &done.refused {
                eprintln!("{problem}");
            }
            if !done.refused.is_empty() {
                code = ExitCode::FAILURE;
            }
        }
        Err(e) => {
            eprintln!("lmix-core: {e}");
            code = ExitCode::FAILURE;
        }
    }
    code
}

/// Load again on every change of a Markdown file, until killed.
fn watch_folder(api: &load::Api, folder: &std::path::Path) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        let markdown = event
            .paths
            .iter()
            .any(|p| p.extension().is_some_and(|e| e == "md"));
        if markdown && !event.kind.is_access() {
            let _ = tx.send(());
        }
    })?;
    watcher.watch(folder, notify::RecursiveMode::Recursive)?;
    println!("watching {}", folder.display());
    while rx.recv().is_ok() {
        // Wait until the editor is done, then load once.
        while rx.recv_timeout(SETTLE).is_ok() {}
        let _ = load_once(api, folder);
    }
    Ok(())
}
