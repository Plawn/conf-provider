//! End-to-end tests for local server mode with nested configurations.
//!
//! These tests spawn the actual server and make HTTP requests to verify
//! that nested folder paths work correctly through the API.

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Find an available port for testing
fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to address")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Get the path to the example folder
fn example_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example")
}

/// Spawn the server process
fn spawn_server(port: u16) -> Child {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    Command::new("cargo")
        .args([
            "run",
            "--bin",
            "server",
            "--",
            "local",
            "--folder",
            example_folder().to_str().unwrap(),
            "--port",
            &port.to_string(),
        ])
        .current_dir(&manifest_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("Failed to spawn server")
}

/// Wait for the server to be ready
async fn wait_for_server(port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    let client = reqwest::Client::new();

    while start.elapsed() < timeout {
        if client
            .get(format!("http://127.0.0.1:{}/live", port))
            .send()
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

struct TestServer {
    port: u16,
    process: Child,
}

impl TestServer {
    async fn new() -> Self {
        let port = find_available_port();
        let process = spawn_server(port);

        // Wait for server to be ready
        if !wait_for_server(port, Duration::from_secs(30)).await {
            panic!("Server failed to start within timeout");
        }

        Self { port, process }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

#[tokio::test]
async fn test_server_health_check() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/live"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "OK");
}

#[tokio::test]
async fn test_server_metrics_endpoint() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/metrics"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success());
    let body = response.text().await.unwrap();

    // Metrics should appear immediately at startup (pre-initialized)
    assert!(
        body.contains("# HELP") && body.contains("# TYPE"),
        "Should return prometheus metrics format with descriptions"
    );
    assert!(
        body.contains("config_reloads_total"),
        "Should contain config_reloads_total metric"
    );
    assert!(
        body.contains("git_cache_lookups_total"),
        "Should contain git_cache_lookups_total metric"
    );
}

#[tokio::test]
async fn test_server_flat_config() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Test flat config at root level
    let response = client
        .get(server.url("/data/json/a"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Should find flat config 'a'"
    );

    let body = response.text().await.unwrap();
    assert!(body.contains("dzedez"), "Should contain value from a.yaml");
}

#[tokio::test]
async fn test_server_nested_config_common_database() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Test nested config: common/database
    let response = client
        .get(server.url("/data/json/common/database"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Should find nested config 'common/database'"
    );

    let body: serde_json::Value = response.json().await.expect("Should be valid JSON");
    assert_eq!(body["host"], "localhost");
    // Numbers are stored as f64 internally
    assert_eq!(body["port"].as_f64().unwrap() as i64, 5432);
    assert_eq!(body["user"], "app_user");
}

#[tokio::test]
async fn test_server_nested_config_common_redis() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Test nested config: common/redis
    let response = client
        .get(server.url("/data/yaml/common/redis"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Should find nested config 'common/redis'"
    );

    let body = response.text().await.unwrap();
    assert!(body.contains("host: localhost"), "Should contain redis host");
    assert!(body.contains("port: 6379"), "Should contain redis port");
}

#[tokio::test]
async fn test_server_deeply_nested_config_api() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Test deeply nested config: services/api/config
    let response = client
        .get(server.url("/data/json/services/api/config"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Should find deeply nested config 'services/api/config'"
    );

    let body: serde_json::Value = response.json().await.expect("Should be valid JSON");

    // Check service section
    assert_eq!(body["service"]["name"], "api-service");
    // Numbers are stored as f64 internally
    assert_eq!(body["service"]["port"].as_f64().unwrap() as i64, 8080);

    // Check that imports were resolved
    let db_url = body["database"]["url"].as_str().unwrap();
    assert!(
        db_url.contains("app_user"),
        "Database URL should have resolved user: {}",
        db_url
    );
    assert!(
        db_url.contains("5432"),
        "Database URL should have resolved port: {}",
        db_url
    );
    assert!(
        db_url.contains("localhost"),
        "Database URL should have resolved host: {}",
        db_url
    );
}

#[tokio::test]
async fn test_server_deeply_nested_config_worker() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Test deeply nested config: services/worker/config
    let response = client
        .get(server.url("/data/json/services/worker/config"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        response.status().is_success(),
        "Should find deeply nested config 'services/worker/config'"
    );

    let body: serde_json::Value = response.json().await.expect("Should be valid JSON");

    // Check service section
    assert_eq!(body["service"]["name"], "worker-service");
    // Numbers are stored as f64 internally
    assert_eq!(body["service"]["concurrency"].as_f64().unwrap() as i64, 4);

    // Check queue section with resolved redis URL
    let redis_url = body["queue"]["redis_url"].as_str().unwrap();
    assert!(
        redis_url.contains("localhost"),
        "Redis URL should have resolved host: {}",
        redis_url
    );
    assert!(
        redis_url.contains("6379"),
        "Redis URL should have resolved port: {}",
        redis_url
    );
}

#[tokio::test]
async fn test_server_multiple_output_formats() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    // Note: docker-env uses hyphen, not underscore
    let formats = ["json", "yaml", "toml", "env", "properties", "docker-env"];

    for format in formats {
        let response = client
            .get(server.url(&format!("/data/{}/common/database", format)))
            .send()
            .await
            .expect("Failed to send request");

        assert!(
            response.status().is_success(),
            "Should return {} format for nested config",
            format
        );
    }
}

#[tokio::test]
async fn test_server_not_found() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/data/json/nonexistent/path"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(
        !response.status().is_success(),
        "Should return error for nonexistent path"
    );
}

#[tokio::test]
async fn test_server_reload() {
    let server = TestServer::new().await;
    let client = reqwest::Client::new();

    let response = client
        .get(server.url("/reload"))
        .send()
        .await
        .expect("Failed to send request");

    assert!(response.status().is_success(), "Reload should succeed");
    assert_eq!(response.text().await.unwrap(), "OK");
}
