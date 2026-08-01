use axum::{
    http::Request,
    middleware::Next,
    response::Response,
};

pub async fn capability_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, crate::Error> {
    let auth_header = req.headers().get("Authorization").ok_or("missing auth")?;
    let token_str = auth_header.to_str()?;
    let _ = token_str;
    Ok(next.run(req).await)
}
