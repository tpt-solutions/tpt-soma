use reqwest::Client;
use md5;

pub struct ObjectStoreClient {
    client: Client,
    endpoint: String,
    bucket: String,
}

impl ObjectStoreClient {
    pub fn new(endpoint: String, bucket: String) -> Self {
        let client = Client::builder().build().expect("reqwest client");
        Self { client, endpoint, bucket }
    }

    pub async fn upload_with_checksum(
        &self,
        key: &str,
        data: Vec<u8>,
        expected_checksum: &str,
    ) -> Result<(), StoreError> {
        let url = format!("{}/{}/{}", self.endpoint, self.bucket, key);
        let response = self.client.put(&url).body(data).send().await?;
        if !response.status().is_success() {
            return Err(StoreError::UploadFailed(response.status().as_str().to_string()));
        }
        let bytes = response.bytes().await?;
        let actual_checksum = format!("{:x}", md5::compute(bytes));
        if actual_checksum != expected_checksum {
            return Err(StoreError::ChecksumMismatch);
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
