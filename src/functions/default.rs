//! Default value function.

use crate::Value;

use super::{FunctionArg, FunctionError, TemplateFunction};

/// Returns a default value if the input is null.
pub struct Default;

impl TemplateFunction for Default {
    fn name(&self) -> &'static str {
        "default"
    }

    fn execute(&self, value: Value, args: &[FunctionArg]) -> Result<Value, FunctionError> {
        // If value is not null, return it as-is
        if !matches!(value, Value::Null) {
            return Ok(value);
        }

        // Value is null, use the default argument
        match args.first() {
            Some(FunctionArg::String(s)) => Ok(Value::String(s.clone())),
            Some(FunctionArg::Int(n)) => Ok(Value::Int(*n)),
            Some(FunctionArg::Float(f)) => Ok(Value::Float(*f)),
            Some(FunctionArg::Boolean(b)) => Ok(Value::Boolean(*b)),
            None => Err(FunctionError::InvalidArgument {
                function: self.name().to_string(),
                expected: "a default value argument",
                got: "no argument".to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_with_null() {
        let func = Default;
        assert_eq!(func.name(), "default");

        // Null with string default
        let result = func.execute(Value::Null, &[FunctionArg::String("fallback".to_string())]);
        assert_eq!(result.unwrap(), Value::String("fallback".to_string()));

        // Null with int default
        let result = func.execute(Value::Null, &[FunctionArg::Int(42)]);
        assert_eq!(result.unwrap(), Value::Int(42));

        // Null with float default
        let result = func.execute(Value::Null, &[FunctionArg::Float(3.14)]);
        assert_eq!(result.unwrap(), Value::Float(3.14));

        // Null with boolean default
        let result = func.execute(Value::Null, &[FunctionArg::Boolean(true)]);
        assert_eq!(result.unwrap(), Value::Boolean(true));
    }

    #[test]
    fn test_default_with_non_null() {
        let func = Default;

        // String value - default not applied
        let result = func.execute(
            Value::String("existing".to_string()),
            &[FunctionArg::String("fallback".to_string())],
        );
        assert_eq!(result.unwrap(), Value::String("existing".to_string()));

        // Int value - default not applied
        let result = func.execute(Value::Int(10), &[FunctionArg::Int(42)]);
        assert_eq!(result.unwrap(), Value::Int(10));

        // Boolean value - default not applied
        let result = func.execute(Value::Boolean(false), &[FunctionArg::Boolean(true)]);
        assert_eq!(result.unwrap(), Value::Boolean(false));
    }

    #[test]
    fn test_default_missing_argument() {
        let func = Default;

        let result = func.execute(Value::Null, &[]);
        assert!(result.is_err());
    }
}
