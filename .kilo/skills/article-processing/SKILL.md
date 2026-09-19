---
name: article-processing
description: Build or debug Ornith's article cleanup and Kindle materialization pipeline. Use for HTML extraction, compression, embedded images, and output validation.
---

# Article processing

The output is a reader-ready, self-contained HTML article stored as a compressed
SQLite BLOB. Preserve readability offline and keep the payload suitable for an
ARMv7 grayscale Kindle.

## Processing contract

- Retain meaningful article structure and text; remove unsafe or unnecessary
  active/external content according to existing project policy.
- Fetch/select images only when they contribute to the article. Downscale and
  convert them to grayscale, then embed them as Base64 data URLs in the HTML.
- Do not leave external image, stylesheet, script, font, or network dependencies
  in the final materialized article.
- Compress after materialization; decode/decompress only at defined boundaries.

## Debug cheaply

Search for the pipeline stage owning the bad output. Inspect one representative
article and one failing element, not a feed corpus. Validate: decompression,
HTML references, image dimensions/color mode, Base64 decode, and a small
offline render path. Keep transformations bounded and preserve useful fallbacks
when extraction or image processing fails.
