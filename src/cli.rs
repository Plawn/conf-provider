//! Unified CLI for konf development tools
//!
//! Combines the config renderer and LSP server into a single binary.
//!
//! Usage:
//!   konf render -f /path/to/configs -n myconfig -o yaml
//!   konf lsp

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};

use konf_provider::{
    fs::local::BasicFsFileProvider,
    loader::MultiLoader,
    loaders::yaml::YamlLoader,
    render::Dag,
    writer::{
        MultiWriter, docker_env::DockerEnvVarWriter, env::EnvVarWriter, json::JsonWriter,
        properties::PropertiesWriter, toml::TomlWriter, yaml::YamlWriter,
    },
};

#[derive(Debug, Parser)]
#[command(
    name = "konf",
    version,
    about = "Konf development tools - config renderer and LSP server"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Render and test konf configuration files locally
    Render {
        /// Folder containing configuration files
        #[arg(long, short)]
        folder: PathBuf,

        /// File to render (without extension, e.g., "app" for "app.yaml")
        #[arg(long, short = 'n')]
        file: String,

        /// Output format (yaml, json, env, properties, toml, docker_env)
        #[arg(long, short = 'o', default_value = "yaml")]
        format: String,
    },

    /// Start the Language Server Protocol (LSP) server
    Lsp,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Render { folder, file, format } => {
            run_render(folder, file, format)
        }
        Commands::Lsp => {
            run_lsp()
        }
    }
}

fn run_render(folder: PathBuf, file: String, format: String) -> anyhow::Result<()> {
    let multiloader = Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})]));
    let multiwriter = MultiWriter::new(vec![
        YamlWriter::new_boxed(),
        JsonWriter::new_boxed(),
        EnvVarWriter::new_boxed(),
        PropertiesWriter::new_boxed(),
        TomlWriter::new_boxed(),
        DockerEnvVarWriter::new_boxed(),
    ]);

    let rt = tokio::runtime::Runtime::new()?;

    let dag = rt
        .block_on(Dag::new(
            BasicFsFileProvider::new(folder.clone()),
            multiloader,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to load configs from {:?}: {}", folder, e))?;

    let rendered = rt
        .block_on(dag.get_rendered(&file))
        .map_err(|e| anyhow::anyhow!("Failed to render '{}': {}", file, e))?;

    let output = multiwriter
        .write(&format, &rendered)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown format '{}'. Supported formats: yaml, json, env, properties, toml, docker_env",
                format
            )
        })?
        .map_err(|e| anyhow::anyhow!("Failed to serialize to {}: {}", format, e))?;

    println!("{}", output);
    Ok(())
}

fn run_lsp() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(konf_provider::lsp::run_lsp());
    Ok(())
}
