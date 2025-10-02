
#[utoipa::path(
         get,
         path = "/v1/health",
         responses((status = 200, description = "OK"))
     )]
pub async fn health() -> &'static str {
    "ok"
}
