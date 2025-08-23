use arc_swap::ArcSwap;
use clap::Parser;
use dashmap::DashMap;

use konf_provider::main_local::{get_data_local, reload_local};
use konf_provider::writer::env::EnvVarWriter;
use konf_provider::writer::properties::PropertiesWriter;
use konf_provider::writer::toml::TomlWriter;
use konf_provider::{
    config::{GitAppState, LocalAppState, RepoConfig},
    fs::{
        fs::BasicFsFileProvider,
        git::{list_all_commit_hashes, setup_repository},
    },
    loader::MultiLoader,
    loaders::yaml::YamlLoader,
    main_git::{get_data_git, reload_git},
    render::Dag,
    utils::{self},
    writer::{MultiWriter, json::JsonWriter, yaml::YamlWriter},
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
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
    },
    Local {
        #[arg(long)]
        folder: PathBuf,
    },
}

fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let multiwriter = MultiWriter::new(vec![
        YamlWriter::new_boxed(),
        JsonWriter::new_boxed(),
        EnvVarWriter::new_boxed(),
        PropertiesWriter::new_boxed(),
        TomlWriter::new_boxed(),
    ]);

    match args {
        Args::Local { folder } => {
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
            };

            App::new()
                .with_state(state)
                .at("/live", get(handler_service(async || "OK")))
                .at("/reload", get(handler_service(reload_local)))
                .at("/data/:format/*rest", get(handler_service(get_data_local)))
                .enclosed_fn(utils::error_handler)
                .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
                .serve()
                .bind(format!("0.0.0.0:{}", &4000))?
                .run()
                .wait()
        }
        Args::Git { repo_url, branch } => {
            let repo_path =
                setup_repository(&repo_url, &branch).expect("failed to initialyze repository");

            let commits = list_all_commit_hashes(&repo_url, &branch, false).unwrap();

            let state = Arc::from(GitAppState {
                repo_config: RepoConfig {
                    path: repo_path,
                    url: repo_url.to_string(),
                    branch: branch.to_string(),
                },
                dag: DashMap::new(),
                writer: Arc::from(multiwriter),
                commits: ArcSwap::from(Arc::from(commits)),
                multiloader: Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})])),
            });

            App::new()
                .with_state(state)
                .at("/live", get(handler_service(async || "OK")))
                .at("/reload", get(handler_service(reload_git)))
                .at(
                    "/data/:commit/:token/:format/*rest",
                    get(handler_service(get_data_git)),
                )
                .enclosed_fn(utils::error_handler)
                .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
                .serve()
                .bind(format!("0.0.0.0:{}", &4000))?
                .run()
                .wait()
        }
    }
}
