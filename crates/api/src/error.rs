use axum::{http::StatusCode, response::{IntoResponse, Response}};

#[derive(Debug)]
pub struct ApiError(pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response { (StatusCode::BAD_REQUEST, self.0).into_response() }
}
