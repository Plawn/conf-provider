//! Template function system for transforming values in placeholders.
//!
//! This module provides a registry of functions that can be applied to values
//! using pipe syntax: `${path.to.value | trim | upper}`

pub mod default;
pub mod encoding;
pub mod string;

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::Value;

/// Arguments that can be passed to template functions.
#[derive(Debug, Clone)]
pub enum FunctionArg {
    String(String),
    Int(i64),
    Float(f64),
    Boolean(bool),
}

/// Errors that can occur when executing template functions.
#[derive(Debug, thiserror::Error)]
pub enum FunctionError {
    #[error("unknown function: {0}")]
    UnknownFunction(String),

    #[error("function '{function}' expected {expected}, got {got}")]
    InvalidArgument {
        function: String,
        expected: &'static str,
        got: String,
    },

    #[error("function '{function}' does not support type {got}")]
    UnsupportedType { function: String, got: &'static str },

    #[error("function '{function}' execution error: {message}")]
    ExecutionError { function: String, message: String },
}

/// Trait for implementing template functions.
pub trait TemplateFunction: Send + Sync {
    /// Returns the name of the function as used in templates.
    fn name(&self) -> &'static str;

    /// Executes the function on the given value with optional arguments.
    fn execute(&self, value: Value, args: &[FunctionArg]) -> Result<Value, FunctionError>;
}

/// Registry holding all available template functions.
pub struct FunctionRegistry {
    functions: HashMap<&'static str, Box<dyn TemplateFunction>>,
}

impl FunctionRegistry {
    /// Creates a new registry with all built-in functions registered.
    pub fn new() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
        };

        // Register string functions
        registry.register(Box::new(string::Trim));
        registry.register(Box::new(string::Upper));
        registry.register(Box::new(string::Lower));

        // Register encoding functions
        registry.register(Box::new(encoding::Base64Encode));
        registry.register(Box::new(encoding::Base64Decode));
        registry.register(Box::new(encoding::UrlEscape));

        // Register default function
        registry.register(Box::new(default::Default));

        registry
    }

    /// Registers a function in the registry.
    pub fn register(&mut self, func: Box<dyn TemplateFunction>) {
        self.functions.insert(func.name(), func);
    }

    /// Gets a function by name.
    pub fn get(&self, name: &str) -> Option<&dyn TemplateFunction> {
        self.functions.get(name).map(|b| b.as_ref())
    }

    /// Executes a function by name on the given value.
    pub fn execute(
        &self,
        name: &str,
        value: Value,
        args: &[FunctionArg],
    ) -> Result<Value, FunctionError> {
        match self.get(name) {
            Some(func) => func.execute(value, args),
            None => Err(FunctionError::UnknownFunction(name.to_string())),
        }
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global function registry singleton.
static REGISTRY: OnceLock<FunctionRegistry> = OnceLock::new();

/// Returns a reference to the global function registry.
pub fn registry() -> &'static FunctionRegistry {
    REGISTRY.get_or_init(FunctionRegistry::new)
}

/// Helper to get the type name of a Value for error messages.
pub fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Boolean(_) => "boolean",
        Value::Null => "null",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
    }
}
