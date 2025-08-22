use arc_swap::ArcSwap;
use dashmap::{DashMap, Entry};

use konf_provider::{
    authorizer::Authorizer,
    fs::{
        FileProvider,
        git::{GitFileProvider, list_all_commit_hashes, setup_repository},
    },
    loader::MultiLoader,
    loaders::yaml::YamlLoader,
    render::Dag,
    utils::{self, GetError, MyError},
    writer::{MultiWriter, json::JsonWriter, yaml::YamlWriter},
};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use xitca_web::{
    App,
    handler::{handler_service, state::StateRef},
    route::get,
};
use xitca_web::{
    error::Error,
    handler::header::{self, HeaderRef},
    http::{Method, WebResponse},
};
use xitca_web::{handler::params::Params, http::Request};
use xitca_web::{http::RequestExt, middleware::tower_http_compat::TowerHttpCompat};

#[derive(Debug)]
struct RepoConfig {
    url: String,
    branch: String,
    path: PathBuf,
}
#[derive(Debug)]
struct DagEntry<P: FileProvider> {
    dag: Dag<P>,
    authorizer: Authorizer,
}

#[derive(Debug)]
struct AppState<P: FileProvider> {
    dag: DashMap<String, DagEntry<P>>,
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
    let branch = "main";
    let repo_path = setup_repository(url, branch).unwrap();
    // let folder = "example".parse().expect("failed to parse folder path");

    let commits = list_all_commit_hashes(&repo_path, branch, false).unwrap();

    // keep this to work locally
    // let d =
    //     Dag::new(BasicFsFileProvider::new(folder), multiloader).expect("failed to read directory");

    let multiwriter = MultiWriter::new(vec![Box::new(YamlWriter {}), Box::new(JsonWriter {})]);

    let state: Arc<AppState<GitFileProvider>> = Arc::from(AppState {
        repo_config: RepoConfig {
            path: repo_path,
            url: url.to_string(),
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
        .at("/reload", get(handler_service(reload)))
        .at(
            "/data/:commit/:format/:token/*rest", // url is ok
            get(handler_service(get_data)),
        )
        .enclosed_fn(utils::error_handler)
        .enclosed(TowerHttpCompat::new(TraceLayer::new_for_http()))
        .serve()
        .bind(format!("0.0.0.0:{}", &4000))?
        .run()
        .wait()
}

async fn new_dag(
    repo_url: &str,
    commit: &str,
    multiloader: Arc<MultiLoader>,
) -> Result<DagEntry<GitFileProvider>, GetError> {
    let fs = GitFileProvider::new(repo_url, &commit)
        .await
        .map_err(|_| GetError::CommitNotFound)?; // should never happen, we already checked
    let authorizer = Authorizer::new(&fs, &multiloader).await;
    let d = Dag::new(fs, multiloader)
        .await
        .map_err(|_| GetError::MissingItem)?;
    Ok(DagEntry { dag: d, authorizer })
}

use xitca_web::WebContext;
use xitca_web::{
    body::ResponseBody,
    handler::{Responder, handler_sync_service},
    service::fn_service,
};
// function with concrete typed input and output where http types
// are handled manually and explicitly.
// it can also be seen as a desugar of previous example of
// handler_service(high)
async fn low(ctx: WebContext<'_>) -> Result<WebResponse, Error> {
    // extract method from request context.
    let method = ctx.extract().await?;
    // execute high level abstraction example function.
    let str = high(method).await;
    // convert string literal to http response.
    str.respond(ctx).await
}
// magic function with arbitrary receiver type and output type
// that can be extracted from http requests and packed into http
// response.
async fn high(method: &Method) -> &'static str {
    // extract http method from http request.
    assert_eq!(method, Method::GET);
    // pack string literal into http response.
    "high level"
}
// fix proper token sourcing
// -> should be in headers
async fn get_data(
    Params((commit, format, token, path)): Params<(String, String, String, String)>,
    StateRef(state): StateRef<'_, AppState<GitFileProvider>>,
) -> Result<String, GetError> {
    if !state.commits.load().contains(&commit) {
        return Err(GetError::CommitNotFound);
    }

    let dag = match state.dag.entry(commit.clone()) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            let d = new_dag(&state.repo_config.url, &commit, state.multiloader.clone()).await?;
            entry.insert(d)
        }
    };

    if dag.authorizer.authorize(&path, &token) {
        let d = dag
            .dag
            .get_rendered(&path)
            .await
            .map_err(|_| GetError::MissingItem)?;

        state.writer.write(&format, &d).ok_or(GetError::FormatError)
    } else {
        Err(GetError::CommitNotFound)
    }
}

/// reload the commit set
async fn reload(
    StateRef(state): StateRef<'_, AppState<GitFileProvider>>,
) -> Result<String, MyError> {
    // TODO: add fetch before list
    let commits =
        list_all_commit_hashes(&state.repo_config.path, &state.repo_config.branch, true).unwrap();
    state.commits.store(Arc::from(commits));
    Ok("OK".to_string())
}
