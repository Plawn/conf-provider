use dashmap::Entry;

use crate::{
    DagEntry,
    authorizer::Authorizer,
    config::GitAppState,
    fs::git::{GitFileProvider, list_all_commit_hashes},
    loader::MultiLoader,
    render::Dag,
    utils::GetError,
};

use std::sync::Arc;

use xitca_web::handler::params::Params;
use xitca_web::handler::state::StateRef;

pub async fn new_dag_git(
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

// fix proper token sourcing
// -> should be in headers
pub async fn get_data_git(
    Params((commit, format, token, path)): Params<(String, String, String, String)>,
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
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
pub async fn reload_git(
    StateRef(state): StateRef<'_, GitAppState<GitFileProvider>>,
) -> Result<String, GetError> {
    // TODO: add fetch before list
    let commits = list_all_commit_hashes(&state.repo_config.path, &state.repo_config.branch, true)
        .map_err(|_| GetError::FormatError)?;
    state.commits.store(Arc::from(commits));
    Ok("OK".to_string())
}
