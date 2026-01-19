//! Encoding functions for base64 and URL encoding.

use base64::{engine::general_purpose::STANDARD, Engine};

use crate::Value;

use super::{value_type_name, FunctionArg, FunctionError, TemplateFunction};

/// Encodes a string to base64.
pub struct Base64Encode;

impl TemplateFunction for Base64Encode {
    fn name(&self) -> &'static str {
        "base64"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => Ok(Value::String(STANDARD.encode(s.as_bytes()))),
            other => Err(FunctionError::UnsupportedType {
                function: self.name().to_string(),
                got: value_type_name(&other),
            }),
        }
    }
}

/// Decodes a base64 string.
pub struct Base64Decode;

impl TemplateFunction for Base64Decode {
    fn name(&self) -> &'static str {
        "base64_decode"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => {
                let decoded = STANDARD.decode(s.as_bytes()).map_err(|e| {
                    FunctionError::ExecutionError {
                        function: self.name().to_string(),
                        message: e.to_string(),
                    }
                })?;
                let decoded_str = String::from_utf8(decoded).map_err(|e| {
                    FunctionError::ExecutionError {
                        function: self.name().to_string(),
                        message: e.to_string(),
                    }
                })?;
                Ok(Value::String(decoded_str))
            }
            other => Err(FunctionError::UnsupportedType {
                function: self.name().to_string(),
                got: value_type_name(&other),
            }),
        }
    }
}

/// URL-encodes a string (percent encoding).
pub struct UrlEscape;

impl TemplateFunction for UrlEscape {
    fn name(&self) -> &'static str {
        "url_escape"
    }

    fn execute(&self, value: Value, _args: &[FunctionArg]) -> Result<Value, FunctionError> {
        match value {
            Value::String(s) => Ok(Value::String(urlencoding::encode(&s).into_owned())),
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
    fn test_base64_encode() {
        let func = Base64Encode;
        assert_eq!(func.name(), "base64");

        let result = func.execute(Value::String("hello".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("aGVsbG8=".to_string()));

        let result = func.execute(Value::String("hello world".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("aGVsbG8gd29ybGQ=".to_string()));

        // Unsupported type
        let result = func.execute(Value::Int(42), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_base64_decode() {
        let func = Base64Decode;
        assert_eq!(func.name(), "base64_decode");

        let result = func.execute(Value::String("aGVsbG8=".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));

        let result = func.execute(Value::String("aGVsbG8gd29ybGQ=".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello world".to_string()));

        // Invalid base64
        let result = func.execute(Value::String("not valid base64!!!".to_string()), &[]);
        assert!(result.is_err());

        // Unsupported type
        let result = func.execute(Value::Boolean(true), &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_url_escape() {
        let func = UrlEscape;
        assert_eq!(func.name(), "url_escape");

        let result = func.execute(Value::String("hello world".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello%20world".to_string()));

        let result = func.execute(Value::String("a=b&c=d".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("a%3Db%26c%3Dd".to_string()));

        // No encoding needed
        let result = func.execute(Value::String("hello".to_string()), &[]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));

        // Unsupported type
        let result = func.execute(Value::Null, &[]);
        assert!(result.is_err());
    }
}
