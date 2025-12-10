use async_once_cell::OnceCell;
use dashmap::Entry;

use crate::{
    DagEntry,
    authorizer::Authorizer,
    config::GitAppState,
    fs::git::{GitFileProvider, clone_or_update, is_valid_commit_hash, list_all_commit_hashes},
    loader::MultiLoader,
    metrics,
    render::Dag,
    utils::GetError,
};

use std::sync::Arc;
use std::time::Instant;

use xitca_web::handler::state::StateRef;
use xitca_web::{handler::params::Params, http::HeaderMap};

use anyhow::Result;
use tokio::sync::Mutex;

async fn new_dag_git(
    repo_url: &str,
    commit: &str,
    multiloader: Arc<MultiLoader>,
) -> Result<DagEntry<GitFileProvider>, GetError> {
    let fs = GitFileProvider::new(repo_url, commit)
        .await
        .map_err(|e| GetError::DagInitError {
            commit: commit.to_string(),
            reason: format!("failed to create git file provider: {e}"),
        })?;
    let authorizer = Authorizer::new(&fs, &multiloader).await;
    let d = Dag::new(fs, multiloader)
        .await
        .map_err(|e| GetError::DagInitError {
            commit: commit.to_string(),
            reason: format!("failed to load config files: {e}"),
        })?;
    Ok(DagEntry { dag: d, authorizer })
}

pub async fn get_data(
    headers: HeaderMap,
    Params((commit, format, path)): Params<(String, String, String)>,
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
    let start = Instant::now();

    let token = headers
        .get("token")
        .ok_or(GetError::Unauthorized {
            reason: "missing 'token' header".to_string(),
        })?
        .to_str()
        .map_err(|_| GetError::BadRequest {
            reason: "invalid 'token' header: must be valid UTF-8".to_string(),
        })?;

    // Validate commit hash format before checking if it exists
    if !is_valid_commit_hash(&commit) {
        return Err(GetError::BadRequest {
            reason: format!("invalid commit hash format: '{commit}' (expected 40-char hex string)"),
        });
    }

    if !state.commits.load().contains(&commit) {
        return Err(GetError::CommitNotFound {
            commit: commit.clone(),
        });
    }

    let dag = match state.dag.entry(commit.clone()) {
        Entry::Occupied(entry) => {
            metrics::record_git_cache(true);
            entry.into_ref()
        }
        Entry::Vacant(entry) => {
            metrics::record_git_cache(false);
            let d = new_dag_git(&state.repo_config.url, &commit, state.multiloader.clone()).await?;
            entry.insert(d)
        }
    };

    if !dag.authorizer.authorize(&path, token) {
        return Err(GetError::Forbidden { path: path.clone() });
    }

    let rendered = dag
        .dag
        .get_rendered(&path)
        .await
        .map_err(|e| GetError::RenderError {
            path: path.clone(),
            reason: e.to_string(),
        })?;

    let result = state
        .writer
        .write(&format, &rendered)
        .ok_or_else(|| GetError::BadRequest {
            reason: format!("unknown output format: '{format}'"),
        })?
        .map_err(|e| GetError::InternalError {
            reason: format!("failed to serialize to '{format}': {e}"),
        });

    metrics::record_render(&format, result.is_ok(), start.elapsed());
    result
}

/// We wrap the reload lock in a OnceCell, so it's globally available.
static RELOAD_CELL: OnceCell<Arc<Mutex<()>>> = OnceCell::new();

/// Ensure the global lock exists.
async fn reload_lock() -> &'static Arc<Mutex<()>> {
    (RELOAD_CELL
        .get_or_init(async { Arc::new(Mutex::new(())) })
        .await) as _
}

/// reload the commit set
pub async fn reload(
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
    let lock = reload_lock().await.clone();
    if let Ok(guard) = lock.try_lock() {
        let result = clone_or_update(
            &state.repo_config.url,
            &state.repo_config.branch,
            &state.repo_config.creds,
        )
        .await;

        metrics::record_reload(result.is_ok());

        if let Err(e) = &result {
            return Err(GetError::InternalError {
                reason: format!("failed to update git repository: {e}"),
            });
        }

        let commits = list_all_commit_hashes(&state.repo_config.url).map_err(|e| {
            GetError::InternalError {
                reason: format!("failed to list commit hashes: {e}"),
            }
        })?;
        state.commits.store(Arc::from(commits));
        drop(guard);
    }

    Ok("OK".to_string())
}

pub async fn metrics_handler(
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> String {
    state.metrics.render()
}
