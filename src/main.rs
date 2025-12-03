use arc_swap::ArcSwap;
use clap::Parser;
use dashmap::DashMap;

use konf_provider::fs::git::Creds;
use konf_provider::local_routes;
use konf_provider::metrics::init_metrics;
use konf_provider::telemetry::{init_tracing, TelemetryConfig};
use konf_provider::writer::docker_env::DockerEnvVarWriter;
use konf_provider::writer::env::EnvVarWriter;
use konf_provider::writer::properties::PropertiesWriter;
use konf_provider::writer::toml::TomlWriter;
use konf_provider::{
    config::{GitAppState, LocalAppState, RepoConfig},
    fs::{
        local::BasicFsFileProvider,
        git::{clone_or_update, list_all_commit_hashes},
    },
    git_routes,
    loader::MultiLoader,
    loaders::yaml::YamlLoader,
    render::Dag,
    utils::{self},
    writer::{MultiWriter, json::JsonWriter, yaml::YamlWriter},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tower_http::trace::TraceLayer;
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{App, handler::handler_service, route::get};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
enum Args {
    Git {
        #[arg(long)]
        repo_url: String,
        #[arg(long)]
        branch: String,

        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        password: Option<String>,

        /// Port to listen on
        #[arg(long, short, default_value = "4000", env = "KONF_PORT")]
        port: u16,
    },
    Local {
        #[arg(long)]
        folder: PathBuf,

        /// Port to listen on
        #[arg(long, short, default_value = "4000", env = "KONF_PORT")]
        port: u16,
    },
}

fn make_git_creds(username: Option<String>, password: Option<String>) -> Option<Creds> {
    if let Some(u) = username
        && let Some(p) = password
    {
        return Some(Creds::new(u, p));
    }
    None
}


fn main() -> std::io::Result<()> {
    // Initialize tracing with optional OpenTelemetry export
    let _tracer_provider = init_tracing(TelemetryConfig::default());

    // Initialize Prometheus metrics
    let prometheus_handle = Arc::new(init_metrics());

    let args = Args::parse();
    let multiwriter = MultiWriter::new(vec![
        YamlWriter::new_boxed(),
        JsonWriter::new_boxed(),
        EnvVarWriter::new_boxed(),
        PropertiesWriter::new_boxed(),
        TomlWriter::new_boxed(),
        DockerEnvVarWriter::new_boxed(),
    ]);

    match args {
        Args::Local { folder, port } => {
            let multiloader = Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})]));
            let rt = Runtime::new().expect("failed to get tokio runtime");

            // Run the async function in sync context
            let dag = rt
                .block_on(Dag::new(
                    BasicFsFileProvider::new(folder.clone()),
                    multiloader.clone(),
                ))
                .expect("failed to read directory");

            let state = LocalAppState {
                folder,
                dag,
                writer: Arc::from(multiwriter),
                multiloader,
                metrics: prometheus_handle.clone(),
            };

            App::new()
                .with_state(state)
                .at("/live", get(handler_service(async || "OK")))
                .at("/metrics", get(handler_service(local_routes::metrics_handler)))
                .at("/reload", get(handler_service(local_routes::reload)))
                .at(
                    "/data/:format/*rest",
                    get(handler_service(local_routes::get_data)),
                )
                .enclosed_fn(utils::error_handler)
                .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
                .serve()
                .bind(format!("0.0.0.0:{port}"))?
                .run()
                .wait()
        }
        Args::Git {
            repo_url,
            branch,
            username,
            password,
            port,
        } => {
            let creds = make_git_creds(username, password);
            let creds_clone = creds.clone();
            let rt = Runtime::new()?;
            rt.block_on(clone_or_update(&repo_url, &branch, &creds))
                .expect("failed to initialize repository");

            let commits = list_all_commit_hashes(&repo_url).unwrap();

            let state = Arc::from(GitAppState {
                repo_config: RepoConfig {
                    url: repo_url.to_string(),
                    branch: branch.to_string(),
                    creds: creds_clone,
                },
                dag: DashMap::new(),
                writer: Arc::from(multiwriter),
                commits: ArcSwap::from(Arc::from(commits)),
                multiloader: Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})])),
                metrics: prometheus_handle,
            });

            App::new()
                .with_state(state)
                .at("/live", get(handler_service(async || "OK")))
                .at("/metrics", get(handler_service(git_routes::metrics_handler)))
                .at("/reload", get(handler_service(git_routes::reload)))
                .at(
                    "/data/:commit/:format/*rest",
                    get(handler_service(git_routes::get_data)),
                )
                .enclosed_fn(utils::error_handler)
                .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
                .serve()
                .bind(format!("0.0.0.0:{port}"))?
                .run()
                .wait()
        }
    }
}
