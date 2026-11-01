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

| What | Result | Conditions |
|---|---|---|
| Seek to data already cached, 2 MiB ready | 16-28 ms | One Android phone |
| Seek over the network, 2 MiB ready | 1060 ms median in the best run; 1080-1467 ms in others | One Android phone, real swarm, not controlled |
| Seek over the network, first bytes | 292 ms median in the best run | Same |
| Whole Sintel torrent with `ziyron download`, 129 MiB video | Under 60 s, about 6 MiB/s near the end; all 987 pieces checked separately | One run on Linux, real swarm |
| Local transfer speed | About 423 MiB/s, against 296 MiB/s for the Go engine at the time | `ziyron-parity` on one machine |

Read these with care:

- Seek medians for the same scheduler vary by at least 25% between runs.
- Ziyron is not ahead everywhere. In a five-run storage benchmark on a noisy
  host, sequential write had a median of 121 MiB/s against 512 MiB/s for the Go
  engine. See [BENCH.md](crates/ziyron-storage/BENCH.md).
- **No head-to-head test has been run against libtorrent, rqbit or WebTorrent.**
  Ziyron should not be called faster than them until one has.

## How it compares

| Engine | Language | Licence | Choose it when |
|---|---|---|---|
| **Ziyron** | Rust | MPL-2.0 | You are building a player where seeking matters most |
| [libtorrent](https://github.com/arvidn/libtorrent) | C++ | BSD-3-Clause | You need a mature engine with a long track record |
| [rqbit](https://github.com/ikatson/rqbit) | Rust | Apache-2.0 | You want an established Rust client and library today |
| [anacrolix/torrent](https://github.com/anacrolix/torrent) | Go | MPL-2.0 | You are building in Go |
| [WebTorrent](https://github.com/webtorrent/webtorrent) | JavaScript | MIT | You need torrents inside a browser |

## Protocol support

- **Peers:** BitTorrent v1 and v2, TCP and uTP with LEDBAT, encrypted connections, hole punching
- **Finding peers:** trackers (HTTP, UDP, WebSocket), DHT, peer exchange, local discovery
- **Other sources:** web seeds, WebTorrent peers
- **Network:** IPv4 and IPv6, router port mapping

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

## Other builds

- **Control server:** `cargo build --release -p ziyron-engine --bin engine-host`
  gives you a localhost HTTP API for building your own app.
- **Android:** see [docs/ANDROID_BUILD.md](docs/ANDROID_BUILD.md).

## Roadmap

- A head-to-head benchmark against libtorrent, rqbit and WebTorrent
- The full test suite in CI
- A stable public Rust API
- Windows support: the engine currently uses Unix file descriptors

## Licence

[MPL-2.0](LICENSE), because Ziyron derives from anacrolix/torrent. See
[NOTICE.md](NOTICE.md) and [CONTRIBUTING.md](CONTRIBUTING.md).

Use Ziyron only with content you have the right to download and share.
