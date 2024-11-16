use notify_debouncer_full::{new_debouncer, notify::*, DebounceEventResult, DebouncedEvent};
use std::{sync::mpsc, time::Duration};

fn main() -> Result<()> {
    let (tx, rx) = mpsc::channel();

    // Select recommended watcher for debouncer.
    // Using a callback here, could also be a channel.
    let mut debouncer = new_debouncer(Duration::from_secs(2), None, tx)?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    debouncer.watch("./downloads", RecursiveMode::Recursive)?;

    for res in rx {
        match res {
            Ok(event) => println!("event: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }
    Ok(())
}
