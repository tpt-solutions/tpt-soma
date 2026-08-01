use axum::{
    extract::Multipart,
    http::StatusCode,
    response::{IntoResponse, Response},
    Router,
    routing::post,
    body::Body,
};
use crate::vcf::VcfParser;
use crate::h5ad::AnnDataParser;
use uuid::Uuid;

pub fn router() -> Router {
    Router::new()
        .route("/ingest/vcf", post(upload_vcf))
        .route("/ingest/h5ad", post(upload_h5ad))
}

async fn upload_vcf(
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut tmp_path = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::Multipart(e.to_string()))? {
        let name = field.name().unwrap_or("");
        if name == "file" {
            let data = field.bytes().await.map_err(|e| AppError::Multipart(e.to_string()))?;
            let path = format!("/tmp/upload_{}.vcf", Uuid::new_v4());
            std::fs::write(&path, data)?;
            tmp_path = Some(path);
        }
    }
    let path = tmp_path.ok_or_else(|| AppError::MissingField("file".into()))?;
    let records = VcfParser::new(&path).parse().map_err(|e| AppError::Parse(e.to_string()))?;
    std::fs::remove_file(&path)?;
    Ok((StatusCode::OK, format!("{{\"status\":\"ok\",\"records\":{}}}", records.len())))
}

async fn upload_h5ad(
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut tmp_path = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::Multipart(e.to_string()))? {
        let name = field.name().unwrap_or("");
        if name == "file" {
            let data = field.bytes().await.map_err(|e| AppError::Multipart(e.to_string()))?;
            let path = format!("/tmp/upload_{}.h5ad", Uuid::new_v4());
            std::fs::write(&path, data)?;
            tmp_path = Some(path);
        }
    }
    let path = tmp_path.ok_or_else(|| AppError::MissingField("file".into()))?;
    let adata = AnnDataParser::new(&path).parse().map_err(|e| AppError::Parse(e.to_string()))?;
    std::fs::remove_file(&path)?;
    Ok((StatusCode::OK, format!("{{\"status\":\"ok\",\"cells\":{}}}", adata.obs_count)))
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("multipart error: {0}")]
    Multipart(String),
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response<Body> {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from(self.to_string()))
            .unwrap()
    }
}
