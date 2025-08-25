use anyhow::{Result, anyhow};
use git2::build::RepoBuilder;
use git2::{Cred, Error, FetchOptions, RemoteCallbacks};
use git2::{Oid, Repository};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fs::{DirEntry, FileProvider};

// We implement TryFrom for a standard Path reference
impl TryFrom<&Path> for DirEntry {
    type Error = std::io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid filename")
            })?
            .to_string();

        let full_path = path.to_string_lossy().into_owned();

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        Ok(DirEntry {
            filename,
            full_path,
            ext,
        })
    }
}

#[derive(Clone, Debug)]
pub struct GitFileProvider {
    /// Path to the local clone of the repository.
    repo_path: PathBuf,
    /// The specific commit OID to read files from.
    commit_oid: Oid,
}

fn get_git_storage_directory() -> PathBuf {
    std::env::var("GIT_DIR")
        .ok()
        .and_then(|e| e.parse().ok())
        .unwrap_or("._git_storage".parse().unwrap())
}

pub fn get_git_directory(repo_url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(repo_url.as_bytes());
    let cache_dir_name = hex::encode(hasher.finalize());
    
    get_git_storage_directory().join(cache_dir_name)
}

impl GitFileProvider {
    /// Creates a new GitFileProvider.
    /// This will clone the repository if it's not already cached locally,
    /// or fetch the latest changes if it is.
    pub async fn new(repo_url: &str, commit_hash: &str) -> Result<Self> {
        // 1. Determine a stable cache path from the repository URL.
        // This ensures the same URL always uses the same local directory.
        let repo_path = get_git_directory(repo_url);
        // 2. Clone or fetch the repo. This is a blocking operation.
        let repo_path_clone = repo_path.clone();
        let repo = tokio::task::spawn_blocking(move || {
            if repo_path_clone.exists() {
                // If the repo exists, open it and fetch updates.
                let repo = Repository::open(&repo_path_clone)?;
                Ok(repo)
            } else {
                Err(anyhow!("repo should have been init already"))
            }
        })
        .await??;

        // 3. Verify that the commit exists in the local repository.
        let commit_oid = Oid::from_str(commit_hash)?;
        repo.find_commit(commit_oid)
            .map_err(|e| anyhow::anyhow!("Commit '{}' not found: {}", commit_hash, e))?;

        println!(
            "Successfully initialized provider for commit {}",
            commit_hash
        );

        Ok(Self {
            repo_path,
            commit_oid,
        })
    }
}

impl FileProvider for GitFileProvider {
    /// Loads the content of a single file from the specific commit.
    async fn load(&self, path: &str) -> Option<String> {
        let repo_path = self.repo_path.clone();
        let commit_oid = self.commit_oid;
        let path_str = path.to_string();

        // spawn_blocking is crucial here to avoid blocking the async runtime.
        tokio::task::spawn_blocking(move || {
            let repo = Repository::open(repo_path).ok()?;
            let commit = repo.find_commit(commit_oid).ok()?;
            let tree = commit.tree().ok()?;

            // Find the file in the repository tree for that commit
            let tree_entry = tree.get_path(Path::new(&path_str)).ok()?;
            let blob = tree_entry.to_object(&repo).ok()?.into_blob().ok()?;

            // Git blobs are bytes, we try to convert to UTF-8 string
            String::from_utf8(blob.content().to_vec()).ok()
        })
        .await
        .ok()
        .flatten()
    }

    /// Lists all files in the repository at the specific commit.
    async fn list(&self) -> Vec<DirEntry> {
        let repo_path = self.repo_path.clone();
        let commit_oid = self.commit_oid;

        tokio::task::spawn_blocking(move || {
            let mut entries = Vec::new();
            let Ok(repo) = Repository::open(repo_path) else {
                return entries;
            };
            let Ok(commit) = repo.find_commit(commit_oid) else {
                return entries;
            };
            let Ok(tree) = commit.tree() else {
                return entries;
            };

            // Walk the tree of files recursively
            let _ = tree.walk(git2::TreeWalkMode::PostOrder, |root, entry| {
                // We only care about files (blobs), not directories
                if entry.kind() == Some(git2::ObjectType::Blob)
                    && let Some(filename) = entry.name() {
                        let full_path = Path::new(root).join(filename);
                        if let Ok(dir_entry) = DirEntry::try_from(full_path.as_path()) {
                            entries.push(dir_entry);
                        }
                    }
                git2::TreeWalkResult::Ok
            });

            entries
        })
        .await
        .unwrap_or_default()
    }
}

/// Walks the Git history and collects all reachable commit hashes.
pub fn list_all_commit_hashes(repo_url: &str) -> Result<HashSet<String>, Error> {
    let path = get_git_directory(repo_url);
    let repo = Repository::open(&path)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.push_glob("refs/*")?; // Pushes HEAD, all branches, all tags, all remotes

    revwalk
        .map(|res| res.map(|oid| oid.to_string()))
        .collect::<Result<HashSet<String>, Error>>()
}

#[derive(Debug, Clone)]
pub struct Creds {
    username: String,
    password: String,
}

impl Creds {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

fn create_auth_options(creds: Creds) -> FetchOptions<'static> {
    let mut callbacks = RemoteCallbacks::new();

    // The 'move' closure takes ownership of the credentials (`creds`).
    // This ensures the username and password live as long as the callback does.
    callbacks.credentials(move |_url, _username_from_git, _allowed_types| {
        Cred::userpass_plaintext(&creds.username, &creds.password)
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options
}

// ensure only one execution at any given time
pub async fn clone_or_update(
    repo_url: &str,
    branch_name: &str,
    creds: &Option<Creds>,
) -> Result<Repository> {
    let creds = creds.clone();
    let path = get_git_directory(repo_url);
    let repo_url = repo_url.to_string();
    let branch_name = branch_name.to_string();
    let rep = tokio::task::spawn_blocking(move || -> anyhow::Result<Repository> {
        let rep: Repository = if path.exists() {
            println!("Repository exists. Fetching updates...");
            let repo = Repository::open(&path)?;
            let mut remote = repo.find_remote("origin")?;

            // Branch for fetching with or without credentials
            if let Some(c) = creds {
                println!("Fetching with credentials.");
                let mut fetch_options = create_auth_options(c);
                remote.fetch(&[branch_name], Some(&mut fetch_options), None)?;
            } else {
                println!("Fetching without credentials.");
                remote.fetch(&[branch_name], None, None)?;
            }
            drop(remote);
            repo
        } else {
            println!("Cloning repository from {}...", &repo_url);
            // Ensure the parent directory exists before cloning
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            // Use RepoBuilder to allow for custom options
            let mut builder = RepoBuilder::new();

            // Branch for cloning with or without credentials
            if let Some(c) = creds.clone() {
                println!("Cloning with credentials.");
                let fetch_options = create_auth_options(c);
                // Configure the builder with our fetch options. This moves the options.
                builder.fetch_options(fetch_options);
            } else {
                println!("Cloning without credentials.");
            }

            // Perform the clone with the configured builder
            builder.clone(&repo_url, &path)?
        };
        Ok(rep)
    }).await??;

    Ok(rep)
}
