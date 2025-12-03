use std::{collections::HashSet, path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use metrics_exporter_prometheus::PrometheusHandle;

use crate::{
    DagEntry,
    fs::{FileProvider, git::Creds},
    loader::MultiLoader,
    render::Dag,
    writer::MultiWriter,
};

#[derive(Debug)]
pub struct RepoConfig {
    pub url: String,
    pub branch: String,
    pub creds: Option<Creds>,
}

pub struct GitAppState<P: FileProvider> {
    pub dag: DashMap<String, DagEntry<P>>,
    pub writer: Arc<MultiWriter>,
    pub commits: ArcSwap<HashSet<String>>,
    pub multiloader: Arc<MultiLoader>,
    pub repo_config: RepoConfig,
    pub metrics: Arc<PrometheusHandle>,
}

#[derive(Debug, Clone)]
pub struct LocalAppState<P: FileProvider> {
    pub dag: Dag<P>,
    pub writer: Arc<MultiWriter>,
    pub multiloader: Arc<MultiLoader>,
    pub folder: PathBuf,
    pub metrics: Arc<PrometheusHandle>,
}
