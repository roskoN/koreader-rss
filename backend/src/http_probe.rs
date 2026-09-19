//! A deliberately tiny HTTPS probe for the Milestone 0 TLS boundary.

use std::time::Duration;

use crate::Error;

pub fn run(url: &str) -> Result<u16, Error> {
    if !url.starts_with("https://") {
        return Err(Error::message("http-probe requires an https:// URL"));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(5)))
        .timeout_global(Some(Duration::from_secs(20)))
        .https_only(true)
        .user_agent("rss-backend-milestone0/0.0.0")
        .build()
        .into();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| Error::message(format!("HTTPS probe failed: {error}")))?;
    Ok(response.status().as_u16())
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_cleartext_urls_without_network_access() {
        let error = super::run("http://example.com").expect_err("cleartext URL must fail");
        assert!(error.to_string().contains("https://"));
    }
}
