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

    pub fn bucket(&self) -> &str {
        &self.bucket
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    /// Serves one canned HTTP response on a random local port. The returned receiver
    /// yields the request line, so tests can check which bucket was targeted.
    async fn serve_once(
        status: &'static str,
        body: &'static [u8],
    ) -> (String, oneshot::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 65536];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            let request_line = request.lines().next().unwrap_or_default().to_string();
            let _ = tx.send(request_line);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            socket.write_all(head.as_bytes()).await.unwrap();
            socket.write_all(body).await.unwrap();
        });
        (format!("http://{addr}"), rx)
    }

    fn client_for(endpoint: String) -> ObjectStoreClient {
        ObjectStoreClient {
            client: Client::new(),
            endpoint,
            bucket: "raw-omics".to_string(),
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
        }
    }

    #[test]
    fn put_url_joins_endpoint_bucket_and_key() {
        let client = client_for("http://minio:9000".to_string());
        assert_eq!(
            client.put_url("sample/a.vcf"),
            "http://minio:9000/raw-omics/sample/a.vcf"
        );
        assert_eq!(client.bucket(), "raw-omics");
    }

    #[tokio::test]
    async fn upload_succeeds_on_success_status() {
        let (endpoint, _) = serve_once("200 OK", b"").await;
        let client = client_for(endpoint);
        client.upload("key", b"data".to_vec()).await.unwrap();
    }

    #[tokio::test]
    async fn upload_reports_failure_status() {
        let (endpoint, _) = serve_once("403 Forbidden", b"").await;
        let client = client_for(endpoint);
        let err = client.upload("key", b"data".to_vec()).await.unwrap_err();
        assert!(matches!(err, StoreError::UploadFailed(ref s) if s == "403"));
    }

    #[tokio::test]
    async fn upload_reports_connection_error() {
        let client = client_for("http://127.0.0.1:1".to_string());
        let err = client.upload("key", b"data".to_vec()).await.unwrap_err();
        assert!(matches!(err, StoreError::Request(_)));
    }

    #[tokio::test]
    async fn upload_to_quarantine_targets_quarantine_bucket() {
        let (endpoint, request_line) = serve_once("200 OK", b"").await;
        let client = client_for(endpoint);
        client
            .upload_to_quarantine("bad.vcf", b"data".to_vec())
            .await
            .unwrap();
        let line = request_line.await.unwrap();
        assert!(
            line.contains("/raw-omics-quarantine/bad.vcf"),
            "request line: {line}"
        );
    }

    #[tokio::test]
    async fn upload_to_quarantine_reports_failure_status() {
        let (endpoint, _) = serve_once("500 Internal Server Error", b"").await;
        let client = client_for(endpoint);
        let err = client
            .upload_to_quarantine("bad.vcf", b"data".to_vec())
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::UploadFailed(ref s) if s == "500"));
    }

    #[tokio::test]
    async fn upload_with_checksum_accepts_matching_md5() {
        // md5("hello") = 5d41402abc4b2a76b9719d911017c592
        let (endpoint, _) = serve_once("200 OK", b"hello").await;
        let client = client_for(endpoint);
        client
            .upload_with_checksum("key", b"hello".to_vec(), "5d41402abc4b2a76b9719d911017c592")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn upload_with_checksum_rejects_mismatch() {
        let (endpoint, _) = serve_once("200 OK", b"hello").await;
        let client = client_for(endpoint);
        let err = client
            .upload_with_checksum("key", b"hello".to_vec(), "00000000000000000000000000000000")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::ChecksumMismatch));
    }

    #[tokio::test]
    async fn upload_with_checksum_reports_failure_status() {
        let (endpoint, _) = serve_once("503 Service Unavailable", b"").await;
        let client = client_for(endpoint);
        let err = client
            .upload_with_checksum("key", b"hello".to_vec(), "5d41402abc4b2a76b9719d911017c592")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::UploadFailed(ref s) if s == "503"));
    }
}
