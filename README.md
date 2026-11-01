# Ziyron

**A torrent engine in Rust, built so that video seeks fast.**

The name is Z + iron, the metal that rusts. Say it "ZY-ron".

Ziyron downloads and streams at the same time. Point any video player at its
HTTP stream, jump anywhere in the file, and the engine moves its peers to that
spot. It runs on Android today and was built to power torrent players,
downloaders and streaming servers.

> **Status: early.** Extracted from the Torrent Player Android app. Expect the
> API to change.

## Quick start

Needs Linux and Rust 1.85 or newer.

```sh
cargo build --release -p ziyron-cli

# Download every file, or pick some with --file
./target/release/ziyron download "magnet:?xt=urn:btih:..." -o ./downloads

# Stream the largest file, then open the printed URL in a video player
./target/release/ziyron stream movie.torrent

# Look inside a .torrent file
./target/release/ziyron info movie.torrent
```

## Why Ziyron

These are design choices. For measured results, including where Ziyron is
behind, see [Measured results](#measured-results).

**Seeking comes first.** Most of a seek's wait is peers holding blocks, so the
scheduler is built around that wait:

- The 2 MiB ahead of the playhead are served before anything else.
- The first 8 blocks at the playhead go to the quickest peer, not the first to ask.
- Right after a seek, the first 1 MiB gets a second copy from another peer.
- A playhead block is overdue after 150 ms or two of its holder's round trips,
  whichever is longer. It then gets a second copy or moves to a quicker peer.
- Playback and saved downloads share one block table and one picker.

**It reads the video.** Ziyron finds keyframe positions in MP4 `moov` and
Matroska `Cues` indexes, since players seek to keyframes.

**A fast path for uTP.** On Linux and Android, it reads up to 32 datagrams in one
`recvmmsg` call into a shared buffer, so a packet costs neither an allocation
nor a copy. Receiving has its own thread, kept on the fast cores only while a
stream is open.

**It survives network changes.** It detects a route change, such as Wi-Fi to
mobile data, drops the stale tracker connections and retries recovery.

**Real streaming.** It serves HTTP Range requests, so any player that can open
a URL can use it.

## Measured results

### Against other engines

The Sintel torrent (129 MiB) was downloaded on one Linux machine with
[tools/bench/compare.py](tools/bench/compare.py). This is **one round on a public
swarm**, so the times are not a verdict.

| Engine | Version | Time (s) | CPU time (s) | Peak RAM (MiB) |
|---|---|---:|---:|---:|
| **Ziyron** | 0.1.0 | **25.6** | 2.51 | 374 |
| rqbit | 9.1.0 | 27.5 | 1.89 | 35 |
| WebTorrent | 5.1.3 | 35.0 | 15.19 | 238 |
| aria2 | 1.37.0 | 42.1 | **1.34** | **22** |
| libtorrent | 2.1.1.0 | 63.3 | 1.87 | 47 |

Six more Ziyron runs took 23-50 s, used 2.35-2.99 s of CPU and peaked at
301-388 MiB.

**Ziyron's weak spot is memory.** It peaked at about ten times rqbit's RAM and
used more CPU than every engine here except WebTorrent. The cause of the high
RAM is not known yet. Lowering the saved-download cache minimum from 256 MiB to
16 MiB made no clear difference, and a figure excluding cache has not been
measured. Peak RAM here is the whole process.

### Seeking

| What | Result | Conditions |
|---|---|---|
| Seek to data already cached, 2 MiB ready | 16-28 ms | One Android phone |
| Seek over the network, 2 MiB ready | 1060 ms median in the best run; 1080-1467 ms in others | One Android phone, real swarm |
| Seek over the network, first bytes | 292 ms median in the best run | Same |
| 1 MiB from the middle of Sintel with `ziyron stream` | About 1 s | One request, Linux, real swarm |

Seek medians for the same scheduler vary by at least 25% between runs. Other
engines' seek times have not been measured, so **Ziyron cannot yet be called
faster at seeking than they are**.

### Against the old Go engine

- **Local transfer:** about 423 MiB/s against 296 MiB/s, using `ziyron-parity`.
- **Storage:** Ziyron is not ahead everywhere. On a noisy host, sequential write
  had a median of 121 MiB/s against 512 MiB/s. See
  [BENCH.md](crates/ziyron-storage/BENCH.md).

## How it compares

| Engine | Language | Licence | Choose it when |
|---|---|---|---|
| **Ziyron** | Rust | MPL-2.0 | You are building a player where seeking matters most |
| [libtorrent](https://github.com/arvidn/libtorrent) | C++ | BSD-3-Clause | You need a mature engine with a long track record |
| [rqbit](https://github.com/ikatson/rqbit) | Rust | Apache-2.0 | You want an established Rust client and library today |
| [anacrolix/torrent](https://github.com/anacrolix/torrent) | Go | MPL-2.0 | You are building in Go |
| [WebTorrent](https://github.com/webtorrent/webtorrent) | JavaScript | MIT | You need torrents inside a browser |

## Protocol support

Ziyron's column comes from its source code. The others are from general
knowledge of each project and were not tested here; `?` means not confirmed.
Please open an issue if one is wrong.

| Feature | Ziyron | libtorrent | rqbit | aria2 | WebTorrent |
|---|---|---|---|---|---|
| BitTorrent v1 | Yes | Yes | Yes | Yes | Yes |
| BitTorrent v2 (BEP 52) | Yes | Yes | ? | No | ? |
| Magnet links | Yes | Yes | Yes | Yes | Yes |
| TCP peers | Yes | Yes | Yes | Yes | Yes |
| uTP peers | Yes | Yes | ? | No | ? |
| Encrypted connections (MSE) | Yes | Yes | ? | Yes | ? |
| Hole punching | Yes | Yes | ? | ? | ? |
| WebRTC peers (WebTorrent) | Yes | ? | ? | No | Yes |
| HTTP and UDP trackers | Yes | Yes | Yes | Yes | Yes |
| WebSocket trackers | Yes | ? | ? | No | Yes |
| DHT | Yes | Yes | Yes | Yes | Yes |
| Peer exchange | Yes | Yes | Yes | Yes | Yes |
| Local peer discovery | Yes | Yes | ? | Yes | ? |
| Web seeds | Yes | Yes | ? | Yes | Yes |
| IPv6 | Yes | Yes | ? | Yes | ? |
| HTTP streaming with seeking | Yes | No (library has no HTTP server) | Yes | No | Yes |

## Crates

| Crate | Job |
|---|---|
| `ziyron-types` | Shared types and storage traits |
| `ziyron-meta` | Torrent files, magnet links, bencode |
| `ziyron-wire` | Peer protocol, uTP, encryption |
| `ziyron-discovery` | Trackers, DHT, local discovery, web seeds, WebTorrent |
| `ziyron-core` | Client, peers and the block scheduler |
| `ziyron-storage` | Disk storage, caches, piece verification |
| `ziyron-policy` | Shared engine policies |
| `ziyron-engine` | Engine server, HTTP streaming, `engine-host` control server |
| `ziyron-android` | Android JNI bridge, built as `libengine.so` |
| `ziyron-cli` | The `ziyron` command-line tool |
| `ziyron-parity` | Performance benchmark |

## Add Ziyron to your app

Ziyron runs on Linux and Android. There are three ways to use it.

### From any language: HTTP API

Start the local control server, then send it JSON:

```sh
cargo build --release -p ziyron-engine --bin engine-host
./target/release/engine-host --port 18080 --path ./downloads

# Add a torrent. The reply contains its id
curl -X POST localhost:18080/add -d '{"torrentId":"magnet:?xt=urn:btih:..."}'

# Status, including the file list once metadata arrives
curl localhost:18080/torrent/ID

# Choose files. "descriptors" needs one entry per file in the torrent
curl -X POST localhost:18080/configure \
  -d '{"id":"ID","descriptors":[null],"selected":[0],"storageMode":"stream"}'

# Get a URL any video player can open
curl -X POST localhost:18080/play -d '{"id":"ID","fileIndex":0}'
```

Use `"storageMode":"download"` to save files. With `null` descriptors, the engine
keeps data in its own files under `--path`. Other routes include `/pause`,
`/resume`, `/remove`, `/stats` and `/settings`.

### From Rust

```toml
[dependencies]
ziyron-engine = { git = "https://github.com/karmugilen/ziyron" }
```

Call the same routes in-process with `EngineServer::request`, inside a Tokio
runtime. [crates/ziyron-cli/src/main.rs](crates/ziyron-cli/src/main.rs) is a
complete, short example. A simpler typed API is on the roadmap.

### From Android

Build `ziyron-android` into `libengine.so`, as described in
[docs/ANDROID_BUILD.md](docs/ANDROID_BUILD.md). Its JNI names currently expect
the class `webtor.app.EngineHost`.

## Roadmap

- Find and reduce the high peak RAM, then lower CPU use
- More benchmark rounds, including seek times against other engines
- A simple typed Rust API
- The full test suite in CI
- A stable public Rust API
- macOS and Windows support: the engine currently uses Linux-only calls

## Licence

[MPL-2.0](LICENSE), because Ziyron derives from anacrolix/torrent. See
[NOTICE.md](NOTICE.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

Use Ziyron only with content you have the right to download and share.
