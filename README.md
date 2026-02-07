# lastmod-rs

A single-purpose CLI tool that finds the most recent modification timestamp in a directory tree. Built in Rust for extreme performance on large codebases and filesystems with millions of files.

```
$ lastmod-rs /var/log
2025-07-14 03:22:17
```

## Why not `find` or `fd`?

The traditional approach to finding the newest file's mtime involves piping multiple commands together:

```bash
# GNU find + sort (most common)
find /path -type f -printf '%T@\n' | sort -nr | head -1

# fd + stat + sort
fd -t f /path -x stat --format '%Y' | sort -nr | head -1
```

These work, but they have fundamental inefficiencies that compound as directory size grows:

| Problem | Shell pipeline | lastmod-rs |
|---|---|---|
| **Memory** | `sort` must hold every timestamp in memory before producing output | Holds a single 8-byte atomic integer, regardless of file count |
| **Process overhead** | 3-4 separate processes (`find`, `stat`, `sort`, `head`) communicating via pipes | Single process, zero IPC |
| **Parallelism** | Sequential traversal through a single `find` process | Multi-threaded directory reading via work-stealing thread pool |
| **Wasted work** | `sort` orders the *entire* list just to extract one value | Compares each timestamp once with an atomic `fetch_max` |
| **stat calls** | Often calls `stat` in a separate process per file (or per batch via `xargs`) | Reads metadata inline during traversal, no extra process spawning |

### What this means in practice

On a directory tree with 1 million files, the shell pipeline must:

1. Traverse every entry (single-threaded)
2. Spawn `stat` or format timestamps for each entry
3. Pipe all 1M lines into `sort`
4. `sort` allocates memory for 1M entries, runs O(n log n) comparison sort
5. `head` reads one line and discards the rest

`lastmod-rs` does:

1. Traverse entries in parallel across all available cores
2. Read metadata inline (one `fstat` syscall per entry, no extra process)
3. Update a single `AtomicU64` via `fetch_max` -- O(1) per entry, O(n) total
4. Print the result

Total memory used: **8 bytes** of mutable state, regardless of tree size.

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
git clone https://github.com/danielbemvindo/lastmod-rs.git
cd lastmod-rs
cargo build --release
# Binary is at ./target/release/lastmod-rs
```

## Usage

```
lastmod-rs [OPTIONS] [PATH]
```

When run without arguments, scans the current directory.

### Options

| Flag | Short | Description |
|---|---|---|
| `--hidden` | `-H` | Include hidden files and directories (excluded by default) |
| `--no-ignore` | `-I` | Don't respect `.gitignore` rules |
| `--follow-links` | `-L` | Follow symbolic links |
| `--max-depth <N>` | `-d` | Limit traversal to N levels deep |
| `--help` | `-h` | Print help |

### Examples

```bash
# Scan current directory
lastmod-rs

# Scan a specific path
lastmod-rs /var/log

# Include hidden files (dotfiles, .git, etc.)
lastmod-rs -H ~/projects

# Include everything, ignore .gitignore rules
lastmod-rs -HI ~/projects

# Limit depth to 2 levels
lastmod-rs -d 2 /usr

# Follow symlinks
lastmod-rs -L /opt
```

### Output

A single line to stdout in ISO 8601 format:

```
YYYY-MM-DD HH:MM:SS
```

The timestamp is in local time. If no files are found, prints an error to stderr and exits with code 1.

## Design

### Parallel traversal

Directory reading is parallelized using the [`ignore`](https://crates.io/crates/ignore) crate (from the ripgrep project). It uses a work-stealing thread pool where each thread maintains a local deque of directories to process. When a thread's queue empties, it steals work from other threads. This keeps all cores busy even when the directory tree is unbalanced.

### Atomic maximum tracking

Instead of collecting file timestamps into a list and sorting, `lastmod-rs` maintains a single `AtomicU64` storing the highest modification time seen so far (as nanoseconds since the Unix epoch). Each worker thread updates it with `fetch_max` using relaxed memory ordering -- the cheapest atomic operation available, with no memory barriers.

### Metadata-only

The tool only calls `metadata()` on each entry to read the modification time. It never opens or reads file contents.

### Error resilience

Permission errors and inaccessible entries are silently skipped. The scan continues through the rest of the tree without interruption.

## License

MIT
