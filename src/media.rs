use anyhow::{anyhow, Result};
use tracing::{debug, warn};
use reqwest::Client;

const MAX_DISCORD_FILE_SIZE: usize = 8 * 1024 * 1024;
const MAX_MATRIX_FILE_SIZE: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub data: Vec<u8>,
    pub content_type: String,
    pub filename: String,
    pub size: usize,
}

pub struct MediaHandler {
    client: Client,
    homeserver_url: String,
}

impl MediaHandler {
    pub fn new(homeserver_url: &str) -> Self {
        Self {
            client: Client::new(),
            homeserver_url: homeserver_url.to_string(),
        }
    }

    pub async fn download_from_url(&self, url: &str) -> Result<MediaInfo> {
        debug!("downloading media from {}", url);
        
        let response = self.client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow!("failed to download from {}: {}", url, e))?;
        
        if !response.status().is_success() {
            return Err(anyhow!("failed to download from {}: status {}", url, response.status()));
        }
        
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        
        let data = response
            .bytes()
            .await
            .map_err(|e| anyhow!("failed to read response body: {}", e))?
            .to_vec();
        
        let size = data.len();
        let filename = url
            .rsplit('/')
            .next()
            .unwrap_or("attachment")
            .to_string();
        
        debug!("downloaded {} bytes from {}", size, url);
        
        Ok(MediaInfo {
            data,
            content_type,
            filename,
            size,
        })
    }

    pub async fn download_matrix_media(&self, mxc_url: &str) -> Result<MediaInfo> {
        if !mxc_url.starts_with("mxc://") {
            return Err(anyhow!("invalid mxc URL: {}", mxc_url));
        }
        
        let mxc_path = mxc_url.trim_start_matches("mxc://");
        let download_url = format!(
            "{}/_matrix/media/v3/download/{}",
            self.homeserver_url.trim_end_matches('/'),
            mxc_path
        );
        
        self.download_from_url(&download_url).await
    }

    pub async fn upload_to_matrix(
        &self,
        media: &MediaInfo,
        access_token: &str,
    ) -> Result<String> {
        if media.size > MAX_MATRIX_FILE_SIZE {
            return Err(anyhow!(
                "file too large for Matrix: {} bytes (max {})",
                media.size,
                MAX_MATRIX_FILE_SIZE
            ));
        }
        
        let upload_url = format!(
            "{}/_matrix/media/v3/upload?filename={}",
            self.homeserver_url.trim_end_matches('/'),
            urlencoding::encode(&media.filename)
        );
        
        debug!("uploading {} to Matrix", media.filename);
        
        let response = self.client
            .post(&upload_url)
            .header("Authorization", format!("Bearer {}", access_token))
            .header("Content-Type", &media.content_type)
            .body(media.data.clone())
            .send()
            .await
            .map_err(|e| anyhow!("failed to upload to Matrix: {}", e))?;
        
        let status = response.status();
        
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("failed to upload to Matrix: {} - {}", status, body));
        }
        
        let body_bytes = response.bytes().await
            .map_err(|e| anyhow!("failed to read response body: {}", e))?;
        let json: serde_json::Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| anyhow!("failed to parse upload response: {}", e))?;
        
        let content_uri = json
            .get("content_uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("no content_uri in upload response"))?
            .to_string();
        
        debug!("uploaded to Matrix: {}", content_uri);
        Ok(content_uri)
    }

    pub fn check_discord_file_size(size: usize) -> Result<()> {
        if size > MAX_DISCORD_FILE_SIZE {
            warn!(
                "file too large for Discord: {} bytes (max {})",
                size, MAX_DISCORD_FILE_SIZE
            );
            Err(anyhow!(
                "file too large for Discord: {} bytes (max {})",
                size,
                MAX_DISCORD_FILE_SIZE
            ))
        } else {
            Ok(())
        }
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
    }
}
