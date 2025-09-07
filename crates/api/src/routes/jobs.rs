use axum::{extract::{Path, State}, Json};
use crate::state::AppState;
use utoipa;
use types::SolveResult;

#[utoipa::path(
        get,
        path = "/v1/jobs/{id}",
        params(("id" = String, Path, description = "Job ID")),
        responses((status = 200, description = "Job status", body = jobs::JobStatus))
    )]
pub async fn status(State(state): State<AppState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let st = state.jobs.get(&id);
    Json(match st {
        None => serde_json::json!({"status": "not_found"}),
        Some(s) => serde_json::to_value(s).unwrap(),
    })
}

#[utoipa::path(
        get,
        path = "/v1/jobs/{id}/result",
        params(("id" = String, Path, description = "Job ID")),
        responses(
            (status = 200, description = "Solve result (if ready)", body = SolveResult)
        )
    )]
pub async fn result(State(state): State<AppState>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let st = state.jobs.get(&id);
    Json(match st {
        Some(jobs::JobStatus::Solved { result }) => serde_json::to_value(result).unwrap(),
        Some(_) => serde_json::json!({"status": "not_ready"}),
        None => serde_json::json!({"status": "not_found"}),
    })
}
