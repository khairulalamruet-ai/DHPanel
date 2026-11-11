//! Axum extractors, middleware, problem-details errors and OpenAPI helpers.

pub mod client_ip;
pub mod error;
pub mod health;
pub mod metrics;
pub mod request_id;

pub use error::{AppError, FieldError, ProblemDetails};
