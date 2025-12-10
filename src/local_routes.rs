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

    let rendered = state
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
