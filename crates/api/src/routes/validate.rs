use axum::{http::StatusCode, Json};
use sched_core::{validate, ValidationError};
use serde::Serialize;
use types::Instance;

#[derive(Serialize, utoipa::ToSchema)]
pub struct ValidationReport {
    pub ok: bool,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/v1/validate",
    request_body = Instance,
    responses(
    (status = 200, description = "Validation result", body = ValidationReport)
    )
)]
pub async fn validate_handler(Json(inst): Json<Instance>) -> (StatusCode, Json<ValidationReport>) {
    match validate(&inst) {
        Ok(()) => (StatusCode::OK, Json(ValidationReport { ok: true, errors: vec![] })),
        Err(ValidationError::Msg(msg)) => {
            let errs = msg.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            (StatusCode::OK, Json(ValidationReport { ok: false, errors: errs }))
        }
    }
}
