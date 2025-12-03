//! End-to-end tests for git functionality.
//!
//! These tests require valid credentials in `prod.env`:
//! - REPO_URL: Git repository URL
//! - BRANCH: Branch name
//! - USERNAME: Git username
//! - PASSWORD: Git password/token
//!
//! Run with: `cargo test --test e2e_git`
//!
//! Tests that require valid credentials are marked with `#[ignore]` by default.
//! To run them: `cargo test --test e2e_git -- --ignored`

use std::sync::Arc;

use konf_provider::{
    fs::git::{clone_or_update, get_git_directory, is_valid_commit_hash, is_valid_git_url, list_all_commit_hashes, Creds, GitFileProvider},
    loader::MultiLoader,
    loaders::yaml::YamlLoader,
    render::Dag,
};

struct TestEnv {
    repo_url: String,
    branch: String,
    username: String,
    password: String,
}

impl TestEnv {
    fn load() -> Option<Self> {
        // Load from prod.env file
        dotenvy::from_filename("prod.env").ok()?;

        Some(Self {
            repo_url: std::env::var("REPO_URL").ok()?,
            branch: std::env::var("BRANCH").ok()?,
            username: std::env::var("USERNAME").ok()?,
            password: std::env::var("PASSWORD").ok()?,
        })
    }

    fn creds(&self) -> Option<Creds> {
        Some(Creds::new(self.username.clone(), self.password.clone()))
    }
}

// ============================================================================
// Unit tests (no credentials required)
// ============================================================================

#[test]
fn test_commit_hash_validation() {
    // Valid full hashes (40 chars)
    assert!(is_valid_commit_hash("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"));
    assert!(is_valid_commit_hash("0123456789abcdef0123456789abcdef01234567"));

    // Valid short hashes (7+ chars)
    assert!(is_valid_commit_hash("a1b2c3d"));
    assert!(is_valid_commit_hash("0123456789"));

    // Invalid: too short
    assert!(!is_valid_commit_hash("a1b2c3"));
    assert!(!is_valid_commit_hash("abc"));

    // Invalid: too long
    assert!(!is_valid_commit_hash("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c"));

    // Invalid: non-hex characters
    assert!(!is_valid_commit_hash("g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"));
    assert!(!is_valid_commit_hash("hello-world"));

    // Invalid: empty
    assert!(!is_valid_commit_hash(""));
}

#[test]
fn test_git_url_validation() {
    // Valid HTTPS URLs
    assert!(is_valid_git_url("https://github.com/user/repo.git"));
    assert!(is_valid_git_url("https://gitlab.com/user/repo"));
    assert!(is_valid_git_url("https://git.example.com/path/to/repo.git"));

    // Valid HTTP URLs
    assert!(is_valid_git_url("http://github.com/user/repo.git"));

    // Valid git:// URLs
    assert!(is_valid_git_url("git://github.com/user/repo.git"));

    // Valid SSH URLs
    assert!(is_valid_git_url("ssh://git@github.com/user/repo.git"));
    assert!(is_valid_git_url("git@github.com:user/repo.git"));
    assert!(is_valid_git_url("git@gitlab.example.com:group/project.git"));

    // Invalid URLs
    assert!(!is_valid_git_url("ftp://example.com/repo.git"));
    assert!(!is_valid_git_url("file:///path/to/repo"));
    assert!(!is_valid_git_url("/local/path/to/repo"));
    assert!(!is_valid_git_url(""));
    assert!(!is_valid_git_url("not a url"));
}

#[test]
fn test_git_directory_is_deterministic() {
    let url = "https://github.com/example/repo.git";
    let dir1 = get_git_directory(url);
    let dir2 = get_git_directory(url);
    assert_eq!(dir1, dir2, "Same URL should produce same directory");

    let different_url = "https://github.com/example/other-repo.git";
    let dir3 = get_git_directory(different_url);
    assert_ne!(dir1, dir3, "Different URLs should produce different directories");
}

// ============================================================================
// E2E tests (require valid credentials - run with --ignored)
// ============================================================================

#[tokio::test]
#[ignore = "Requires valid credentials in prod.env"]
async fn test_clone_repository() {
    let env = TestEnv::load().expect("Failed to load test environment from prod.env");

    // Clean up any existing clone
    let git_dir = get_git_directory(&env.repo_url);
    if git_dir.exists() {
        std::fs::remove_dir_all(&git_dir).ok();
    }

    // Clone the repository
    let result = clone_or_update(&env.repo_url, &env.branch, &env.creds()).await;
    assert!(result.is_ok(), "Failed to clone repository: {:?}", result.err());

    // Verify the directory was created
    assert!(git_dir.exists(), "Git directory should exist after clone");
}

#[tokio::test]
#[ignore = "Requires valid credentials in prod.env"]
async fn test_fetch_updates() {
    let env = TestEnv::load().expect("Failed to load test environment from prod.env");

    // Ensure repo exists first
    let result = clone_or_update(&env.repo_url, &env.branch, &env.creds()).await;
    assert!(result.is_ok(), "Failed to clone/update repository: {:?}", result.err());

    // Fetch again (should succeed as an update)
    let result = clone_or_update(&env.repo_url, &env.branch, &env.creds()).await;
    assert!(result.is_ok(), "Failed to fetch updates: {:?}", result.err());
}

#[tokio::test]
#[ignore = "Requires valid credentials in prod.env"]
async fn test_list_commits() {
    let env = TestEnv::load().expect("Failed to load test environment from prod.env");

    // Ensure repo exists
    clone_or_update(&env.repo_url, &env.branch, &env.creds())
        .await
        .expect("Failed to clone repository");

    // List commits
    let commits = list_all_commit_hashes(&env.repo_url);
    assert!(commits.is_ok(), "Failed to list commits: {:?}", commits.err());

    let commits = commits.unwrap();
    assert!(!commits.is_empty(), "Repository should have at least one commit");

    // Verify all commit hashes are valid format
    for commit in &commits {
        assert!(
            is_valid_commit_hash(commit),
            "Invalid commit hash format: {}",
            commit
        );
    }
}

#[tokio::test]
#[ignore = "Requires valid credentials in prod.env"]
async fn test_git_file_provider() {
    let env = TestEnv::load().expect("Failed to load test environment from prod.env");

    // Ensure repo exists
    clone_or_update(&env.repo_url, &env.branch, &env.creds())
        .await
        .expect("Failed to clone repository");

    // Get a commit hash
    let commits = list_all_commit_hashes(&env.repo_url).expect("Failed to list commits");
    let commit = commits.iter().next().expect("No commits found");

    // Create file provider
    let provider = GitFileProvider::new(&env.repo_url, commit).await;
    assert!(provider.is_ok(), "Failed to create GitFileProvider: {:?}", provider.err());
}

#[tokio::test]
#[ignore = "Requires valid credentials in prod.env"]
async fn test_dag_from_git() {
    let env = TestEnv::load().expect("Failed to load test environment from prod.env");

    // Ensure repo exists
    clone_or_update(&env.repo_url, &env.branch, &env.creds())
        .await
        .expect("Failed to clone repository");

    // Get a commit hash
    let commits = list_all_commit_hashes(&env.repo_url).expect("Failed to list commits");
    let commit = commits.iter().next().expect("No commits found");

    // Create file provider and DAG
    let provider = GitFileProvider::new(&env.repo_url, commit)
        .await
        .expect("Failed to create GitFileProvider");

    let multiloader = Arc::from(MultiLoader::new(vec![Box::new(YamlLoader {})]));
    let dag = Dag::new(provider, multiloader).await;

    assert!(dag.is_ok(), "Failed to create DAG: {:?}", dag.err());
}
