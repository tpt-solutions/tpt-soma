use md5;
use reqwest::Client;
use std::env;

pub struct ObjectStoreClient {
    client: Client,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
}

impl ObjectStoreClient {
    pub fn from_env() -> Self {
        let endpoint =
            env::var("MINIO_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
        let bucket = env::var("MINIO_BUCKET").unwrap_or_else(|_| "raw-omics".to_string());
        let access_key = env::var("MINIO_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
        let secret_key = env::var("MINIO_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());
        let client = Client::builder().build().expect("reqwest client");
        Self {
            client,
            endpoint,
            bucket,
            access_key,
            secret_key,
        }
    }

    fn put_url(&self, key: &str) -> String {
        format!("{}/{}/{}", self.endpoint, self.bucket, key)
    }

    pub async fn upload_with_checksum(
        &self,
        key: &str,
        data: Vec<u8>,
        expected_checksum: &str,
    ) -> Result<(), StoreError> {
        let url = self.put_url(key);
        let response = self
            .client
            .put(&url)
            .basic_auth(&self.access_key, Some(&self.secret_key))
            .body(data)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(StoreError::UploadFailed(
                response.status().as_str().to_string(),
            ));
        }
        let bytes = response.bytes().await?;
        let actual_checksum = format!("{:x}", md5::compute(bytes));
        if actual_checksum != expected_checksum {
            return Err(StoreError::ChecksumMismatch);
        }
        Ok(())
    }

    pub async fn upload(&self, key: &str, data: Vec<u8>) -> Result<(), StoreError> {
        let url = self.put_url(key);
        let response = self
            .client
            .put(&url)
            .basic_auth(&self.access_key, Some(&self.secret_key))
            .body(data)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(StoreError::UploadFailed(
                response.status().as_str().to_string(),
            ));
        }
        Ok(())
    }

    pub async fn upload_to_quarantine(&self, key: &str, data: Vec<u8>) -> Result<(), StoreError> {
        let quarantine_bucket = format!("{}-quarantine", self.bucket);
        let url = format!("{}/{}/{}", self.endpoint, quarantine_bucket, key);
        let response = self
            .client
            .put(&url)
            .basic_auth(&self.access_key, Some(&self.secret_key))
            .body(data)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(StoreError::UploadFailed(
                response.status().as_str().to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("upload failed: {0}")]
    UploadFailed(String),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("request error: {0}")]
    Request(#[from] reqwest::Error),
}
