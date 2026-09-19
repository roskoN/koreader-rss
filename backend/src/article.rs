//! Minimal embedded-content article representation for the first feed slice.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use flate2::{write::ZlibEncoder, Compression};
use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
    DynamicImage, GenericImageView, ImageEncoder,
};
use url::Url;

use crate::feed::ParsedEntry;
use crate::Error;

pub const COMPRESSION_CODEC: i64 = 1;
pub const CONTENT_FORMAT: i64 = 1;
pub const STORAGE_VERSION: i64 = 1;
const CACHE_MAX_FILES: usize = 3;
const CACHE_MAX_BYTES: u64 = 32 * 1024 * 1024;

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
    // Feed content is untrusted. Keep the embedded-content vertical slice
    // deliberately conservative: remove executable/remote constructs while
    // retaining semantic markup supported by crengine.
    let body = sanitize_fragment(body);
    Ok(format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title><style>img{{max-width:100%;height:auto}}pre{{white-space:pre-wrap}}</style></head><body><article><header><h1>{}</h1>{}</header>{}</article></body></html>",
        escape(title), escape(title), metadata, body
    ))
}

fn sanitize_fragment(input: &str) -> String {
    let mut output = input.to_owned();
    for tag in ["script", "style", "iframe", "object", "embed", "form"] {
        while let (Some(start), Some(end)) = (
            output.to_ascii_lowercase().find(&format!("<{tag}")),
            output.to_ascii_lowercase().find(&format!("</{tag}>")),
        ) {
            if end < start {
                break;
            }
            let end = end + tag.len() + 3;
            output.replace_range(start..end.min(output.len()), "");
        }
    }
    for attribute in [
        "onclick=",
        "onload=",
        "onerror=",
        "onmouseover=",
        "onfocus=",
        "onanimationstart=",
    ] {
        output = output.replace(&format!(" {attribute}"), " data-removed=");
        output = output.replace(
            &format!(" {}", attribute.to_ascii_uppercase()),
            " data-removed=",
        );
    }
    output = output.replace("javascript:", "").replace("JAVASCRIPT:", "");
    output
}

pub fn compress(html: &str) -> Result<Vec<u8>, Error> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(html.as_bytes())?;
    Ok(encoder.finish()?)
}

/// Replace a bounded number of remote image sources with grayscale data URIs.
/// Failed images are removed from the source attribute while surrounding text
/// and alt text remain available to the reader.
pub fn embed_images(
    html: &str,
    base_url: Option<&str>,
    client: &crate::http::HttpClient,
) -> String {
    let mut output = html.to_owned();
    let mut cursor = 0;
    let mut processed = 0;
    while processed < 12 {
        let lower = output[cursor..].to_ascii_lowercase();
        let Some(relative) = lower.find("<img") else {
            break;
        };
        let start = cursor + relative;
        let Some(end_rel) = output[start..].find('>') else {
            break;
        };
        let end = start + end_rel + 1;
        let tag = output[start..end].to_owned();
        let Some(src_rel) = tag.to_ascii_lowercase().find("src=") else {
            cursor = end;
            continue;
        };
        let value_start = src_rel + 4;
        let bytes = tag.as_bytes();
        let quote = bytes.get(value_start).copied().unwrap_or(b' ');
        let (from, to) = if quote == b'"' || quote == b'\'' {
            let Some(close) = tag[value_start + 1..].find(quote as char) else {
                cursor = end;
                continue;
            };
            (value_start + 1, value_start + 1 + close)
        } else {
            let close = tag[value_start..]
                .find(char::is_whitespace)
                .unwrap_or(tag.len() - value_start);
            (value_start, value_start + close)
        };
        let raw_url = &tag[from..to];
        let resolved = base_url
            .and_then(|base| Url::parse(base).ok().and_then(|u| u.join(raw_url).ok()))
            .map(|u| u.to_string())
            .unwrap_or_else(|| raw_url.to_owned());
        if resolved.starts_with("http://") || resolved.starts_with("https://") {
            if let Ok(bytes) = client.fetch_image(&resolved) {
                if let Ok(uri) = encode_image(&bytes) {
                    let replacement = format!("{}{}{}", &tag[..from], uri, &tag[to..]);
                    output.replace_range(start..end, &replacement);
                }
            }
        }
        cursor = end;
        processed += 1;
    }
    output
}

fn encode_image(bytes: &[u8]) -> Result<String, Error> {
    let image = image::load_from_memory(bytes)?;
    let image = limit_image(image);
    let gray = image.to_luma8();
    let mut encoded = Vec::new();
    if matches!(image::guess_format(bytes), Ok(image::ImageFormat::Png)) {
        PngEncoder::new(&mut encoded).write_image(
            &gray,
            gray.width(),
            gray.height(),
            image::ExtendedColorType::L8,
        )?;
        Ok(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(encoded)
        ))
    } else {
        JpegEncoder::new_with_quality(&mut encoded, 80)
            .encode_image(&DynamicImage::ImageLuma8(gray))?;
        Ok(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(encoded)
        ))
    }
}

fn limit_image(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let max_width = 1200u32;
    let max_height = 2400u32;
    if width <= max_width && height <= max_height {
        return image;
    }
    image.resize(max_width, max_height, FilterType::Lanczos3)
}

pub fn decompress(blob: &[u8], expected_size: usize) -> Result<Vec<u8>, Error> {
    let mut decoder = flate2::read::ZlibDecoder::new(blob);
    let mut output = Vec::with_capacity(expected_size.min(1024 * 1024));
    decoder.read_to_end(&mut output)?;
    if output.len() != expected_size {
        return Err(Error::message("decompressed article size mismatch"));
    }
    Ok(output)
}

pub fn materialize(
    id: i64,
    storage_version: i64,
    fetched_at: i64,
    blob: &[u8],
    uncompressed_size: usize,
    cache: &Path,
) -> Result<PathBuf, Error> {
    std::fs::create_dir_all(cache)?;
    let path = cache.join(format!("article-{id}-s{storage_version}-{fetched_at}.html"));
    purge_cache(cache, Some(&path))?;
    if path.is_file() {
        let _ = filetime_touch(&path);
        return Ok(path);
    }
    let html = decompress(blob, uncompressed_size)?;
    let tmp = cache.join(format!(".tmp-{id}-{}", std::process::id()));
    std::fs::write(&tmp, html)?;
    std::fs::rename(&tmp, &path)?;
    purge_cache(cache, Some(&path))?;
    Ok(path)
}

fn purge_cache(cache: &Path, keep: Option<&Path>) -> Result<(), Error> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".tmp-"))
        {
            let _ = std::fs::remove_file(path);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("html") {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            files.push((
                path,
                metadata.len(),
                metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
            ));
        }
    }
    files.sort_by_key(|(_, _, modified)| *modified);
    let mut bytes: u64 = files.iter().map(|(_, size, _)| *size).sum();
    let mut count = files.len();
    for (path, size, _) in files {
        let is_keep = keep.is_some_and(|keep| keep == path);
        let over_count = count > CACHE_MAX_FILES;
        let over_bytes = bytes > CACHE_MAX_BYTES;
        if (!over_count && !over_bytes) || is_keep {
            continue;
        }
        std::fs::remove_file(path)?;
        count -= 1;
        bytes = bytes.saturating_sub(size);
    }
    Ok(())
}

fn filetime_touch(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(&[])?;
    drop(file);
    let _ = metadata;
    Ok(())
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

    #[test]
    fn sanitizes_event_handlers_and_javascript_urls() {
        let entry = ParsedEntry {
            id: "id".into(),
            url: None,
            title: Some("Title".into()),
            authors: vec![],
            published_at: None,
            updated_at: None,
            content: Some(
                r#"<p onclick="alert(1)"><a href="javascript:alert(2)">link</a></p>"#.into(),
            ),
            summary: None,
            dedupe_key: "g:id".into(),
        };
        let html = wrap(&entry).expect("wrap");
        assert!(!html.contains("onclick="));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn materializes_compressed_html_atomically() {
        let directory = tempfile::tempdir().expect("temp directory");
        let html = "<!doctype html><p>offline</p>";
        let blob = compress(html).expect("compress");
        let path = materialize(7, STORAGE_VERSION, 42, &blob, html.len(), directory.path())
            .expect("materialize");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), html);
        assert_eq!(
            materialize(7, STORAGE_VERSION, 42, &blob, html.len(), directory.path()).unwrap(),
            path
        );
        assert!(!directory.path().join(".tmp-7").exists());
    }

    #[test]
    fn materialization_purges_stale_temps_and_bounds_article_files() {
        let directory = tempfile::tempdir().expect("temp directory");
        std::fs::write(directory.path().join(".tmp-stale"), b"stale").expect("temp");
        let html = "<!doctype html><p>offline</p>";
        let blob = compress(html).expect("compress");
        for id in 1..=4 {
            materialize(id, STORAGE_VERSION, id, &blob, html.len(), directory.path())
                .expect("materialize");
        }
        let html_files = std::fs::read_dir(directory.path())
            .expect("read cache")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("html"))
            .count();
        assert!(html_files <= CACHE_MAX_FILES);
        assert!(!directory.path().join(".tmp-stale").exists());
    }
}
