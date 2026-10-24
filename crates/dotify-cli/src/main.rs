//! `dotify` – headless project manipulation and export.
//!
//! Every edit goes through `dotify_core::Editor`, the same command engine the
//! desktop app uses.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dotify_core::format::{self, WriteOptions};
use dotify_core::{Command, Document, Editor};
use dotify_export::PngExportOptions;

#[derive(Parser)]
#[command(name = "dotify", version, about = "Headless Dotify project tool")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an empty project.
    New {
        path: PathBuf,
        #[arg(long, default_value_t = 32)]
        width: u32,
        #[arg(long, default_value_t = 32)]
        height: u32,
        #[arg(long)]
        name: Option<String>,
    },
    /// Print the project structure as JSON.
    Info { path: PathBuf },
    /// Apply a JSON command (or array of commands) atomically and save.
    ///
    /// Reads from FILE, or stdin when FILE is `-`.
    Apply {
        path: PathBuf,
        file: PathBuf,
        /// Validate and report without saving.
        #[arg(long)]
        dry_run: bool,
    },
    /// Export one frame as PNG.
    ExportPng {
        path: PathBuf,
        output: PathBuf,
        /// Zero-based frame index.
        #[arg(long, default_value_t = 0)]
        frame: usize,
        #[arg(long, default_value_t = 1)]
        scale: u32,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Cmd::New { path, width, height, name } => {
            let name = name.unwrap_or_else(|| {
                path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into())
            });
            let doc = Document::new(name, width, height)?;
            format::save(&doc, &path, WriteOptions::default())?;
            println!("{}", path.display());
        }
        Cmd::Info { path } => {
            let editor = Editor::new(format::load(&path)?);
            println!("{}", serde_json::to_string_pretty(&editor.summary())?);
        }
        Cmd::Apply { path, file, dry_run } => {
            let mut text = String::new();
            if file.as_os_str() == "-" {
                std::io::stdin().read_to_string(&mut text)?;
            } else {
                text = std::fs::read_to_string(&file)?;
            }
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let command = match value {
                serde_json::Value::Array(_) => Command::Batch { label: None, commands: serde_json::from_value(value)? },
                v => serde_json::from_value(v)?,
            };
            let mut editor = Editor::new(format::load(&path)?);
            let outcome = editor.execute(command).map_err(|e| format!("{} ({})", e, e.code()))?;
            if !dry_run && outcome.changed {
                format::save(editor.document(), &path, WriteOptions::default())?;
            }
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        Cmd::ExportPng { path, output, frame, scale } => {
            let doc = format::load(&path)?;
            let frame = doc.frames().get(frame).ok_or("frame index out of range")?.id;
            dotify_export::export_png(&doc, PngExportOptions { frame, scale }, &output)?;
            println!("{}", output.display());
        }
    }
    Ok(())
}
