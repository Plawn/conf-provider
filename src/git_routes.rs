use async_once_cell::OnceCell;
use dashmap::Entry;

use crate::{
    DagEntry,
    authorizer::Authorizer,
    config::GitAppState,
    fs::git::{GitFileProvider, clone_or_update, list_all_commit_hashes},
    loader::MultiLoader,
    render::Dag,
    utils::GetError,
};

use std::sync::Arc;

use xitca_web::handler::state::StateRef;
use xitca_web::{handler::params::Params, http::HeaderMap};

use anyhow::Result;
use tokio::sync::Mutex;

async fn new_dag_git(
    repo_url: &str,
    commit: &str,
    multiloader: Arc<MultiLoader>,
) -> Result<DagEntry<GitFileProvider>, GetError> {
    let fs = GitFileProvider::new(repo_url, &commit)
        .await
        .map_err(|_| GetError::Unknown)?; // should never happen, we already checked
    let authorizer = Authorizer::new(&fs, &multiloader).await;
    let d = Dag::new(fs, multiloader)
        .await
        .map_err(|_| GetError::Unknown)?;
    Ok(DagEntry { dag: d, authorizer })
}

pub async fn get_data(
    headers: HeaderMap,
    Params((commit, format, path)): Params<(String, String, String)>,
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
    let token = headers
        .get("token")
        .ok_or(GetError::Unauthorized)?
        .to_str()
        .map_err(|_| GetError::FormatError)?;

    if !state.commits.load().contains(&commit) {
        return Err(GetError::CommitNotFound);
    }

    let dag = match state.dag.entry(commit.clone()) {
        Entry::Occupied(entry) => entry.into_ref(),
        Entry::Vacant(entry) => {
            let d = new_dag_git(&state.repo_config.url, &commit, state.multiloader.clone()).await?;
            entry.insert(d)
        }
    };

    match dag.authorizer.authorize(&path, &token) {
        true => {
            let d = dag
                .dag
                .get_rendered(&path)
                .await
                .map_err(|_| GetError::MissingItem)?;

            state.writer.write(&format, &d).ok_or(GetError::FormatError)
        }
        false => Err(GetError::Unauthorized),
    }
}

/// We wrap the reload lock in a OnceCell, so it's globally available.
static RELOAD_CELL: OnceCell<Arc<Mutex<()>>> = OnceCell::new();

/// Ensure the global lock exists.
async fn reload_lock() -> &'static Arc<Mutex<()>> {
    let e = RELOAD_CELL
        .get_or_init(async { Arc::new(Mutex::new(())) })
        .await;
    e
}

/// reload the commit set
pub async fn reload(
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
    // TODO: add fetch before list
    let lock = reload_lock().await.clone();
    if let Ok(guard) = lock.try_lock() {
        clone_or_update(
            &state.repo_config.url,
            &state.repo_config.branch,
            &state.repo_config.creds,
        )
        .await
        .map_err(|_| GetError::Unauthorized)?;
        let commits =
            list_all_commit_hashes(&state.repo_config.url).map_err(|_| GetError::FormatError)?;
        state.commits.store(Arc::from(commits));
        drop(guard);
    } else {
    }

    Ok("OK".to_string())
}
