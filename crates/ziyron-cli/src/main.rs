//! `ziyron`: download and stream torrents from the command line.

use std::fs::{self, File};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use ziyron_engine::server::EngineServer;

const POLL: Duration = Duration::from_millis(500);
const STREAM_CACHE_BYTES: i64 = 256 << 20;
/// Engine state stays out of the way of the downloaded files.
const STATE_DIR: &str = ".ziyron";

#[derive(Parser)]
#[command(name = "ziyron", version, about = "Download and stream torrents.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download a torrent's files, then exit.
    Download {
        #[command(flatten)]
        engine: EngineArgs,
        /// File index to download; repeat for several. Default: all files.
        #[arg(long = "file")]
        files: Vec<usize>,
    },
    /// Serve one file over HTTP for a video player until Ctrl+C.
    Stream {
        #[command(flatten)]
        engine: EngineArgs,
        /// File index to stream. Default: the largest file.
        #[arg(long)]
        file: Option<usize>,
    },
    /// Show what is inside a .torrent file, without using the network.
    Info {
        /// Path to a .torrent file.
        torrent: PathBuf,
    },
}

#[derive(Args)]
struct EngineArgs {
    /// Magnet link, info hash, or path to a .torrent file.
    source: String,
    /// Directory for downloaded files.
    #[arg(short, long, default_value = ".")]
    output: PathBuf,
    /// Maximum connected peers.
    #[arg(long, default_value_t = 55)]
    max_peers: usize,
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Download { engine, files } => download(&engine, &files),
        Command::Stream { engine, file } => stream(&engine, file),
        Command::Info { torrent } => info(&torrent),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn info(path: &Path) -> Result<(), String> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let meta = ziyron_meta::parse_metainfo(&data).map_err(|e| e.to_string())?;
    let info = ziyron_meta::parse_info(&meta.info_bytes).map_err(|e| e.to_string())?;
    println!("name:      {}", info.name);
    println!("info hash: {}", ziyron_meta::info_hash_v1(&meta.info_bytes).to_hex());
    println!("piece:     {} bytes", info.piece_length);
    Ok(())
}

fn download(args: &EngineArgs, files: &[usize]) -> Result<(), String> {
    let session = Session::start(args, stop_flag()?)?;
    let status = session.wait_for_metadata()?;
    let all = file_list(&status)?;
    let selected: Vec<usize> = if files.is_empty() {
        all.iter().map(|f| f.index).collect()
    } else {
        files.to_vec()
    };

    // The engine duplicates each descriptor, and these handles stay open with it.
    let mut outputs = Vec::new();
    let mut descriptors = vec![Value::Null; all.len()];
    for file in &all {
        if selected.contains(&file.index) {
            let opened = open_output(&args.output, &file.path)?;
            descriptors[file.index] = json!(opened.as_raw_fd());
            outputs.push(opened);
        }
    }
    session.call("POST", "/configure", &json!({
        "id": session.id,
        "descriptors": descriptors,
        "selected": selected,
        "storageMode": "download",
    }))?;

    loop {
        session.check_stop()?;
        let status = session.status()?;
        if let Some(error) = status["error"].as_str().filter(|e| !e.is_empty()) {
            return Err(error.to_string());
        }
        print_progress(&status);
        if status["done"].as_bool() == Some(true) {
            eprintln!();
            println!("Saved to {}", args.output.display());
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
}

fn stream(args: &EngineArgs, file: Option<usize>) -> Result<(), String> {
    let session = Session::start(args, stop_flag()?)?;
    let status = session.wait_for_metadata()?;
    let all = file_list(&status)?;
    let chosen = match file {
        Some(index) => index,
        None => all.iter().max_by_key(|f| f.length).map(|f| f.index).ok_or("torrent has no files")?,
    };
    session.call("POST", "/configure", &json!({
        "id": session.id,
        "descriptors": vec![Value::Null; all.len()],
        "selected": [chosen],
        "storageMode": "stream",
        "streamCacheBytes": STREAM_CACHE_BYTES,
    }))?;
    let play = session.call("POST", "/play", &json!({
        "id": session.id,
        "fileIndex": chosen,
    }))?;
    println!("{}", play["name"].as_str().unwrap_or(""));
    println!("{}", play["streamUrl"].as_str().ok_or("engine returned no stream URL")?);
    eprintln!("Open the URL in a video player. Press Ctrl+C to stop.");
    loop {
        if session.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
}

fn stop_flag() -> Result<Arc<AtomicBool>, String> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)).map_err(|e| e.to_string())?;
    Ok(stop)
}

/// Runs the engine for one torrent and shuts it down when dropped.
struct Session {
    server: Arc<EngineServer>,
    id: String,
    stop: Arc<AtomicBool>,
    _runtime: tokio::runtime::Runtime,
}

impl Session {
    fn start(args: &EngineArgs, stop: Arc<AtomicBool>) -> Result<Self, String> {
        let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let _guard = runtime.enter();
        let state = args.output.join(STATE_DIR);
        let server = EngineServer::new(0, &state, args.max_peers).map_err(|e| e.to_string())?;
        runtime.block_on(server.start_streaming()).map_err(|e| e.to_string())?;
        let mut session = Self { server, id: String::new(), stop, _runtime: runtime };
        let added = session.call("POST", "/add", &add_body(&args.source)?)?;
        session.id = added["id"].as_str().ok_or("engine returned no torrent id")?.to_string();
        Ok(session)
    }

    fn call(&self, method: &str, path: &str, body: &Value) -> Result<Value, String> {
        let (code, bytes) = self.server.request(method, path, &body.to_string());
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        if code == 200 {
            return Ok(value);
        }
        Err(value["error"].as_str().map_or_else(|| format!("engine returned {code}"), str::to_string))
    }

    fn status(&self) -> Result<Value, String> {
        self.call("GET", &format!("/torrent/{}", self.id), &Value::Null)
    }

    fn check_stop(&self) -> Result<(), String> {
        if self.stop.load(Ordering::SeqCst) {
            return Err("stopped; run the same command again to resume".into());
        }
        Ok(())
    }

    fn wait_for_metadata(&self) -> Result<Value, String> {
        eprintln!("Finding peers and fetching metadata...");
        loop {
            self.check_stop()?;
            let status = self.status()?;
            if status["ready"].as_bool() == Some(true) {
                return Ok(status);
            }
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.server.close();
    }
}

fn add_body(source: &str) -> Result<Value, String> {
    if source.starts_with("magnet:") || is_info_hash(source) {
        return Ok(json!({ "torrentId": source }));
    }
    let data = fs::read(source).map_err(|e| format!("{source}: {e}"))?;
    Ok(json!({
        "torrentId": source,
        "torrentData": base64::engine::general_purpose::STANDARD.encode(data),
    }))
}

fn is_info_hash(source: &str) -> bool {
    source.len() == 40 && source.bytes().all(|b| b.is_ascii_hexdigit())
}

struct TorrentFile {
    index: usize,
    path: String,
    length: i64,
}

fn file_list(status: &Value) -> Result<Vec<TorrentFile>, String> {
    let files = status["files"].as_array().ok_or("engine returned no file list")?;
    files.iter().map(|file| Ok(TorrentFile {
        index: file["index"].as_u64().ok_or("file without an index")? as usize,
        path: file["path"].as_str().ok_or("file without a path")?.to_string(),
        length: file["length"].as_i64().unwrap_or(0),
    })).collect()
}

fn open_output(root: &Path, torrent_path: &str) -> Result<File, String> {
    let path = safe_join(root, torrent_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    File::options().read(true).write(true).create(true).truncate(false).open(&path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// File names come from strangers: a torrent must not write outside `root`.
fn safe_join(root: &Path, torrent_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(torrent_path);
    let safe = relative.components().all(|part| matches!(part, Component::Normal(_)));
    if !safe || torrent_path.is_empty() {
        return Err(format!("unsafe file path in torrent: {torrent_path}"));
    }
    Ok(root.join(relative))
}

fn print_progress(status: &Value) {
    let progress = status["progress"].as_f64().unwrap_or(0.0) * 100.0;
    let speed = status["downloadSpeed"].as_i64().unwrap_or(0) as f64 / (1024.0 * 1024.0);
    let peers = status["numPeers"].as_i64().unwrap_or(0);
    eprint!("\r{progress:6.2}%  {speed:7.2} MiB/s  {peers:3} peers ");
    let _ = std::io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rejects_paths_that_leave_the_output_directory() {
        let root = Path::new("/downloads");
        assert_eq!(safe_join(root, "show/a.mp4").unwrap(), root.join("show/a.mp4"));
        for bad in ["../a", "show/../../a", "/etc/passwd", ""] {
            assert!(safe_join(root, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn info_hash_needs_40_hex_digits() {
        assert!(is_info_hash("08ada5a7a6183aae1e09d831df6748d566095a10"));
        assert!(!is_info_hash("sintel.torrent"));
    }
}
