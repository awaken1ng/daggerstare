use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Barrier, RwLock},
    thread,
    time::{Duration, Instant},
};

use clap::Parser;
use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;

#[derive(Parser, Debug)]
struct Args {
    /// Command to run.
    cmd: Vec<String>,

    /// File to watch for changes.
    #[arg(short, long)]
    watch: PathBuf,

    /// Milliseconds to wait before killing the running command.
    #[arg(short, long)]
    timeout: u128,
}

fn main() {
    let args = Args::parse();

    let barrier = Arc::new(Barrier::new(2));
    let last_event = Arc::new(RwLock::new(None));

    // keep debouncer around to not be dropped immediately
    let _debouncer = {
        println!("👀🔪: Setting up watcher");
        let (tx, rx) = crossbeam_channel::unbounded();

        let mut debouncer = new_debouncer(Duration::from_millis(50), None, tx).unwrap();
        let watcher = debouncer.watcher();
        watcher
            .watch(&args.watch, RecursiveMode::NonRecursive)
            .unwrap();

        let barrier = barrier.clone();
        let last_event = last_event.clone();

        thread::spawn(move || {
            println!("👀🔪: Watching");
            barrier.wait();

            for _ in rx {
                let mut lock = last_event.write().unwrap();

                if lock.is_none() {
                    println!("👀🔪: Starting the timer");
                }

                let now = Instant::now();
                *lock = Some(now);
            }
        });

        debouncer
    };

    // ensure that watcher is ready
    barrier.wait();

    println!("👀🔪: Starting command");
    let mut cmd = {
        let (program, args) = args.cmd.split_first().unwrap();
        Command::new(program).args(args).spawn().unwrap()
    };

    thread::spawn(move || loop {
        let lock = last_event.read().unwrap();

        if let Some(last) = *lock {
            let now = Instant::now();
            let passed = now.duration_since(last).as_millis();

            if passed >= args.timeout {
                println!("👀🔪: Killing process {}", cmd.id());
                cmd.kill().unwrap();
                break;
            }
        }

        drop(lock); // deadlocks otherwise

        thread::sleep(Duration::from_millis(50));
    })
    .join()
    .unwrap();
}
