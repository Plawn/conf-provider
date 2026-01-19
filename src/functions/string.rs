//! String manipulation functions.

use crate::Value;

use super::{value_type_name, FunctionArg, FunctionError, TemplateFunction};

/// Trims whitespace from both ends of a string.
pub struct Trim;

impl TemplateFunction for Trim {
    fn name(&self) -> &'static str {
        "trim"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => Ok(Value::String(s.trim().to_string())),
            other => Err(FunctionError::UnsupportedType {
                function: self.name().to_string(),
                got: value_type_name(&other),
            }),
        }
    }
}

/// Converts a string to uppercase.
pub struct Upper;

impl TemplateFunction for Upper {
    fn name(&self) -> &'static str {
        "upper"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => Ok(Value::String(s.to_uppercase())),
            other => Err(FunctionError::UnsupportedType {
                function: self.name().to_string(),
                got: value_type_name(&other),
            }),
        }
    }
}

/// Converts a string to lowercase.
pub struct Lower;

impl TemplateFunction for Lower {
    fn name(&self) -> &'static str {
        "lower"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => Ok(Value::String(s.to_lowercase())),
            other => Err(FunctionError::UnsupportedType {
                function: self.name().to_string(),
                got: value_type_name(&other),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trim() {
        let func = Trim;
        assert_eq!(func.name(), "trim");

        // Normal case
        let result = func.execute(Value::String("  hello  ".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));

        // Already trimmed
        let result = func.execute(Value::String("hello".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));

        // Empty string
        let result = func.execute(Value::String("   ".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("".to_string()));

        // Unsupported type
        let result = func.execute(Value::Int(42), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_upper() {
        let func = Upper;
        assert_eq!(func.name(), "upper");

        let result = func.execute(Value::String("hello".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("HELLO".to_string()));

        let result = func.execute(Value::String("Hello World".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("HELLO WORLD".to_string()));

        // Unsupported type
        let result = func.execute(Value::Boolean(true), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_lower() {
        let func = Lower;
        assert_eq!(func.name(), "lower");

        let result = func.execute(Value::String("HELLO".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));

        let result = func.execute(Value::String("Hello World".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello world".to_string()));

        // Unsupported type
        let result = func.execute(Value::Null, &[]);
        assert!(result.is_err());
    }
}
