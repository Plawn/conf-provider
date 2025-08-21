use arc_swap::ArcSwap;
use dashmap::DashMap;
use konf_provider::fs::FileProvider;
use konf_provider::fs::git::{GitFileProvider, list_all_commit_hashes, setup_repository};
use konf_provider::loader::MultiLoader;
use konf_provider::loaders::yaml::YamlLoader;
use konf_provider::render::Dag;
use konf_provider::utils;
use konf_provider::utils::MyError;
use konf_provider::writer::MultiWriter;
use konf_provider::writer::{json::JsonWriter, yaml::YamlWriter};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use xitca_web::handler::params::Params;
use xitca_web::middleware::tower_http_compat::TowerHttpCompat;
use xitca_web::{
    App,
    handler::{handler_service, state::StateRef},
    route::get,
};

#[derive(Debug)]
struct RepoConfig {
    url: String,
    path: PathBuf,
}

#[derive(Debug)]
struct AppState<P: FileProvider> {
    dag: DashMap<String, Dag<P>>,
    writer: Arc<MultiWriter>,
    commits: ArcSwap<HashSet<String>>,
    multiloader: Arc<MultiLoader>,
    repo_config: RepoConfig,
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

    // should be a param
    let url = "https://github.com/Plawn/configuration.git";

    let repo_path = setup_repository(url).unwrap();
    // let folder = "example".parse().expect("failed to parse folder path");

    let commits = list_all_commit_hashes(&repo_path).unwrap();

    // let d =
    //     Dag::new(BasicFsFileProvider::new(folder), multiloader).expect("failed to read directory");

    let multiwriter = MultiWriter::new(vec![Box::new(YamlWriter {}), Box::new(JsonWriter {})]);

    let state = Arc::from(AppState {
        repo_config: RepoConfig {
            path: repo_path,
            url: url.to_string(),
        },
        dag: DashMap::new(),
        writer: Arc::from(multiwriter),
        commits: ArcSwap::from(Arc::from(commits)),
        multiloader: Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})])),
    });
    // let rt = tokio::runtime::Runtime::new().unwrap();
    // rt.block_on(state.dag.reload())
    //     .expect("failed to initialyze");

    App::new()
        .with_state(state)
        .at("/live", get(handler_service(async || "OK")))
        .at("/reload", get(handler_service(reload)))
        .at(
            "/data/:commit/:format/*rest", // url is ok
            get(handler_service(get_data)),
        )
        .enclosed_fn(utils::error_handler)
        .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
        .serve()
        .bind(format!("0.0.0.0:{}", &4000))?
        .run()
        .wait()
}

// add security
// check token from header
// should have a token which authorize to read a prefix
async fn get_data(
    Params((commit, format, path)): Params<(String, String, String)>,
    StateRef(state): StateRef<'_, AppState<GitFileProvider>>,
) -> Result<String, MyError> {
    if !state.commits.load().contains(&commit) {
        return Err(MyError(anyhow::Error::msg("Commit not found")));
    }
    let dag = match state.dag.contains_key(&commit) {
        true => state.dag.get(&commit).unwrap(),
        false => {
            let fs = GitFileProvider::new(&state.repo_config.url, &commit)
                .await
                .unwrap();
            let d = Dag::new(fs, state.multiloader.clone()).await.unwrap();
            state.dag.insert(commit.clone(), d);
            state.dag.get(&commit).unwrap()
        }
    };
    let d = dag
        .get_rendered(&path)
        .await
        .map_err(|_| MyError(anyhow::Error::msg("missing item")))?;
    return state
        .writer
        .write(&format, &d)
        .ok_or(MyError(anyhow::Error::msg("Failed to get format")));
}

/// reload the commit set
async fn reload(
    StateRef(state): StateRef<'_, AppState<GitFileProvider>>,
) -> Result<String, MyError> {
    let commits = list_all_commit_hashes(&state.repo_config.path).unwrap();
    state.commits.store(Arc::from(commits));
    Ok("OK".to_string())
}

// The state that will be swapped atomically. It must be cheap to clone the Arc, not the data.
