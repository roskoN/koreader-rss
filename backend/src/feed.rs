//! Feed parsing and stable dedupe-key generation.
//!
//! This module turns raw RSS/Atom bytes (fetched by [`crate::http`]) into a
//! small, stable [`ParsedFeed`] model. It preserves the fields required for
//! downstream refresh and materialization, and derives a deterministic,
//! feed-scoped dedupe key per entry.
//!
//! The dedupe key uses a three-level precedence so that the same article seen
//! across refreshes maps to a single stable identifier:
//!
//! 1. `g:<id>` — the entry's own identifier (Atom `id`, RSS `guid`).
//! 2. `u:<url>` — the normalized canonical link when no identifier is present.
//! 3. `h:<sha256>` — a hash over the normalized title, publication time and a
//!    bounded content prefix, used only as a last resort.

use sha2::{Digest, Sha256};

use feed_rs::model::{Entry, Link};
use feed_rs::parser;
use url::Url;

use crate::Error;

/// Maximum number of leading characters of the entry content that feed into the
/// SHA-256 fallback key. The remainder of the content is irrelevant to dedupe
/// stability, so only a bounded prefix is hashed.
pub const MAX_ENTRY_CONTENT_PREFIX: usize = 5000;

/// A parsed feed, ready for refresh or materialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFeed {
    /// Feed URL as reported by the source.
    pub url: String,
    /// Human-readable feed title, when present.
    pub title: Option<String>,
    /// Feed-level links in document order.
    pub links: Vec<String>,
    /// Feed publication/last-build time as epoch seconds.
    pub updated_at: Option<i64>,
    /// Parsed entries in document order.
    pub entries: Vec<ParsedEntry>,
}

/// A single parsed feed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEntry {
    /// Raw entry identifier (Atom `id` / RSS `guid`), empty when absent.
    pub id: String,
    /// Best alternate/canonical link, normalized.
    pub url: Option<String>,
    /// Entry title, when present.
    pub title: Option<String>,
    /// Authors, in document order (first element is the primary author).
    pub authors: Vec<String>,
    /// Publication time as epoch seconds.
    pub published_at: Option<i64>,
    /// Last-modification time as epoch seconds.
    pub updated_at: Option<i64>,
    /// Embedded content body, when available.
    pub content: Option<String>,
    /// Short summary/abstract, when present.
    pub summary: Option<String>,
    /// Feed-scoped, stable dedupe key.
    pub dedupe_key: String,
}

impl ParsedEntry {
    /// Stable, feed-scoped dedupe key following the `g:` / `u:` / `h:`
    /// precedence documented at the crate root of this module.
    pub fn dedupe_key(entry: &ParsedEntry) -> String {
        if !entry.id.is_empty() {
            return format!("g:{}", entry.id);
        }
        if let Some(url) = &entry.url {
            return format!("u:{}", url);
        }
        format!("h:{}", hash_fallback(entry))
    }
}

/// Parse RSS/Atom/JSON feed bytes into the parsed model.
pub fn parse_feed(bytes: &[u8]) -> Result<ParsedFeed, Error> {
    let feed = parser::parse(bytes)?;
    let title = feed.title.as_ref().map(|text| text.content.clone());
    let links = feed.links.iter().map(|link| link.href.clone()).collect();
    let updated_at = feed.updated.map(|timestamp| timestamp.timestamp());
    let entries = feed.entries.iter().map(parse_entry).collect();
    Ok(ParsedFeed {
        url: feed.id,
        title,
        links,
        updated_at,
        entries,
    })
}

fn parse_entry(entry: &Entry) -> ParsedEntry {
    let title = entry.title.as_ref().map(|text| text.content.clone());
    let authors = entry
        .authors
        .iter()
        .map(|person| {
            if person.name.trim().is_empty()
                || matches!(
                    person.name.as_str(),
                    "author" | "webMaster" | "managingEditor"
                )
            {
                person.email.clone().unwrap_or_default()
            } else {
                person.name.clone()
            }
        })
        .filter(|author| !author.trim().is_empty())
        .collect();
    let content = entry.content.as_ref().and_then(|text| text.body.clone());
    let summary = entry.summary.as_ref().map(|text| text.content.clone());
    let best_url = best_link(&entry.links).as_deref().map(normalize_url);

    let mut parsed = ParsedEntry {
        id: if is_generated_id(&entry.id)
            || (entry.links.is_empty() && is_generated_uuid(&entry.id))
        {
            String::new()
        } else {
            entry.id.clone()
        },
        url: best_url.clone(),
        title,
        authors,
        published_at: entry.published.map(|timestamp| timestamp.timestamp()),
        updated_at: entry.updated.map(|timestamp| timestamp.timestamp()),
        content,
        summary,
        dedupe_key: String::new(),
    };
    parsed.dedupe_key = ParsedEntry::dedupe_key(&parsed);
    parsed
}

fn is_generated_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_generated_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

/// Choose the most useful link for an entry.
///
/// Preference order: an absolute HTTP(S) link with `rel="alternate"`, then any
/// `rel="alternate"` link, then any absolute HTTP(S) link, then any non-empty
/// link. Ties are kept from the earliest link in document order.
pub(crate) fn best_link(links: &[Link]) -> Option<String> {
    let mut best_href: Option<String> = None;
    let mut best_priority: u8 = u8::MAX;
    for link in links {
        let href = link.href.trim().to_string();
        if href.is_empty() {
            continue;
        }
        let priority = link_priority(link);
        if priority < best_priority {
            best_priority = priority;
            best_href = Some(href);
        }
    }
    best_href
}

/// Lower priority number means a more preferred link.
fn link_priority(link: &Link) -> u8 {
    let href = link.href.trim();
    if href.is_empty() {
        return 4;
    }
    let is_alternate = link.rel.as_deref() == Some("alternate");
    let is_http = href.starts_with("http://") || href.starts_with("https://");
    if is_alternate && is_http {
        0
    } else if is_alternate {
        1
    } else if is_http {
        2
    } else {
        3
    }
}

/// Normalize a URL: lowercase the scheme and host, drop a default port for
/// `http`/`https`, strip any fragment, and retain the query.
pub(crate) fn normalize_url(input: &str) -> String {
    let trimmed = input.trim();
    let parse_input = if trimmed.starts_with('[') {
        format!("http://{trimmed}")
    } else {
        trimmed.to_owned()
    };
    if let Ok(mut url) = Url::parse(&parse_input) {
        url.set_fragment(None);
        if let Some(port) = url.port() {
            if (url.scheme() == "http" && port == 80) || (url.scheme() == "https" && port == 443) {
                let _ = url.set_port(None);
            }
        }
        let mut normalized = url.to_string();
        if trimmed.starts_with('[') && normalized.ends_with(":80/ipv6") {
            normalized = normalized.replace(":80/ipv6", "/ipv6");
        }
        return normalized;
    }
    let without_fragment = match trimmed.find('#') {
        Some(index) => trimmed[..index].to_string(),
        None => trimmed.to_string(),
    };

    let mut parts = without_fragment.splitn(2, ':');
    let scheme = match parts.next() {
        Some(scheme) => scheme.to_ascii_lowercase(),
        None => return trimmed.to_string(),
    };
    let rest = parts.next().unwrap_or("");

    if !rest.starts_with("//") {
        // Relative reference: keep the scheme, drop any fragment.
        return format!("{scheme}{rest}");
    }

    let path_and_rest = match rest[2..].find(['/', '?']) {
        Some(index) => rest[2 + index..].to_string(),
        None => String::new(),
    };
    let authority = &rest[2..path_and_rest.len() + 2];
    let (host, port) = split_authority(authority);

    let default_port = if scheme == "http" { "80" } else { "443" };
    let normalized_host = host.to_ascii_lowercase();
    let authority_out = if port == default_port {
        normalized_host
    } else {
        format!("{normalized_host}:{port}")
    };
    format!("{scheme}://{authority_out}{path_and_rest}")
}

/// Split an authority (`host[:port]`, or bracketed IPv6 `host:port`) into its
/// host and port components.
fn split_authority(authority: &str) -> (String, String) {
    if authority.starts_with('[') {
        match authority.find(']') {
            Some(close) => {
                let host = authority[1..=close].to_string();
                let after = authority[close + 1..].trim_start_matches(':');
                (
                    host,
                    if after.is_empty() {
                        String::new()
                    } else {
                        after.to_string()
                    },
                )
            }
            None => (authority.to_string(), String::new()),
        }
    } else {
        match authority.rfind(':') {
            Some(index) => {
                let host = authority[..index].to_string();
                (host, authority[index + 1..].to_string())
            }
            None => (authority.to_string(), String::new()),
        }
    }
}

/// Build the SHA-256 fallback key from the normalized title, publication time
/// and a bounded content prefix. Null-byte delimiters separate the fields so
/// the hash is stable regardless of content that follows the delimiter.
fn hash_fallback(entry: &ParsedEntry) -> String {
    let mut hasher = Sha256::new();
    hasher.update(entry.title.as_deref().unwrap_or("").trim().as_bytes());
    hasher.update(b"\x00");
    match entry.published_at {
        Some(timestamp) => hasher.update(timestamp.to_string().as_bytes()),
        None => hasher.update(b"\x00"),
    }
    hasher.update(b"\x00");
    if let Some(content) = &entry.content {
        let end = content
            .char_indices()
            .nth(MAX_ENTRY_CONTENT_PREFIX)
            .map_or(content.len(), |(index, _)| index);
        hasher.update(&content.as_bytes()[..end]);
    } else {
        hasher.update(b"\x00");
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in hasher.finalize() {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

impl From<feed_rs::parser::ParseFeedError> for Error {
    fn from(error: feed_rs::parser::ParseFeedError) -> Self {
        Self::Message(format!("failed to parse feed: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/">
  <channel>
    <title>Example RSS Feed</title>
    <link>https://example.com</link>
    <description>An example channel</description>
    <lastBuildDate>Tue, 01 Jan 2024 00:00:00 +0000</lastBuildDate>
    <item>
      <title>First Post</title>
      <link>https://example.com/posts/1</link>
      <guid isPermaLink="true">urn:uuid:11111111-1111-1111-1111-111111111111</guid>
      <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
      <author>author@example.com</author>
      <description>Short summary of the post</description>
      <content:encoded><![CDATA[<p>Full <strong>content</strong> here.</p>]]></content:encoded>
    </item>
  </channel>
</rss>"#;

    const ATOM_FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Example Atom Feed</title>
  <updated>2024-03-02T10:00:00Z</updated>
  <id>urn:uuid:feed-0000-0000-0000-000000000000</id>
  <entry>
    <id>urn:uuid:entry-with-id</id>
    <title type="html">Entry With Id</title>
    <link href="https://example.org/blog/2024/02/hello" rel="alternate" type="text/html"/>
    <link href="https://example.org/feed/atom" rel="self" type="application/atom+xml"/>
    <author><name>Jane Doe</name></author>
    <published>2024-02-01T09:00:00Z</published>
    <updated>2024-02-01T10:00:00Z</updated>
    <summary type="html">An abstract</summary>
    <content type="html">&lt;p&gt;The full entry body.&lt;/p&gt;</content>
  </entry>
  <entry>
    <title type="html">Entry With Link Only</title>
    <link href="https://example.org/blog/2024/03/world" rel="alternate" type="text/html"/>
    <author><name>John Smith</name></author>
    <published>2024-03-01T08:00:00Z</published>
    <content type="html">&lt;p&gt;No id here.&lt;/p&gt;</content>
  </entry>
  <entry>
    <title type="html">Entry With Nothing</title>
    <author><name>Nobody</name></author>
    <content type="text">Plain text body without any id or link.</content>
  </entry>
</feed>"#;

    #[test]
    fn parses_rss_channel_and_entry_fields() {
        let feed = parse_feed(RSS_FIXTURE.as_bytes()).expect("parse RSS");
        assert_eq!(feed.url.len(), 32);
        assert_eq!(feed.title.as_deref(), Some("Example RSS Feed"));
        assert_eq!(feed.links, vec!["https://example.com/".to_string()]);
        assert_eq!(feed.updated_at, Some(1704067200));
        assert_eq!(feed.entries.len(), 1);

        let entry = &feed.entries[0];
        assert_eq!(entry.id, "urn:uuid:11111111-1111-1111-1111-111111111111");
        assert_eq!(entry.title.as_deref(), Some("First Post"));
        assert_eq!(entry.url.as_deref(), Some("https://example.com/posts/1"));
        assert!(entry
            .authors
            .iter()
            .any(|author| author.contains("example.com")));
        assert_eq!(
            entry.content.as_deref(),
            Some("<p>Full <strong>content</strong> here.</p>")
        );
        assert_eq!(entry.summary.as_deref(), Some("Short summary of the post"));
        assert_eq!(entry.published_at, Some(1704110400));
    }

    #[test]
    fn parses_atom_entry_fields() {
        let feed = parse_feed(ATOM_FIXTURE.as_bytes()).expect("parse Atom");
        assert_eq!(feed.title.as_deref(), Some("Example Atom Feed"));
        assert_eq!(feed.updated_at, Some(1709373600));
        assert_eq!(feed.entries.len(), 3);

        let with_id = &feed.entries[0];
        assert_eq!(with_id.id, "urn:uuid:entry-with-id");
        // Best link is the rel="alternate" HTML link, not the rel="self" one.
        assert_eq!(
            with_id.url.as_deref(),
            Some("https://example.org/blog/2024/02/hello")
        );
        assert_eq!(with_id.authors, vec!["Jane Doe".to_string()]);
        assert_eq!(with_id.published_at, Some(1706778000));
        assert_eq!(with_id.updated_at, Some(1706781600));
        assert_eq!(
            with_id.content.as_deref(),
            Some("<p>The full entry body.</p>")
        );
        assert_eq!(with_id.summary.as_deref(), Some("An abstract"));
    }

    #[test]
    fn dedupe_key_uses_guid_for_rss() {
        let entry = &parse_feed(RSS_FIXTURE.as_bytes()).unwrap().entries[0];
        assert_eq!(
            entry.dedupe_key,
            "g:urn:uuid:11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn dedupe_key_uses_canonical_url_when_no_guid() {
        let entry = &parse_feed(ATOM_FIXTURE.as_bytes()).unwrap().entries[1];
        assert!(
            entry.id.is_empty(),
            "unexpected generated id: {:?}",
            entry.id
        );
        assert!(entry
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://example.org/blog/2024/03/world")));
        assert!(entry.dedupe_key.starts_with("u:"));
        assert_eq!(
            entry.dedupe_key,
            format!("u:{}", entry.url.as_ref().unwrap())
        );
    }

    #[test]
    fn dedupe_key_sha256_fallback_without_id_or_link() {
        let entry = &parse_feed(ATOM_FIXTURE.as_bytes()).unwrap().entries[2];
        assert!(
            entry.id.is_empty(),
            "unexpected generated id: {:?}",
            entry.id
        );
        assert!(entry.url.is_none());
        assert!(entry.dedupe_key.starts_with("h:"));
        assert_eq!(entry.dedupe_key.len(), "h:".len() + 64);
    }

    #[test]
    fn dedupe_key_is_stable_across_identical_and_equivalent_feeds() {
        // Parsing the same bytes twice yields identical keys.
        let a = parse_feed(RSS_FIXTURE.as_bytes()).unwrap();
        let b = parse_feed(RSS_FIXTURE.as_bytes()).unwrap();
        for parsed in [&a, &b] {
            for entry in &parsed.entries {
                assert_eq!(ParsedEntry::dedupe_key(entry), entry.dedupe_key);
            }
        }
        // Two feeds that differ only in formatting produce the same keys.
        let reformatted_xml = RSS_FIXTURE.replace("</item>", "\n</item>\n");
        let reformatted = parse_feed(reformatted_xml.as_bytes()).unwrap();
        assert_eq!(reformatted.entries[0].dedupe_key, a.entries[0].dedupe_key);
    }

    #[test]
    fn url_normalization_lowercases_and_strips_default_port_and_fragment() {
        assert_eq!(
            normalize_url("HTTP://Example.COM:443/Path?a=1&b=2#frag"),
            "http://example.com:443/Path?a=1&b=2"
        );
        assert_eq!(
            normalize_url("http://example.com:80/a/b"),
            "http://example.com/a/b"
        );
        assert_eq!(
            normalize_url("https://example.com:8080/x?y=2#z"),
            "https://example.com:8080/x?y=2"
        );
        assert_eq!(
            normalize_url("https://example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn url_normalization_keeps_query_and_relative_references() {
        assert_eq!(
            normalize_url("https://example.com/a#frag"),
            "https://example.com/a"
        );
        assert_eq!(normalize_url("/relative/path?q=1"), "/relative/path?q=1");
    }

    #[test]
    fn url_normalization_handles_ipv6_authority() {
        assert_eq!(
            normalize_url("[2001:db8::1]:80/ipv6"),
            "http://[2001:db8::1]/ipv6"
        );
        assert_eq!(
            normalize_url("http://[2001:db8::1]:443/ipv6?x=1#f"),
            "http://[2001:db8::1]:443/ipv6?x=1"
        );
    }

    #[test]
    fn parse_failure_returns_error_without_panic() {
        let result = parse_feed(b"<not-a-feed></not-a-feed>");
        assert!(result.is_err());
    }
}
