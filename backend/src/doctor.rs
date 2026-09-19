//! Small runtime diagnostic suitable for native, QEMU, and Kindle execution.

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::probe;

fn command_version(command: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(command).args(arguments).output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned()
    })
}

pub fn run() -> bool {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let database = std::env::temp_dir().join(format!(
        "rss-backend-doctor-{}-{stamp}.sqlite3",
        std::process::id()
    ));

    println!("rss-backend doctor");
    let sqlite_ok =
        match probe::initialize(&database).and_then(|_| probe::sqlite_version(&database)) {
            Ok(version) => {
                println!("[critical] sqlite: ok ({version})");
                true
            }
            Err(error) => {
                println!("[critical] sqlite: FAILED ({error})");
                false
            }
        };
    let _ = fs::remove_file(&database);

    for (name, command, args) in [
        ("rustc", "rustc", &["--version"][..]),
        ("qemu-arm", "qemu-arm", &["--version"][..]),
    ] {
        match command_version(command, args) {
            Some(version) => println!("[advisory] {name}: {version}"),
            None => println!("[advisory] {name}: unavailable"),
        }
    }
    println!("target_arch={}", std::env::consts::ARCH);
    if sqlite_ok {
        println!("doctor: critical checks passed");
    }
    sqlite_ok
}

#[cfg(test)]
mod tests {
    #[test]
    fn critical_doctor_check_passes() {
        assert!(super::run());
    }
}
