use parking_lot::RwLock;
use sched_core::{SolveEnvelope, SolveResult, Solver};
use std::collections::HashMap;
use tracing::error;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct JobId(pub String);

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ToSchema)]
#[serde(tag = "status")]
pub enum JobStatus {
    Queued,
    Running,
    Solved { result: SolveResult },
    Infeasible,
    Failed { message: String },
}

#[derive(Clone)]
pub struct InMemJobs<S: Solver> {
    inner: std::sync::Arc<RwLock<HashMap<String, JobStatus>>>,
    solver: std::sync::Arc<S>,
}

impl<S: Solver> InMemJobs<S> {
    pub fn new(solver: S) -> Self {
        Self {
            inner: Default::default(),
            solver: std::sync::Arc::new(solver),
        }
    }

    pub fn enqueue(&self, env: SolveEnvelope) -> JobId {
        let id = Uuid::new_v4().to_string();
        self.inner.write().insert(id.clone(), JobStatus::Queued);

        let map = self.inner.clone();
        let solver = self.solver.clone();
        let id_for_task = id.clone();

        tokio::spawn(async move {
            {
                let mut w = map.write();
                w.insert(id_for_task.clone(), JobStatus::Running);
            }
            match solver.solve(env).await {
                Ok(res) => {
                    map.write()
                        .insert(id_for_task, JobStatus::Solved { result: res });
                }
                Err(e) => {
                    error!(?e, "job failed");
                    map.write().insert(
                        id_for_task,
                        JobStatus::Failed {
                            message: e.to_string(),
                        },
                    );
                }
            }
        });

        JobId(id)
    }

    pub fn get(&self, id: &str) -> Option<JobStatus> {
        self.inner.read().get(id).cloned()
    }
}
