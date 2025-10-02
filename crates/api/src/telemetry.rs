use tower::layer::util::{Identity, Stack};
use tower::ServiceBuilder;
use tower_http::trace::HttpMakeClassifier;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer, trace::TraceLayer};

pub fn stack() -> ServiceBuilder<
    Stack<RequestBodyLimitLayer, Stack<CorsLayer, Stack<TraceLayer<HttpMakeClassifier>, Identity>>>,
> {
    let trace = TraceLayer::new_for_http();
    let cors = CorsLayer::permissive();
    let limit = RequestBodyLimitLayer::new(2 * 1024 * 1024);

    ServiceBuilder::new().layer(trace).layer(cors).layer(limit)
}
