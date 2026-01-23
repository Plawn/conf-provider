use std::{convert::Infallible, error, fmt};

use xitca_web::{
    WebContext,
    error::{Error, MatchError},
    handler::{Responder, html::Html},
    http::{StatusCode, WebResponse},
    service::Service,
};

use crate::Value;

// a custom error type. must implement following traits:
// std::fmt::{Debug, Display} for formatting
// std::error::Error for backtrace and type casting
// From for converting from Self to xitca_web::error::Error type.
// xitca_web::service::Service for lazily generating http response.
pub struct MyError(pub anyhow::Error);

impl fmt::Debug for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MyError").finish()
    }
}

impl fmt::Display for MyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

impl error::Error for MyError {}

// Error<C> is the main error type xitca-web uses and at some point MyError would
// need to be converted to it.
impl From<MyError> for Error {
    fn from(e: MyError) -> Self {
        Error::from_service(e)
    }
}

// response generator of MyError. in this case we generate blank bad request error.
impl<'r, C> Service<WebContext<'r, C>> for MyError {
    type Response = WebResponse;
    type Error = Infallible;

    async fn call(&self, ctx: WebContext<'r, C>) -> Result<Self::Response, Self::Error> {
        StatusCode::INTERNAL_SERVER_ERROR.call(ctx).await
    }
}

// a middleware function used for intercept and interact with app handler outputs.
pub async fn error_handler<S, C>(s: &S, mut ctx: WebContext<'_, C>) -> Result<WebResponse, Error>
where
    S: for<'r> Service<WebContext<'r, C>, Response = WebResponse, Error = Error>,
{
    match s.call(ctx.reborrow()).await {
        Ok(res) => Ok(res),
        Err(e) => {
            // debug format error info.
            tracing::debug!("{e:?}");

            // generate http response actively. from here it's OK to early return it in Result::Ok
            // variant as error_handler function's output
            // let _res = e.call(ctx.reborrow()).await?;
            // return Ok(_res);

            // upcast trait and downcast to concrete type again.
            // this offers the ability to regain typed error specific error handling.
            // *. this is a runtime feature and not reinforced at compile time.
            if let Some(_e) = e.upcast().downcast_ref::<MyError>() {
                // handle typed error.
            }

            // type casting can also be used to handle xitca-web's "internal" error types for overriding
            // default error behavior.
            // *. "internal" means these error types have their default error formatter and http response generator.
            // *. "internal" error types are public types exported through `xitca_web::error` module. it's OK to
            // override them for custom formatting/http response generating.
            if e.upcast().downcast_ref::<MatchError>().is_some() {
                // MatchError is error type for request not matching any route from application service.
                // in this case we override it's default behavior by generating a different http response.
                return (Html("<h1>404 Not Found -> yo </h1>"), StatusCode::NOT_FOUND)
                    .respond(ctx)
                    .await;
            }

            // the most basic error handling is to ignore it and return as is. xitca-web is able to take care
            // of error by utilizing it's according trait implements(Debug,Display,Error and Service impls)
            tracing::error!("{e}");
            Err(e)
        }
    }
}

#[derive(Debug)]
pub enum GetError {
    /// The requested commit hash was not found in the repository
    CommitNotFound { commit: String },
    /// The requested config file was not found
    ConfigNotFound { path: String },
    /// Failed to render the configuration (e.g., missing imports, circular deps)
    RenderError { path: String, reason: String },
    /// Failed to initialize the DAG for a commit
    DagInitError { commit: String, reason: String },
    /// Unknown/internal error
    InternalError { reason: String },
    /// Invalid request (e.g., invalid commit hash format, unknown output format)
    BadRequest { reason: String },
    /// Missing or invalid authentication token
    Unauthorized { reason: String },
    /// Token is valid but not authorized for this resource
    Forbidden { path: String },
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetError::CommitNotFound { commit } => {
                write!(f, "commit not found: '{commit}'")
            }
            GetError::ConfigNotFound { path } => {
                write!(f, "config file not found: '{path}'")
            }
            GetError::RenderError { path, reason } => {
                write!(f, "failed to render config '{path}': {reason}")
            }
            GetError::DagInitError { commit, reason } => {
                write!(f, "failed to initialize config for commit '{commit}': {reason}")
            }
            GetError::InternalError { reason } => {
                write!(f, "internal error: {reason}")
            }
            GetError::BadRequest { reason } => {
                write!(f, "bad request: {reason}")
            }
            GetError::Unauthorized { reason } => {
                write!(f, "unauthorized: {reason}")
            }
            GetError::Forbidden { path } => {
                write!(f, "forbidden: not authorized to access '{path}'")
            }
        }
    }
}

impl error::Error for GetError {}

// Error<C> is the main error type xitca-web uses and at some point MyError would
// need to be converted to it.
impl From<GetError> for Error {
    fn from(e: GetError) -> Self {
        Error::from_service(e)
    }
}

// response generator of GetError. Returns appropriate HTTP status codes with error message body.
impl<'r, C> Service<WebContext<'r, C>> for GetError {
    type Response = WebResponse;
    type Error = Infallible;

    async fn call(&self, ctx: WebContext<'r, C>) -> Result<Self::Response, Self::Error> {
        let status = match self {
            GetError::CommitNotFound { .. } => StatusCode::NOT_FOUND,
            GetError::ConfigNotFound { .. } => StatusCode::NOT_FOUND,
            GetError::RenderError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            GetError::DagInitError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            GetError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            GetError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            GetError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            GetError::Forbidden { .. } => StatusCode::FORBIDDEN,
        };
        // Include the error message in the response body
        (self.to_string(), status)
            .respond(ctx)
            .await
            .map_err(|_| unreachable!())
    }
}


pub fn get_conf_strings(value: &Value, key: &str) -> Vec<String> {
    const MAIN_KEY: &str = "<!>";
    value
        .get(MAIN_KEY)
        .and_then(|main_value| main_value.as_mapping())
        .and_then(|main_map| main_map.get(key))
        .and_then(|import_value| import_value.as_sequence())
        .map(|import_sequence| {
            import_sequence
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}