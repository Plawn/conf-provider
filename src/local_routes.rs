use crate::fs::local::BasicFsFileProvider;
use crate::{config::LocalAppState, metrics, utils::GetError};

use std::time::Instant;
use xitca_web::handler::params::Params;
use xitca_web::handler::state::StateRef;

pub async fn get_data(
    Params((format, path)): Params<(String, String)>,
    StateRef(state): StateRef<'_, LocalAppState<BasicFsFileProvider>>,
) -> Result<String, GetError> {
    let start = Instant::now();

    let d = state
        .dag
        .get_rendered(&path)
        .await
        .map_err(|_| GetError::MissingItem)?;

    let result = state
        .writer
        .write(&format, &d)
        .ok_or(GetError::BadRequest)?
        .map_err(|_| GetError::BadRequest);

    metrics::record_render(&format, result.is_ok(), start.elapsed());
    result
}

pub async fn reload(
    StateRef(state): StateRef<'_, LocalAppState<BasicFsFileProvider>>,
) -> Result<String, GetError> {
    let result = state.dag.reload().await;
    metrics::record_reload(result.is_ok());
    result.expect("failed to reload");
    Ok("OK".to_string())
}

pub async fn metrics_handler(
    StateRef(state): StateRef<'_, LocalAppState<BasicFsFileProvider>>,
) -> String {
    state.metrics.render()
}
