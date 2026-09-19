//! Bounded synchronous feed HTTP client.

use std::time::Duration;

use ureq::ResponseExt;

use crate::Error;

pub const MAX_FEED_BYTES: u64 = 4 * 1024 * 1024;

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

        let mut builder = self.agent.get(request.url);
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
            return Err(Error::message(format!("feed returned HTTP {status}")));
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
}
