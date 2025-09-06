use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use headers::{authorization::Bearer, Authorization, HeaderMapExt};
use std::sync::Arc;

use crate::config::Config;

pub async fn auth_middleware(
    State(config): State<Arc<Config>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = req.headers()
        .typed_get::<Authorization<Bearer>>()
        .and_then(|auth| Some(auth.token().to_string()));

    if let Some(api_token) = &config.monitoring.api_token {
        if token.is_none() || &token.unwrap() != api_token {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(next.run(req).await)
}
