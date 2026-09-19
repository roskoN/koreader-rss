pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MILESTONE: &str = "milestone0";

pub fn banner() -> String {
    format!("rss-backend {VERSION} ({MILESTONE})")
}
