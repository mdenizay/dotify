//! `dotify` – headless project manipulation and export.
//!
//! Every edit goes through `dotify_core::Editor`, the same command engine the
//! desktop app uses.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dotify_core::format::{self, WriteOptions};
use dotify_core::{ArtboardId, Command, Project, ProjectCommand, ProjectEditor};
use dotify_export::gif::GifOptions;
use dotify_export::godot::GodotOptions;
use dotify_export::meta::Pivot;
use dotify_export::project::{ArtboardExporter, GodotArtboardExporter, PngArtboardExporter};
use dotify_export::sheet::{SheetLayout, SheetOptions};
use dotify_export::PngExportOptions;

#[derive(Parser)]
#[command(name = "dotify", version, about = "Headless Dotify project tool")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a project with one empty artboard.
    New {
        path: PathBuf,
        #[arg(long, default_value_t = 32)]
        width: u32,
        #[arg(long, default_value_t = 32)]
        height: u32,
        #[arg(long)]
        name: Option<String>,
    },
    /// Print the project structure (library and artboards) as JSON.
    Info { path: PathBuf },
    /// Apply a JSON command (or array of commands) atomically and save.
    ///
    /// Reads from FILE, or stdin when FILE is `-`. Accepts project commands
    /// (`create_artboard`, `edit`, …) and document commands (`draw_line`, …);
    /// document commands apply to `--artboard` (default: the first one).
    Apply {
        path: PathBuf,
        file: PathBuf,
        /// Artboard ID that document commands apply to.
        #[arg(long)]
        artboard: Option<u32>,
        /// Validate and report without saving.
        #[arg(long)]
        dry_run: bool,
    },
    /// Export one frame of one artboard as PNG.
    ExportPng {
        path: PathBuf,
        output: PathBuf,
        /// Artboard ID (default: the first one).
        #[arg(long)]
        artboard: Option<u32>,
        /// Zero-based frame index.
        #[arg(long, default_value_t = 0)]
        frame: usize,
        #[arg(long, default_value_t = 1)]
        scale: u32,
    },
    /// Export every artboard into `<DIR>/<folder>/`: the first frame as
    /// `<name>.png`, or with `--godot` the Godot 4 files of `export-godot`.
    ExportAll {
        path: PathBuf,
        dir: PathBuf,
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Write Godot 4 SpriteFrames (`.png`, `.tres`, `.json`) per artboard.
        #[arg(long)]
        godot: bool,
    },
    /// Export a spritesheet PNG plus JSON metadata (`<output>.json`).
    ExportSheet {
        path: PathBuf,
        output: PathBuf,
        /// Artboard ID (default: the first one).
        #[arg(long)]
        artboard: Option<u32>,
        #[command(flatten)]
        sheet: SheetArgs,
    },
    /// Export an animated GIF (exact colors, at most 255 per frame).
    ExportGif {
        path: PathBuf,
        output: PathBuf,
        /// Artboard ID (default: the first one).
        #[arg(long)]
        artboard: Option<u32>,
        /// Only this tag, with its direction and loop setting.
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, default_value_t = 1)]
        scale: u32,
    },
    /// Export for Godot 4: `<name>.png`, a `<name>.tres` SpriteFrames
    /// resource with one animation per tag, and `<name>.json`.
    ExportGodot {
        path: PathBuf,
        out_dir: PathBuf,
        /// Artboard ID (default: the first one).
        #[arg(long)]
        artboard: Option<u32>,
        /// File stem (defaults to the artboard name).
        #[arg(long)]
        name: Option<String>,
        /// Texture directory inside the Godot project, e.g. `res://sprites`.
        /// By default the .tres refers to the PNG relative to itself.
        #[arg(long)]
        res_dir: Option<String>,
        #[command(flatten)]
        sheet: SheetArgs,
    },
}

#[derive(clap::Args)]
struct SheetArgs {
    /// `grid` packs every frame once; `rows` puts each tag on its own row.
    #[arg(long, default_value = "grid", value_parser = ["grid", "rows"])]
    layout: String,
    /// Grid columns (default: near-square).
    #[arg(long)]
    columns: Option<u32>,
    #[arg(long, default_value_t = 1)]
    scale: u32,
    /// Transparent pixels around and between frames.
    #[arg(long, default_value_t = 0)]
    padding: u32,
    /// Repeat each frame's edge pixels outwards by this many pixels.
    #[arg(long, default_value_t = 0)]
    extrude: u32,
    /// Sprite origin in frame pixels as `X,Y` (default: centre).
    #[arg(long, value_parser = parse_pivot)]
    pivot: Option<Pivot>,
}

impl SheetArgs {
    fn options(&self) -> SheetOptions {
        SheetOptions {
            layout: if self.layout == "rows" { SheetLayout::RowsPerTag } else { SheetLayout::Grid },
            columns: self.columns,
            scale: self.scale,
            padding: self.padding,
            extrude: self.extrude,
        }
    }
}

fn parse_pivot(s: &str) -> std::result::Result<Pivot, String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    let num = |v: &str| v.trim().parse::<f64>().map_err(|e| e.to_string());
    Ok(Pivot { x: num(x)?, y: num(y)? })
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
            let project = Project::new(name, width, height)?;
            format::save(&project, &path, WriteOptions::default())?;
            println!("{}", path.display());
        }
        Cmd::Info { path } => {
            let editor = ProjectEditor::new(format::load(&path)?);
            println!("{}", serde_json::to_string_pretty(&editor.summary())?);
        }
        Cmd::Apply { path, file, artboard, dry_run } => {
            let mut text = String::new();
            if file.as_os_str() == "-" {
                std::io::stdin().read_to_string(&mut text)?;
            } else {
                text = std::fs::read_to_string(&file)?;
            }
            let value: serde_json::Value = serde_json::from_str(&text)?;
            let mut editor = ProjectEditor::new(format::load(&path)?);
            let artboard = resolve_artboard(editor.project(), artboard)?;
            let command = match value {
                serde_json::Value::Array(items) => ProjectCommand::Batch {
                    label: None,
                    commands: items.into_iter().map(|v| parse_command(v, artboard)).collect::<Result<_>>()?,
                },
                v => parse_command(v, artboard)?,
            };
            let outcome = editor.execute(command).map_err(|e| format!("{} ({})", e, e.code()))?;
            if !dry_run && outcome.changed {
                format::save(editor.project(), &path, WriteOptions::default())?;
            }
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        Cmd::ExportPng { path, output, artboard, frame, scale } => {
            let project = format::load(&path)?;
            let doc = project.document(resolve_artboard(&project, artboard)?)?;
            let frame = doc.frames().get(frame).ok_or("frame index out of range")?.id;
            dotify_export::export_png(doc, PngExportOptions { frame, scale }, &output)?;
            println!("{}", output.display());
        }
        Cmd::ExportAll { path, dir, scale, godot } => {
            let project = format::load(&path)?;
            let exporter: Box<dyn ArtboardExporter> = if godot {
                let sheet = SheetOptions { scale, ..SheetOptions::default() };
                Box::new(GodotArtboardExporter { options: GodotOptions { sheet, ..GodotOptions::default() } })
            } else {
                Box::new(PngArtboardExporter { scale })
            };
            for written in dotify_export::project::export_project(&project, &dir, exporter.as_ref())? {
                println!("{}", written.display());
            }
        }
        Cmd::ExportSheet { path, output, artboard, sheet } => {
            let project = format::load(&path)?;
            let doc = project.document(resolve_artboard(&project, artboard)?)?;
            let json = dotify_export::export_spritesheet(doc, &sheet.options(), sheet.pivot, &output)?;
            println!("{}\n{}", output.display(), json.display());
        }
        Cmd::ExportGif { path, output, artboard, tag, scale } => {
            let project = format::load(&path)?;
            let doc = project.document(resolve_artboard(&project, artboard)?)?;
            let report = dotify_export::gif::export_gif(doc, &GifOptions { scale, tag }, &output)?;
            if report.alpha_rounded_pixels > 0 {
                eprintln!(
                    "warning: {} semi-transparent pixels were rounded (GIF has no partial transparency)",
                    report.alpha_rounded_pixels
                );
            }
            println!("{}", output.display());
        }
        Cmd::ExportGodot { path, out_dir, artboard, name, res_dir, sheet } => {
            let project = format::load(&path)?;
            let doc = project.document(resolve_artboard(&project, artboard)?)?;
            let opts = GodotOptions { name, sheet: sheet.options(), res_dir, pivot: sheet.pivot };
            let files = dotify_export::godot::export_godot(doc, &opts, &out_dir)?;
            println!("{}\n{}\n{}", files.png.display(), files.tres.display(), files.json.display());
        }
    }
    Ok(())
}

fn resolve_artboard(project: &Project, id: Option<u32>) -> Result<ArtboardId> {
    match id {
        Some(id) => Ok(project.artboard(ArtboardId(id))?.id()),
        None => Ok(project.artboards()[0].id()),
    }
}

/// A project command, or a document command applied to `artboard`.
fn parse_command(value: serde_json::Value, artboard: ArtboardId) -> Result<ProjectCommand> {
    match serde_json::from_value::<ProjectCommand>(value.clone()) {
        Ok(cmd) => Ok(cmd),
        Err(project_err) => match serde_json::from_value::<Command>(value) {
            Ok(command) => Ok(ProjectCommand::Edit { artboard, command }),
            Err(doc_err) if project_err.to_string().contains("unknown variant") => Err(doc_err.into()),
            Err(_) => Err(project_err.into()),
        },
    }
}
