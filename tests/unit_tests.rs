//! Unit tests for core functionality.
//!
//! Run with: `cargo test --test unit_tests`

use std::collections::HashMap;

use konf_provider::{
    loader::{Loader, MultiLoader},
    loaders::yaml::YamlLoader,
    writer::{
        json::JsonWriter,
        yaml::YamlWriter,
        toml::TomlWriter,
        env::EnvVarWriter,
        properties::PropertiesWriter,
        docker_env::DockerEnvVarWriter,
        ValueWriter,
    },
    Value,
};

// ============================================================================
// Value tests
// ============================================================================

#[test]
fn test_value_get_mapping() {
    let mut map = HashMap::new();
    map.insert("key".to_string(), Value::String("value".to_string()));
    let value = Value::Mapping(map);

    assert!(value.get("key").is_some());
    assert_eq!(value.get("key").unwrap().as_str(), Some(&"value".to_string()));
    assert!(value.get("nonexistent").is_none());
}

#[test]
fn test_value_get_non_mapping() {
    let value = Value::String("test".to_string());
    assert!(value.get("key").is_none());
}

#[test]
fn test_value_as_sequence() {
    let seq = vec![Value::String("a".to_string()), Value::String("b".to_string())];
    let value = Value::Sequence(seq);

    let result = value.as_sequence();
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 2);
}

#[test]
fn test_value_as_str() {
    let value = Value::String("test".to_string());
    assert_eq!(value.as_str(), Some(&"test".to_string()));

    let value = Value::Number(42.0);
    assert_eq!(value.as_str(), None);
}

// ============================================================================
// Loader tests
// ============================================================================

#[test]
fn test_yaml_loader_simple() {
    let loader = YamlLoader {};
    assert_eq!(loader.ext(), "yaml");

    let yaml = r#"
key: value
number: 42
boolean: true
"#;

    let result = loader.load(yaml);
    assert!(result.is_ok());

    let value = result.unwrap();
    assert!(value.get("key").is_some());
    assert_eq!(value.get("key").unwrap().as_str(), Some(&"value".to_string()));
}

#[test]
fn test_yaml_loader_nested() {
    let loader = YamlLoader {};

    let yaml = r#"
parent:
  child:
    nested: value
"#;

    let result = loader.load(yaml);
    assert!(result.is_ok());

    let value = result.unwrap();
    let parent = value.get("parent").unwrap();
    let child = parent.get("child").unwrap();
    assert_eq!(child.get("nested").unwrap().as_str(), Some(&"value".to_string()));
}

#[test]
fn test_yaml_loader_array() {
    let loader = YamlLoader {};

    let yaml = r#"
items:
  - first
  - second
  - third
"#;

    let result = loader.load(yaml);
    assert!(result.is_ok());

    let value = result.unwrap();
    let items = value.get("items").unwrap().as_sequence().unwrap();
    assert_eq!(items.len(), 3);
}

#[test]
fn test_yaml_loader_invalid() {
    let loader = YamlLoader {};

    let invalid_yaml = "{{invalid: yaml";
    let result = loader.load(invalid_yaml);
    assert!(result.is_err());
}

#[test]
fn test_multi_loader() {
    let loader = MultiLoader::new(vec![Box::new(YamlLoader {})]);

    // Should work with yaml extension
    let result = loader.load("yaml", "key: value");
    assert!(result.is_ok());

    // Should fail with unknown extension
    let result = loader.load("unknown", "key: value");
    assert!(result.is_err());
}

// ============================================================================
// Writer tests
// ============================================================================

fn sample_value() -> Value {
    let mut inner = HashMap::new();
    inner.insert("nested".to_string(), Value::String("value".to_string()));

    let mut map = HashMap::new();
    map.insert("string".to_string(), Value::String("hello".to_string()));
    map.insert("number".to_string(), Value::Number(42.0));
    map.insert("boolean".to_string(), Value::Boolean(true));
    map.insert("null".to_string(), Value::Null);
    map.insert("object".to_string(), Value::Mapping(inner));
    map.insert("array".to_string(), Value::Sequence(vec![
        Value::String("a".to_string()),
        Value::String("b".to_string()),
    ]));

    Value::Mapping(map)
}

#[test]
fn test_json_writer() {
    let writer = JsonWriter {};
    assert_eq!(writer.ext(), "json");

    let value = sample_value();
    let result = writer.to_str(&value);
    assert!(result.is_ok());

    let json_str = result.unwrap();
    assert!(json_str.contains("\"string\":\"hello\"") || json_str.contains("\"string\": \"hello\""));
}

#[test]
fn test_yaml_writer() {
    let writer = YamlWriter {};
    assert_eq!(writer.ext(), "yaml");

    let value = sample_value();
    let result = writer.to_str(&value);
    assert!(result.is_ok());

    let yaml_str = result.unwrap();
    assert!(yaml_str.contains("string: hello") || yaml_str.contains("string: 'hello'"));
}

#[test]
fn test_toml_writer() {
    let writer = TomlWriter {};
    assert_eq!(writer.ext(), "toml");

    let value = sample_value();
    let result = writer.to_str(&value);
    assert!(result.is_ok());
}

#[test]
fn test_env_writer() {
    let writer = EnvVarWriter {};
    assert_eq!(writer.ext(), "env");

    let mut map = HashMap::new();
    map.insert("database_url".to_string(), Value::String("postgres://localhost".to_string()));
    map.insert("port".to_string(), Value::Number(8080.0));

    let value = Value::Mapping(map);
    let result = writer.to_str(&value);
    assert!(result.is_ok());

    let env_str = result.unwrap();
    assert!(env_str.contains("DATABASE_URL=\"postgres://localhost\""));
    assert!(env_str.contains("PORT=8080"));
}

#[test]
fn test_docker_env_writer() {
    let writer = DockerEnvVarWriter {};
    assert_eq!(writer.ext(), "docker-env");

    let mut map = HashMap::new();
    map.insert("key".to_string(), Value::String("value".to_string()));

    let value = Value::Mapping(map);
    let result = writer.to_str(&value);
    assert!(result.is_ok());

    let env_str = result.unwrap();
    // Docker env format doesn't quote string values
    assert!(env_str.contains("KEY=value"));
}

#[test]
fn test_properties_writer() {
    let writer = PropertiesWriter {};
    assert_eq!(writer.ext(), "properties");

    let mut map = HashMap::new();
    map.insert("app.name".to_string(), Value::String("myapp".to_string()));
    map.insert("app.version".to_string(), Value::Number(1.0));

    let value = Value::Mapping(map);
    let result = writer.to_str(&value);
    assert!(result.is_ok());
}

// ============================================================================
// Round-trip tests (load -> write -> load)
// ============================================================================

#[test]
fn test_yaml_roundtrip() {
    let loader = YamlLoader {};
    let writer = YamlWriter {};

    let original = r#"
key: value
nested:
  inner: data
"#;

    // Load
    let value = loader.load(original).unwrap();

    // Write
    let written = writer.to_str(&value).unwrap();

    // Load again
    let reloaded = loader.load(&written).unwrap();

    // Compare
    assert_eq!(
        value.get("key").unwrap().as_str(),
        reloaded.get("key").unwrap().as_str()
    );
}
