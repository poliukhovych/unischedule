mod error;
mod state;
mod telemetry;
pub mod routes {
    pub mod explain;
    pub mod health;
    pub mod jobs;
    pub mod solve;
    pub mod validate;
}

use axum::{
    routing::{get, post},
    Router,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
        paths(
            routes::health::health,
            routes::solve::solve,
            routes::jobs::status,
            routes::jobs::result,
            routes::validate::validate_handler,
            routes::explain::explain,
            routes::solve::reoptimize,
        ),
        components(schemas(
            types::Instance, types::Teacher, types::Group, types::Room, types::Course,
            types::Policy, types::SoftWeights, types::SolveParams, types::SolveEnvelope,
            types::SolveResult, types::Assignment, types::Violation, types::SolverKind,
            types::TeacherPrefs, types::DayOfWeek, types::Equip, types::TimeslotId,
            types::TeacherId, types::GroupId, types::RoomId, types::CourseId,
            jobs::JobId, jobs::JobStatus,
            routes::validate::ValidationReport,
            routes::solve::JobCreated,
            routes::explain::ExplainIn,
            routes::explain::ExplainOut,
            routes::explain::Weights,
            routes::explain::Counts
        )),
        tags(
            (name = "unischedule", description = "Scheduling API")
        )
    )]
struct ApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let app_state = state::AppState::new_default();

    let app = Router::new()
        .route("/v1/health", get(routes::health::health))
        .route("/v1/solve", post(routes::solve::solve))
        .route("/v1/reoptimize", post(routes::solve::reoptimize))
        .route("/v1/validate", post(routes::validate::validate_handler))
        .route("/v1/explain", post(routes::explain::explain))
        .route("/v1/jobs/:id", get(routes::jobs::status))
        .route("/v1/jobs/:id/result", get(routes::jobs::result))
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .with_state(app_state);

    let port = std::env::var("UNISCHEDULE__SERVER__PORT").unwrap_or_else(|_| "8080".into());
    let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port)
        .parse()
        .expect("invalid listen addr");
    tracing::info!(%addr, "listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
