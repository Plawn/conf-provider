use crate::fs::fs::BasicFsFileProvider;
use crate::{config::LocalAppState, utils::GetError};

use xitca_web::handler::params::Params;
use xitca_web::handler::state::StateRef;

pub async fn get_data_local(
    Params((format, path)): Params<(String, String)>,
    StateRef(state): StateRef<'_, LocalAppState<BasicFsFileProvider>>,
) -> Result<String, GetError> {
    let d = state
        .dag
        .get_rendered(&path)
        .await
        .map_err(|_| GetError::MissingItem)?;
    state.writer.write(&format, &d).ok_or(GetError::FormatError)
}

pub async fn reload_local(
    StateRef(state): StateRef<'_, LocalAppState<BasicFsFileProvider>>,
) -> Result<String, GetError> {
    state.dag.reload().await.expect("failed to reload");
    Ok("reload".to_string())
}
