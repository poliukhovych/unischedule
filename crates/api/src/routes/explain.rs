use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use sched_core::scoring::compute_soft_scores;
use types::{Assignment, Instance};

#[derive(Deserialize, ToSchema)]
pub struct ExplainIn {
    pub instance: Instance,
    pub assignments: Vec<Assignment>,
}

#[derive(Serialize, ToSchema)]
pub struct ExplainOut {
    pub objective: f64,
    pub weights: Weights,
    pub counts: Counts,
}

#[derive(Serialize, ToSchema)]
pub struct Weights {
    pub unpreferred_time: i32,
    pub windows: i32,
}

#[derive(Serialize, ToSchema)]
pub struct Counts {
    pub unpreferred_meetings: i64,
    pub windows_total: i64,
    pub windows_teachers: std::collections::HashMap<String, i64>,
    pub windows_groups: std::collections::HashMap<String, i64>,
}

#[utoipa::path(
    post,
    path = "/v1/explain",
    request_body = ExplainIn,
    responses(
    (status = 200, description = "Soft-penalty breakdown for provided schedule", body = ExplainOut)
    )
)]
pub async fn explain(Json(input): Json<ExplainIn>) -> Json<ExplainOut> {
    let s = compute_soft_scores(&input.instance, &input.assignments);
    let w = &input.instance.policy.soft_weights;
    Json(ExplainOut {
        objective: s.objective,
        weights: Weights {
            unpreferred_time: w.unpreferred_time,
            windows: w.windows,
        },
        counts: Counts {
            unpreferred_meetings: s.unpreferred_meetings,
            windows_total: s.windows_total,
            windows_teachers: s.windows_teachers,
            windows_groups: s.windows_groups,
        },
    })
}
