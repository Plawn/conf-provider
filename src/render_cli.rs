use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tokio::runtime::Runtime;

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

#[derive(Debug, clap::Parser)]
#[command(
    name = "konf-render",
    version,
    about = "Render and test konf configuration files locally"
)]
struct Args {
    /// Folder containing configuration files
    #[arg(long, short)]
    folder: PathBuf,

    /// File to render (without extension, e.g., "app" for "app.yaml")
    #[arg(long, short = 'n')]
    file: String,

    /// Output format (yaml, json, env, properties, toml, docker_env)
    #[arg(long, short = 'o', default_value = "yaml")]
    format: String,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let multiloader = Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})]));
    let multiwriter = MultiWriter::new(vec![
        YamlWriter::new_boxed(),
        JsonWriter::new_boxed(),
        EnvVarWriter::new_boxed(),
        PropertiesWriter::new_boxed(),
        TomlWriter::new_boxed(),
        DockerEnvVarWriter::new_boxed(),
    ]);

    let rt = Runtime::new()?;

    let dag = rt
        .block_on(Dag::new(
            BasicFsFileProvider::new(args.folder.clone()),
            multiloader,
        ))
        .map_err(|e| anyhow::anyhow!("Failed to load configs from {:?}: {}", args.folder, e))?;

    let rendered = rt
        .block_on(dag.get_rendered(&args.file))
        .map_err(|e| anyhow::anyhow!("Failed to render '{}': {}", args.file, e))?;

    let output = multiwriter
        .write(&args.format, &rendered)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown format '{}'. Supported formats: yaml, json, env, properties, toml, docker_env",
                args.format
            )
        })?
        .map_err(|e| anyhow::anyhow!("Failed to serialize to {}: {}", args.format, e))?;

    println!("{}", output);
    Ok(())
}
