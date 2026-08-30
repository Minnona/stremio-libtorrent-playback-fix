# Stremio libtorrent playback fix

An experimental fork of
[`stremio-native/stream-server`](https://github.com/stremio-native/stream-server)
for torrents that download quickly but make Stremio wait far too long before
starting playback.

## The problem

On fast swarms, libtorrent could fill peer request queues with unrelated pieces
before HLS selected the beginning and metadata pieces needed for playback. The
torrent would continue downloading at full speed, yet Stremio could remain on
the loading screen until a large part of the file was complete.

This does **not** turn a slow or poorly seeded torrent into a fast one. It fixes
the case where bandwidth is available but playback-critical pieces are stuck
behind bulk downloading.

## Changes in this fork

- Newly resolved magnets begin with all files paused until the requested file is
  selected.
- The complete active playback window uses libtorrent priority `7` with ordered
  deadlines.
- Per-peer request queues are reduced from `1500` to `250`, and the target queue
  time from 3 seconds to 1 second.
- Whole-piece and adjacent-piece preferences are disabled so urgent blocks can
  preempt bulk work.
- Linux builds support system libtorrent 2.1 without relying on its private
  `aux::from_hex` function.
- `STREAM_SERVER_HTTP_PORT` and `STREAM_SERVER_DISABLE_HTTPS` can configure a
  localhost-only Stremio integration.
- Restores Stremio Enhanced's local **Play in MPV** device and launch endpoint
  when MPV is installed at `/usr/bin/mpv` or `/usr/local/bin/mpv`.

The queue values are intentionally conservative. More testing across different
connections and swarms is welcome.

## Linux binary

The release binary is for x86-64 Linux and dynamically links against
`libtorrent-rasterbar.so.2.1`. FFmpeg and FFprobe must also be installed.

```bash
chmod +x stream-server-linux-x86_64-libtorrent-2.1
STREAM_SERVER_HTTP_PORT=11470 STREAM_SERVER_DISABLE_HTTPS=1 \
  ./stream-server-linux-x86_64-libtorrent-2.1 --no-tray
```

It is intended as a replacement backend for users already running Stream Server
with Stremio. Fully stop the existing backend before replacing it, and keep a
backup of the old executable. Set `STREAM_SERVER_MPV_PATH` if MPV is installed
somewhere other than `/usr/bin/mpv` or `/usr/local/bin/mpv`.

## Build from source

Install Rust, Boost, libtorrent-rasterbar 2.1, FFmpeg, and pkg-config, then run:

```bash
cargo build --release --features libtorrent --no-default-features -p server
```

The resulting executable is `target/release/server`.

## Status

The build was tested on Linux with libtorrent 2.1.1. It compiled successfully,
started on an isolated port, passed its heartbeat check, and improved the
reported real-world playback case. It remains an unofficial experimental fork,
not an official Stremio release.

## Credits and license

Based on the MIT-licensed Stream Server project by its original contributors.
The original Git history and [`LICENSE`](LICENSE) are retained.
