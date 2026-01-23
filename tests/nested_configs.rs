use std::path::PathBuf;
use std::sync::Arc;

use konf_provider::fs::local::BasicFsFileProvider;
use konf_provider::loader::MultiLoader;
use konf_provider::loaders::yaml::YamlLoader;
use konf_provider::render::Dag;
use konf_provider::Value;

fn example_folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("example")
}

fn create_multiloader() -> Arc<MultiLoader> {
    Arc::new(MultiLoader::new(vec![Box::new(YamlLoader {})]))
}

#[tokio::test]
async fn test_list_nested_files() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test that we can access nested configs by their relative path
    let api_config = dag.get_raw("services/api/config");
    assert!(api_config.is_ok(), "Should find services/api/config");

    let worker_config = dag.get_raw("services/worker/config");
    assert!(worker_config.is_ok(), "Should find services/worker/config");

    let database = dag.get_raw("common/database");
    assert!(database.is_ok(), "Should find common/database");

    let redis = dag.get_raw("common/redis");
    assert!(redis.is_ok(), "Should find common/redis");
}

#[tokio::test]
async fn test_nested_imports() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test that nested imports are resolved correctly
    let api_config = dag
        .get_rendered("services/api/config")
        .await
        .expect("Failed to render services/api/config");

    // Check that the database URL was resolved from the imported common/database
    if let Value::Mapping(map) = &api_config {
        let database = map.get("database").expect("Should have database section");
        if let Value::Mapping(db_map) = database {
            let url = db_map.get("url").expect("Should have url");
            if let Value::String(url_str) = url {
                assert!(
                    url_str.contains("app_user"),
                    "URL should contain resolved user: {}",
                    url_str
                );
                assert!(
                    url_str.contains("secret123"),
                    "URL should contain resolved password: {}",
                    url_str
                );
                assert!(
                    url_str.contains("localhost"),
                    "URL should contain resolved host: {}",
                    url_str
                );
                assert!(
                    url_str.contains("5432"),
                    "URL should contain resolved port: {}",
                    url_str
                );
            } else {
                panic!("URL should be a string");
            }
        } else {
            panic!("Database should be a mapping");
        }
    } else {
        panic!("Config should be a mapping");
    }
}

#[tokio::test]
async fn test_worker_config_resolution() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    let worker_config = dag
        .get_rendered("services/worker/config")
        .await
        .expect("Failed to render services/worker/config");

    if let Value::Mapping(map) = &worker_config {
        // Check service section
        let service = map.get("service").expect("Should have service section");
        if let Value::Mapping(svc_map) = service {
            let name = svc_map.get("name").expect("Should have name");
            assert!(matches!(name, Value::String(s) if s == "worker-service"));
        }

        // Check queue section with resolved redis URL
        let queue = map.get("queue").expect("Should have queue section");
        if let Value::Mapping(queue_map) = queue {
            let redis_url = queue_map.get("redis_url").expect("Should have redis_url");
            if let Value::String(url_str) = redis_url {
                assert!(
                    url_str.contains("localhost"),
                    "Redis URL should contain resolved host"
                );
                assert!(
                    url_str.contains("6379"),
                    "Redis URL should contain resolved port"
                );
            }
        }
    } else {
        panic!("Config should be a mapping");
    }
}

#[tokio::test]
async fn test_flat_configs_still_work() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test that flat configs at the root still work
    let a = dag.get_raw("a");
    assert!(a.is_ok(), "Should find flat config 'a'");

    let b = dag.get_raw("b");
    assert!(b.is_ok(), "Should find flat config 'b'");

    let c = dag.get_raw("c");
    assert!(c.is_ok(), "Should find flat config 'c'");
}

#[tokio::test]
async fn test_flat_config_with_imports() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test that b.yaml which imports 'a' still works
    let b_rendered = dag
        .get_rendered("b")
        .await
        .expect("Failed to render b");

    if let Value::Mapping(map) = &b_rendered {
        let pm = map.get("pm").expect("Should have pm");
        if let Value::String(pm_str) = pm {
            // pm should have resolved ${a.value} ${a.value}
            assert!(
                pm_str.contains("dzedez"),
                "pm should contain resolved value from a: {}",
                pm_str
            );
        }
    }
}

#[tokio::test]
async fn test_new_import_format_with_aliases() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test d.yaml which uses the new import format: { path: alias }
    let d_rendered = dag
        .get_rendered("d")
        .await
        .expect("Failed to render d");

    if let Value::Mapping(map) = &d_rendered {
        // Check that ${db.host} was resolved using the alias
        let db_host = map.get("db_host").expect("Should have db_host");
        assert!(
            matches!(db_host, Value::String(s) if s == "localhost"),
            "db_host should be resolved from alias 'db': {:?}",
            db_host
        );

        // Check that ${db.port} was resolved using the alias
        let db_port = map.get("db_port").expect("Should have db_port");
        assert!(
            matches!(db_port, Value::Int(5432)),
            "db_port should be resolved from alias 'db': {:?}",
            db_port
        );

        // Check that ${cache.host} was resolved using the alias
        let cache_host = map.get("cache_host").expect("Should have cache_host");
        assert!(
            matches!(cache_host, Value::String(s) if s == "localhost"),
            "cache_host should be resolved from alias 'cache': {:?}",
            cache_host
        );

        // Check connection string with multiple alias references
        let conn_str = map.get("connection_string").expect("Should have connection_string");
        if let Value::String(url_str) = conn_str {
            assert!(
                url_str.contains("app_user"),
                "Connection string should contain resolved user: {}",
                url_str
            );
            assert!(
                url_str.contains("localhost"),
                "Connection string should contain resolved host: {}",
                url_str
            );
            assert!(
                url_str.contains("5432"),
                "Connection string should contain resolved port: {}",
                url_str
            );
        } else {
            panic!("connection_string should be a string");
        }
    } else {
        panic!("Config d should be a mapping");
    }
}

#[tokio::test]
async fn test_new_import_format_with_relative_paths() {
    let provider = BasicFsFileProvider::new(example_folder());
    let multiloader = create_multiloader();

    let dag = Dag::new(provider, multiloader)
        .await
        .expect("Failed to create DAG");

    // Test services/api/config_v2.yaml which uses relative paths with aliases
    let config_v2 = dag
        .get_rendered("services/api/config_v2")
        .await
        .expect("Failed to render services/api/config_v2");

    if let Value::Mapping(map) = &config_v2 {
        // Check service name
        let service = map.get("service").expect("Should have service section");
        if let Value::Mapping(svc_map) = service {
            let name = svc_map.get("name").expect("Should have name");
            assert!(
                matches!(name, Value::String(s) if s == "api-service-v2"),
                "Service name should be api-service-v2"
            );
        }

        // Check database URL resolved from relative import with alias
        let database = map.get("database").expect("Should have database section");
        if let Value::Mapping(db_map) = database {
            let url = db_map.get("url").expect("Should have url");
            if let Value::String(url_str) = url {
                // The relative path ../../common/database should resolve to common/database
                assert!(
                    url_str.contains("app_user"),
                    "URL should contain resolved user from relative import: {}",
                    url_str
                );
                assert!(
                    url_str.contains("secret123"),
                    "URL should contain resolved password from relative import: {}",
                    url_str
                );
                assert!(
                    url_str.contains("localhost"),
                    "URL should contain resolved host from relative import: {}",
                    url_str
                );
                assert!(
                    url_str.contains("5432"),
                    "URL should contain resolved port from relative import: {}",
                    url_str
                );
            } else {
                panic!("Database URL should be a string");
            }
        }

        // Check cache URL resolved from relative import with alias
        let cache = map.get("cache").expect("Should have cache section");
        if let Value::Mapping(cache_map) = cache {
            let url = cache_map.get("url").expect("Should have url");
            if let Value::String(url_str) = url {
                assert!(
                    url_str.contains("localhost"),
                    "Cache URL should contain resolved host from relative import: {}",
                    url_str
                );
                assert!(
                    url_str.contains("6379"),
                    "Cache URL should contain resolved port from relative import: {}",
                    url_str
                );
            } else {
                panic!("Cache URL should be a string");
            }
        }
    } else {
        panic!("Config should be a mapping");
    }
}
