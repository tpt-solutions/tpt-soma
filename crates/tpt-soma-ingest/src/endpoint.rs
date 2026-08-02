use crate::h5ad::AnnDataParser;
use crate::vcf::VcfParser;
use axum::{
    Router,
    body::Body,
    extract::Multipart,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use std::sync::Arc;
use tpt_soma_core::store::ObjectStoreClient;
use uuid::Uuid;

pub fn router() -> Router<Arc<ObjectStoreClient>> {
    Router::new()
        .route("/ingest/vcf", post(upload_vcf))
        .route("/ingest/h5ad", post(upload_h5ad))
}

async fn upload_vcf(
    State(store): State<Arc<ObjectStoreClient>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Multipart(e.to_string()))?
    {
        let name = field.name().unwrap_or("");
        if name == "file" {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Multipart(e.to_string()))?,
            );
        }
    }
    let data = data.ok_or_else(|| AppError::MissingField("file".into()))?;

    let tmp_path = format!("/tmp/upload_{}.vcf", Uuid::new_v4());
    std::fs::write(&tmp_path, &data)?;
    let parse_result = VcfParser::new(&tmp_path).parse();
    std::fs::remove_file(&tmp_path)?;

    let records = match parse_result {
        Ok(records) => records,
        Err(e) => {
            let quarantine_key = format!("quarantine/vcf/{}.vcf", Uuid::new_v4());
            let _ = store
                .upload_to_quarantine(&quarantine_key, data.to_vec())
                .await;
            return Err(AppError::Parse(format!(
                "{} (quarantined as {})",
                e, quarantine_key
            )));
        }
    };

    let object_key = format!("vcf/{}.vcf", Uuid::new_v4());
    let checksum = format!("{:x}", md5::compute(&data));
    store
        .upload_with_checksum(&object_key, data.to_vec(), &checksum)
        .await
        .map_err(|e| AppError::Store(e.to_string()))?;

    Ok((
        StatusCode::OK,
        format!(
            "{{\"status\":\"ok\",\"records\":{},\"object_key\":\"{}\"}}",
            records.len(),
            object_key
        ),
    ))
}

async fn upload_h5ad(
    State(store): State<Arc<ObjectStoreClient>>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut data = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::Multipart(e.to_string()))?
    {
        let name = field.name().unwrap_or("");
        if name == "file" {
            data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::Multipart(e.to_string()))?,
            );
        }
    }
    let data = data.ok_or_else(|| AppError::MissingField("file".into()))?;

    let tmp_path = format!("/tmp/upload_{}.h5ad", Uuid::new_v4());
    std::fs::write(&tmp_path, &data)?;
    let parse_result = AnnDataParser::new(&tmp_path).parse();
    std::fs::remove_file(&tmp_path)?;

    let result = match parse_result {
        Ok(result) => result,
        Err(e) => {
            let quarantine_key = format!("quarantine/h5ad/{}.h5ad", Uuid::new_v4());
            let _ = store
                .upload_to_quarantine(&quarantine_key, data.to_vec())
                .await;
            return Err(AppError::Parse(format!(
                "{} (quarantined as {})",
                e, quarantine_key
            )));
        }
    };

    let object_key = format!("h5ad/{}.h5ad", Uuid::new_v4());
    let checksum = format!("{:x}", md5::compute(&data));
    store
        .upload_with_checksum(&object_key, data.to_vec(), &checksum)
        .await
        .map_err(|e| AppError::Store(e.to_string()))?;

    Ok((
        StatusCode::OK,
        format!(
            "{{\"status\":\"ok\",\"cells\":{},\"object_key\":\"{}\"}}",
            result.n_cells(),
            object_key
        ),
    ))
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("multipart error: {0}")]
    Multipart(String),
    #[error("missing field: {0}")]
    MissingField(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("store error: {0}")]
    Store(String),
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
