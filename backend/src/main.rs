mod article;
mod doctor;
mod feed;
mod fixture;
mod http;
mod http_probe;
mod probe;
mod refresh;
mod store;
mod version;

use std::fmt;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
    Image(image::ImageError),
    Message(String),
}

impl Error {
    fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => error.fmt(formatter),
            Self::Sqlite(error) => error.fmt(formatter),
            Self::Image(error) => error.fmt(formatter),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<image::ImageError> for Error {
    fn from(error: image::ImageError) -> Self {
        Self::Image(error)
    }
}

fn usage() -> &'static str {
    "usage:\n  rss-backend --version\n  rss-backend doctor\n  rss-backend http-probe --url HTTPS_URL\n  rss-backend init-probe-db --db PATH\n  rss-backend materialize-fixture --out PATH\n  rss-backend --db PATH feed add URL\n  rss-backend --db PATH refresh --feed ID [--budget SEC]"
}

fn value_argument(arguments: &[String], flag: &str) -> Result<String, Error> {
    if arguments.len() == 2 && arguments[0] == flag {
        Ok(arguments[1].clone())
    } else {
        Err(Error::message(usage()))
    }
}

fn path_argument(arguments: &[String], flag: &str) -> Result<PathBuf, Error> {
    Ok(PathBuf::from(value_argument(arguments, flag)?))
}

fn integer_argument(arguments: &[String], flag: &str, default: u64) -> Result<u64, Error> {
    match arguments.iter().position(|argument| argument == flag) {
        None => Ok(default),
        Some(index) => arguments
            .get(index + 1)
            .ok_or_else(|| Error::message(usage()))?
            .parse()
            .map_err(|_| Error::message(format!("invalid value for {flag}"))),
    }
}

fn run(arguments: &[String]) -> Result<(), Error> {
    if arguments.first().map(String::as_str) == Some("--db") {
        let db = path_argument(&arguments[..2], "--db")?;
        let command = arguments.get(2).map(String::as_str);
        let mut store = store::Store::open(&db)?;
        match command {
            Some("feed")
                if arguments.get(3).map(String::as_str) == Some("add") && arguments.len() == 5 =>
            {
                let id = store.add_feed(&arguments[4], unix_now())?;
                println!("{id}");
            }
            Some("refresh") => {
                let feed_id = integer_argument(&arguments[3..], "--feed", 0)?;
                if feed_id == 0 {
                    return Err(Error::message("refresh requires --feed ID"));
                }
                let budget = integer_argument(&arguments[3..], "--budget", 240)?;
                let _ = refresh::run(&mut store, feed_id as i64, budget)?;
            }
            _ => return Err(Error::message(usage())),
        }
        return Ok(());
    }
    match arguments.first().map(String::as_str) {
        Some("--version") if arguments.len() == 1 => println!("{}", version::banner()),
        Some("doctor") if arguments.len() == 1 => {
            if !doctor::run() {
                return Err(Error::message("doctor critical checks failed"));
            }
        }
        Some("http-probe") => {
            let url = value_argument(&arguments[1..], "--url")?;
            let status = http_probe::run(&url)?;
            println!("https ok status={status} url={url}");
        }
        Some("init-probe-db") => {
            let path = probe::initialize(&path_argument(&arguments[1..], "--db")?)?;
            if !probe::probe_present(&path)? || probe::journal_mode(&path)? != "delete" {
                return Err(Error::message("probe database validation failed"));
            }
            println!("{}", path.display());
        }
        Some("materialize-fixture") => {
            let path = fixture::materialize(&path_argument(&arguments[1..], "--out")?)?;
            println!("{}", path.display());
        }
        _ => return Err(Error::message(usage())),
    }
    Ok(())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rss-backend: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_command() {
        assert!(run(&["refresh".to_owned()]).is_err());
    }

    #[test]
    fn requires_exact_path_flag() {
        assert!(path_argument(&[], "--db").is_err());
        assert!(path_argument(&["--wrong".to_owned(), "x".to_owned()], "--db").is_err());
    }
}
