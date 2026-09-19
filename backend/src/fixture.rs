//! Generate the self-contained HTML used to validate KOReader rendering.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, GrayImage, ImageEncoder, Luma};

use crate::Error;

pub const DEFAULT_TITLE: &str = "Milestone 0 Offline Rendering Probe";

fn gray_image(size: u32, value: impl Fn(u32, u32) -> u8) -> GrayImage {
    let mut image = GrayImage::new(size, size);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = Luma([value(x, y)]);
    }
    image
}

fn png_bytes() -> Result<Vec<u8>, Error> {
    let image = gray_image(96, |x, y| ((x + y) * 255 / 190) as u8);
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes).write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        ExtendedColorType::L8,
    )?;
    Ok(bytes)
}

fn jpeg_bytes() -> Result<Vec<u8>, Error> {
    let image = gray_image(96, |x, y| {
        if ((x / 12) + (y / 12)) % 2 == 0 {
            32
        } else {
            224
        }
    });
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 80).encode(
        image.as_raw(),
        image.width(),
        image.height(),
        ExtendedColorType::L8,
    )?;
    Ok(bytes)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn html(title: &str) -> Result<String, Error> {
    let png = STANDARD.encode(png_bytes()?);
    let jpeg = STANDARD.encode(jpeg_bytes()?);
    let title = escape(title);
    Ok(format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>img{{max-width:100%;height:auto}}pre{{white-space:pre-wrap}}table{{border-collapse:collapse}}td,th{{border:1px solid;padding:4px}}</style>
</head><body><article>
<header><h1>{title}</h1><p>rss-backend · milestone0 · offline fixture</p></header>
<p>This complete HTML document has no external display dependencies.</p>
<figure><img src="data:image/png;base64,{png}" width="96" height="96" alt="Grayscale PNG gradient"><figcaption>Embedded grayscale PNG.</figcaption></figure>
<figure><img src="data:image/jpeg;base64,{jpeg}" width="96" height="96" alt="Grayscale JPEG checkerboard"><figcaption>Embedded grayscale JPEG.</figcaption></figure>
<h2>Semantic HTML</h2><blockquote>Offline rendering boundary probe.</blockquote>
<ul><li>Headings and lists</li><li><strong>Emphasis</strong></li></ul>
<table><tr><th>Format</th><th>Embedded</th></tr><tr><td>PNG</td><td>yes</td></tr><tr><td>JPEG</td><td>yes</td></tr></table>
<pre>rss-backend materialize-fixture --out fixture.html</pre>
<p><a href="https://example.org/">External link retained</a></p>
</article></body></html>"#
    ))
}

fn absolute(path: &Path) -> Result<PathBuf, Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

pub fn materialize(path: &Path) -> Result<PathBuf, Error> {
    let path = absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::message("output has no parent"))?;
    fs::create_dir_all(parent)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp = parent.join(format!(".rss-fixture-{}-{stamp}.tmp", std::process::id()));
    let result = (|| -> Result<(), Error> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(html(DEFAULT_TITLE)?.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_contains_both_embedded_image_formats() {
        let html = html("A & B").expect("fixture HTML");
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("data:image/jpeg;base64,"));
        assert!(html.contains("<table>"));
        assert!(html.contains("A &amp; B"));
        assert!(!html.contains("src=\"http"));
    }

    #[test]
    fn materialization_is_complete_and_leaves_no_temp_file() {
        let directory = tempfile::tempdir().expect("temp directory");
        let output = directory.path().join("fixture.html");
        let written = materialize(&output).expect("materialize fixture");
        assert_eq!(written, output);
        materialize(&output).expect("replace fixture atomically");
        assert!(fs::read_to_string(&output)
            .expect("read fixture")
            .ends_with("</html>"));
        assert_eq!(
            fs::read_dir(directory.path()).expect("list temp").count(),
            1
        );
    }
}
