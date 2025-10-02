use crate::state::AppState;
use axum::{extract::State, Json};
use serde::Deserialize;
use types::SolveEnvelope;
use utoipa::ToSchema;

#[derive(Deserialize)]
pub struct SolveIn {
    #[serde(flatten)]
    pub env: SolveEnvelope,
}

#[derive(serde::Serialize, ToSchema)]
pub struct JobCreated {
    pub jobId: String,
    pub status: &'static str,
}

#[utoipa::path(
        post,
        path = "/v1/solve",
        request_body = SolveEnvelope,
        responses((status = 200, description = "Job enqueued", body = JobCreated))
    )]
pub async fn solve(
    State(state): State<AppState>,
    Json(env): Json<SolveEnvelope>,
) -> Json<JobCreated> {
    let id = state.jobs.enqueue(env);
    Json(JobCreated {
        jobId: id.0,
        status: "queued",
    })
}

#[utoipa::path(
    post,
    path = "/v1/reoptimize",
    request_body = SolveEnvelope,
    responses((status = 200, description = "Reoptimize job enqueued", body = JobCreated))
)]
pub async fn reoptimize(
    State(state): State<AppState>,
    Json(env): Json<SolveEnvelope>,
) -> Json<JobCreated> {
    let id = state.jobs.enqueue(env);
    Json(JobCreated {
        jobId: id.0,
        status: "queued",
    })
}
