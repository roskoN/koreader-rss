use crate::{refresh, store, Error};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const POWERD: &str = "com.lab126.powerd";

fn wait_for(event: &'static str, sender: mpsc::Sender<&'static str>) {
    loop {
        let result = Command::new("lipc-wait-event")
            .args([POWERD, event])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        if let Ok(output) = result {
            let text = String::from_utf8_lossy(&output.stdout);
            let relevant = event != "readyToSuspend"
                || text.lines().any(|line| line.contains("readyToSuspend 1"));
            if output.status.success() && relevant {
                let _ = sender.send(event);
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn set_property(property: &str, value: &str) -> Result<(), Error> {
    let status = Command::new("lipc-set-prop")
        .args(["-i", "--", POWERD, property, value])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::message(format!("lipc-set-prop {property} failed")))
    }
}

fn power_state() -> Result<String, Error> {
    let output = Command::new("lipc-get-prop")
        .args([POWERD, "state"])
        .output()?;
    if !output.status.success() {
        return Err(Error::message("lipc-get-prop powerd state failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_ascii_lowercase())
}

fn arm(database: &Path) -> Result<(), Error> {
    let mut store = store::Store::open(database)?;
    let now = crate::unix_now();
    let deadline = store.ensure_next_refresh_at(now)?;
    let seconds = deadline.saturating_sub(now).max(1);
    set_property("rtcWakeup", &seconds.to_string())?;
    Ok(())
}

fn refresh_if_due(database: &Path, budget: u64) -> Result<(), Error> {
    let mut store = store::Store::open(database)?;
    let now = crate::unix_now();
    if !store.refresh_due(now)? {
        return Ok(());
    }
    let _ = set_property("deferSuspend", "1");
    let result = match refresh::RefreshLock::acquire(database) {
        Ok(_lock) => refresh::run_all_with_reason(&mut store, budget, store::RUN_REASON_WAKE),
        Err(Error::Message(_)) => Ok((0, 0)),
        Err(error) => Err(error),
    };
    let successful = result
        .as_ref()
        .map(|(_, failures)| *failures == 0)
        .unwrap_or(false);
    let schedule_result = store
        .schedule_after_wake(crate::unix_now(), successful)
        .map(|_| ());
    let clear_result = set_property("deferSuspend", "0");
    result.map(|_| ()).and(schedule_result).and(clear_result)
}

pub fn run(database: &Path, budget: u64, settle_s: u64) -> Result<(), Error> {
    let (sender, receiver) = mpsc::channel();
    for event in ["readyToSuspend", "wakeupFromSuspend"] {
        let sender = sender.clone();
        thread::spawn(move || wait_for(event, sender));
    }
    drop(sender);
    for event in receiver {
        match event {
            "readyToSuspend" => {
                let _ = arm(database);
            }
            "wakeupFromSuspend" => {
                thread::sleep(Duration::from_secs(settle_s));
                // A user wake before the persisted deadline is ignored. The
                // state check avoids doing work if powerd is not active.
                match power_state() {
                    Ok(state) if state == "active" => {
                        let _ = refresh_if_due(database, budget);
                    }
                    Ok(_) | Err(_) => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}
