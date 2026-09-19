//! Minimal embedded-content article representation for the first feed slice.

use std::io::Write;

use flate2::{write::ZlibEncoder, Compression};

use crate::feed::ParsedEntry;
use crate::Error;

pub const COMPRESSION_CODEC: i64 = 1;
pub const CONTENT_FORMAT: i64 = 1;
pub const STORAGE_VERSION: i64 = 1;

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Wrap an embedded feed body (or summary) in a complete HTML5 document.
pub fn wrap(entry: &ParsedEntry) -> Result<String, Error> {
    let title = entry
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| Error::message("feed entry has no title"))?;
    let body = entry
        .content
        .as_deref()
        .or(entry.summary.as_deref())
        .filter(|body| !body.trim().is_empty())
        .ok_or_else(|| Error::message("feed entry has no embedded content or summary"))?;
    let byline = entry
        .authors
        .first()
        .map(|author| escape(author))
        .unwrap_or_default();
    let date = entry
        .published_at
        .or(entry.updated_at)
        .map(|timestamp| timestamp.to_string())
        .unwrap_or_default();
    let metadata = match (byline.is_empty(), date.is_empty()) {
        (true, true) => String::new(),
        _ => format!("<p class=\"byline\">{} {}</p>", byline, escape(&date)),
    };
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>img{{max-width:100%;height:auto}}pre{{white-space:pre-wrap}}</style></head><body><article><header><h1>{}</h1>{}</header>{}</article></body></html>",
        escape(title), escape(title), metadata, body
    ))
}

pub fn compress(html: &str) -> Result<Vec<u8>, Error> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(html.as_bytes())?;
    Ok(encoder.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_and_compresses_embedded_content() {
        let entry = ParsedEntry {
            id: "id".into(),
            url: None,
            title: Some("A <title>".into()),
            authors: vec!["Ada".into()],
            published_at: Some(1),
            updated_at: None,
            content: Some("<p>Hello</p>".into()),
            summary: None,
            dedupe_key: "g:id".into(),
        };
        let html = wrap(&entry).expect("wrap");
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("A &lt;title&gt;"));
        assert!(!compress(&html).expect("compress").is_empty());
    }
}
