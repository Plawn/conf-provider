use crate::loader::Value;
pub mod json;
pub mod yaml;
use std::fmt::Debug;
pub trait ValueWriter: Debug + Send + Sync {
    fn ext(&self) -> &'static str;
    fn to_str(&self, v: &Value) -> String;
}