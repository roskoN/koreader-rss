//! Bounded synchronous feed HTTP client.

use std::time::Duration;

use ureq::ResponseExt;

use crate::Error;

pub const MAX_FEED_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

fn retry_after_seconds(value: &str) -> Option<i64> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .map(|seconds| seconds.clamp(0, 48 * 60 * 60))
}

#[derive(Debug, Clone)]
pub struct FeedRequest<'a> {
    pub url: &'a str,
    pub etag: Option<&'a str>,
    pub last_modified: Option<&'a str>,
}

#[derive(Debug)]
pub enum FeedResponse {
    NotModified {
        effective_url: String,
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Body {
        effective_url: String,
        etag: Option<String>,
        last_modified: Option<String>,
        bytes: Vec<u8>,
    },
}

pub struct HttpClient {
    agent: ureq::Agent,
}

impl HttpClient {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(Duration::from_secs(20)))
            .max_redirects(5)
            .http_status_as_error(false)
            .user_agent("rss-backend/0.1")
            .build()
            .into();
        Self { agent }
    }

    pub fn fetch_feed(&self, request: FeedRequest<'_>) -> Result<FeedResponse, Error> {
        if !(request.url.starts_with("https://") || request.url.starts_with("http://")) {
            return Err(Error::message("feed URL must use http:// or https://"));
        }

        let mut builder = self
            .agent
            .get(request.url)
            .header("Accept-Encoding", "gzip");
        if let Some(etag) = request.etag {
            builder = builder.header("If-None-Match", etag);
        }
        if let Some(last_modified) = request.last_modified {
            builder = builder.header("If-Modified-Since", last_modified);
        }

        let mut response = builder
            .call()
            .map_err(|error| Error::message(format!("feed request failed: {error}")))?;
        let status = response.status().as_u16();
        let effective_url = response.get_uri().to_string();
        let etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let last_modified = response
            .headers()
            .get("last-modified")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);

        if status == 304 {
            return Ok(FeedResponse::NotModified {
                effective_url,
                etag,
                last_modified,
            });
        }
        if !(200..300).contains(&status) {
            let retry_after_s = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(retry_after_seconds);
            return Err(Error::Http {
                message: format!("feed returned HTTP {status}"),
                retry_after_s: if matches!(status, 429 | 503) {
                    retry_after_s
                } else {
                    None
                },
            });
        }

        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_FEED_BYTES)
            .read_to_vec()
            .map_err(|error| Error::message(format!("cannot read bounded feed body: {error}")))?;
        Ok(FeedResponse::Body {
            effective_url,
            etag,
            last_modified,
            bytes,
        })
    }

    pub fn fetch_page(&self, url: &str) -> Result<(String, Vec<u8>), Error> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(Error::message("page URL must use http:// or https://"));
        }
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| Error::message(format!("page request failed: {error}")))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(Error::message(format!("page returned HTTP {status}")));
        }
        let effective = response.get_uri().to_string();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_PAGE_BYTES)
            .read_to_vec()
            .map_err(|error| Error::message(format!("cannot read bounded page: {error}")))?;
        Ok((effective, bytes))
    }

    pub fn fetch_image(&self, url: &str) -> Result<Vec<u8>, Error> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(Error::message("image URL must use http:// or https://"));
        }
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|error| Error::message(format!("image request failed: {error}")))?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(Error::message(format!(
                "image returned HTTP {}",
                response.status().as_u16()
            )));
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE_BYTES)
            .read_to_vec()
            .map_err(|error| Error::message(format!("cannot read bounded image: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_http_scheme_before_network_access() {
        let result = HttpClient::new().fetch_feed(FeedRequest {
            url: "file:///etc/passwd",
            etag: None,
            last_modified: None,
        });
        assert!(result
            .expect_err("scheme must fail")
            .to_string()
            .contains("http"));
    }

    #[test]
    fn retry_after_is_bounded_and_rejects_dates_without_clock_dependency() {
        assert_eq!(retry_after_seconds("120"), Some(120));
        assert_eq!(retry_after_seconds("999999"), Some(48 * 60 * 60));
        assert_eq!(retry_after_seconds("-1"), Some(0));
        assert_eq!(retry_after_seconds("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }
}
