use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use chrono::DateTime;
use clap::Parser;
use ignore::WalkBuilder;

/// Find the most recent modification date in a directory tree.
#[derive(Parser)]
#[command(name = "lastmod-rs")]
struct Cli {
    /// Directory to scan
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Include hidden files and directories
    #[arg(short = 'H', long)]
    hidden: bool,

    /// Don't respect .gitignore files
    #[arg(short = 'I', long)]
    no_ignore: bool,

    /// Follow symbolic links
    #[arg(short = 'L', long)]
    follow_links: bool,

    /// Maximum directory depth to traverse
    #[arg(short = 'd', long)]
    max_depth: Option<usize>,
}

fn main() {
    let cli = Cli::parse();

    let mut builder = WalkBuilder::new(&cli.path);
    builder
        .hidden(!cli.hidden)
        .git_ignore(!cli.no_ignore)
        .git_global(!cli.no_ignore)
        .git_exclude(!cli.no_ignore)
        .follow_links(cli.follow_links);

    if let Some(depth) = cli.max_depth {
        builder.max_depth(Some(depth));
    }

    let max_nanos = Arc::new(AtomicU64::new(0));

    builder.build_parallel().run(|| {
        let max_nanos = Arc::clone(&max_nanos);
        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => return ignore::WalkState::Continue,
            };

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => return ignore::WalkState::Continue,
            };

            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                    let nanos = duration.as_nanos() as u64;
                    max_nanos.fetch_max(nanos, Ordering::Relaxed);
                }
            }

            ignore::WalkState::Continue
        })
    });

    let nanos = max_nanos.load(Ordering::Relaxed);
    if nanos == 0 {
        eprintln!("No files found");
        std::process::exit(1);
    }

    let secs = (nanos / 1_000_000_000) as i64;
    let nsecs = (nanos % 1_000_000_000) as u32;
    let dt = DateTime::from_timestamp(secs, nsecs).expect("invalid timestamp");
    let local = dt.with_timezone(&chrono::Local);
    println!("{}", local.format("%Y-%m-%d %H:%M:%S"));
}
