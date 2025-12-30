use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug)]
pub enum Error {
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound => write!(f, "Not found"),
            Error::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            Error::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl<E: std::fmt::Display + std::fmt::Debug> From<sea_orm::TransactionError<E>> for Error {
    fn from(err: sea_orm::TransactionError<E>) -> Self {
        Error::Internal(format!("Transaction failed: {}", err))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::NotFound => {
                (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))).into_response()
            }
            Error::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
            }
            Error::Internal(msg) => {
                tracing::error!("Internal server error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Internal server error" })),
                )
                    .into_response()
            }
        }
    }
}
