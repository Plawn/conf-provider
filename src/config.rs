use std::{collections::HashSet, path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use dashmap::DashMap;

use crate::{DagEntry, fs::FileProvider, loader::MultiLoader, render::Dag, writer::MultiWriter};

#[derive(Debug)]
pub struct RepoConfig {
    pub url: String,
    pub branch: String,
    pub path: PathBuf,
}
#[derive(Debug)]
pub struct GitAppState<P: FileProvider> {
    pub dag: DashMap<String, DagEntry<P>>,
    pub writer: Arc<MultiWriter>,
    pub commits: ArcSwap<HashSet<String>>,
    pub multiloader: Arc<MultiLoader>,
    pub repo_config: RepoConfig,
}

#[derive(Debug, Clone)]
pub struct LocalAppState<P: FileProvider> {
    pub dag: Dag<P>,
    pub writer: Arc<MultiWriter>,
    pub multiloader: Arc<MultiLoader>,
    pub folder: PathBuf,
}
