use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(Debug)]
pub enum Error {}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("Internal server error: {:?}", self);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong".to_string(),
        )
            .into_response()
    }
}
