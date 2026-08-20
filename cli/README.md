# Secure file transfer with a Rust CLI over a private iroh relay

A two-role command-line app: one machine runs a **receiver** and prints a
ticket, another runs a **sender** with that ticket and pushes a file or a whole
directory. The bytes travel over a direct, end-to-end encrypted QUIC connection;
the [self-hosted relay](../README.md) is only there to introduce the peers and
to carry traffic while hole punching is still in progress.

## Configuration

`config.toml` is required and is **always read from the working directory**.

```sh
cp config.toml.example config.toml
```

```toml
# The UDP port the receiver binds.
# `0` asks for a free port (ticket changes on every start).
bind_port = 11842

[[relay]]
url = "https://<VPC DNS host>"
auth_token = "REPLACE_WITH_YOUR_TOKEN"
# QUIC address discovery port
# quic = { port = 7842 }
```

Multiple `[[relay]]` entries are allowed; the endpoint probes them and picks
its home relay by latency.

## Build, run, test

```sh
# run from source (see the roles below)
cargo run -- receiver

# build an optimised binary at target/release/cli
cargo build --release

# test
cargo test
```

`cargo run` with no role argument is an error: the first argument must be
`receiver` or `sender`, and `sender` needs a path as its second.

The test suite is entirely offline — the integration probe in
`tests/feasibility.rs` runs both endpoints in-process with `RelayMode::Disabled`
— so no relay and no `config.toml` are needed to run it.

## Run the app

**Receiver.** Prints its ticket, then stays up and accepts pushes until Ctrl-C:

```sh
cargo run -- receiver
```

**Sender.** Reads the ticket from the `IROH_TICKET` environment variable,
pushes once, and exits:

```sh
export IROH_TICKET=<ticket>
cargo run -- sender ./notes.txt
cargo run -- sender ./photos
```

Surrounding whitespace and the trailing newline of the receiver's output are
tolerated, so piping the ticket across works without trimming it first.

### What lands where

| Path | Created by | Notes |
| --- | --- | --- |
| `downloads/` | receiver | Where completed transfers are exported. Created on the first successful push. |
| `secret.key` | receiver | The endpoint's ed25519 identity, hex-encoded, mode `0600`. Generated on first run. Delete it and the node ID — and every ticket — changes. |
| `.iroh-send-<pid>-<nanos>/` | sender | Temp blob store for one send. Removed on success, failure and panic. |
| `.iroh-recv-<hash>/` | receiver | Temp blob store for one receive, keyed by the collection hash. Removed the same way. |

`downloads/`, `secret.key` and `config.toml` are git-ignored. The two temp
stores are not listed, but they only exist while a transfer is running — they
are removed on success, on failure and on panic.

Names never collide destructively: if `downloads/notes.txt` exists, the next one
arrives as `notes-1.txt` (the extension is preserved so it still looks like a
text file); a directory becomes `photos-1`.

### Directory pushes

- The tree is walked recursively and entries are sorted by name, so the same
  tree always produces the same collection hash.
- **Symlinks are skipped, not followed** — a link inside the folder would
  otherwise pull in a file from anywhere on the sender's disk.
- Empty directories carry no files and so do not appear on the receiver.
- Entry names travel with `/` separators on every platform.
- A single file lands as `downloads/notes.txt`, not inside a directory of its
  own: the root name comes from the announce, and `is_dir` is carried
  explicitly because a directory holding one identically-named file is otherwise
  indistinguishable from a lone file.

## How it works

```
sender                                                   receiver
  │ walk the path, reject locally-detectable errors first
  │ import every file into a temp blob store (by reference, not by copy)
  │ build a collection (HashSeq) over the imported blobs
  │
  ├── ping (iroh-ping) ────────────────────────────────►  a cheap path check
  │◄── rtt                                                before any transfer
  │
  ├── connect, ALPN "iroh-app/push/0" ─────────────────►
  ├── uni stream: Announce frame ──────────────────────►  validate, then accept
  │                                                       or refuse outright
  │◄── iroh-blobs get request ─────────────────────────┤  the *receiver* pulls
  ├── payload ─────────────────────────────────────────►
  │                                                       verify, then export
  │◄── CONNECTION_CLOSE with an outcome code ──────────┤   into downloads/
```

Two things about this shape are worth knowing:

- **The side that dials is the side that serves.** `iroh-blobs` is a pull
  protocol, so the sender hands the connection to the blobs provider and the
  receiver drives the transfer. That this works on a dialed connection is the
  one design assumption that was not obvious, and `tests/feasibility.rs` pins it
  down.
- **The close code is the report.** The receiver's `CONNECTION_CLOSE` code is
  the only thing that distinguishes a refusal from a network failure:
  `0` stored everything, `1` rejected the push before touching the disk,
  `2` accepted it and then broke part way through.

### The announce frame

The one frame a stranger can hand the receiver, so it is a fixed-width,
hand-written layout rather than a serde-decoded one — a parser whose failure
modes can be enumerated in tests. The `/0` suffix on the ALPN is its version:
any change to the layout must bump it, so a mismatched peer is refused at the
handshake instead of misreading a field.

| Field | Bytes | Meaning |
| --- | --- | --- |
| `hash` | 32 | The collection's BLAKE3 hash |
| `format` | 1 | Must decode to `HashSeq`; a wrong value is refused, not a parse error |
| `total_size` | 8 (LE) | Sum of file sizes, for progress and the accept log |
| `file_count` | 4 (LE) | Number of files |
| `is_dir` | 1 | Whether the pushed path was a directory |
| `name_len` | 2 (LE) | Length of the name that follows, ≤ 255 |
| `name` | `name_len` | The root file or directory name, UTF-8, one path component |

The receiver reads at most `48 + 255` bytes, so a peer cannot make it buffer
without bound, and a frame carrying *more* than it declared is as much a
protocol violation as one carrying less.

### Safety on the receiving side

- Every entry name is validated before **any** file is exported — no empty
  components, no `.` or `..`, no `/`, `\`, `:` or NUL — so a hostile name at
  position 9 cannot leave 8 files behind.
- Bytes land in a temp store and are exported into `downloads/` only once the
  whole collection has arrived and verified. The download directory is untouched
  until then.

### Safety on the sending side

The sender serves the collection through a full `iroh-blobs` provider, which
also understands *push* requests — the peer streaming blob data **into** the
provider's store. That is disabled outright (`push: RequestMode::Disabled`),
so the only thing the receiver can do on the connection is fetch what it was
announced. The default for that field is `None`, which means "no events, but
requests are processed normally", so leaving it out would have left push
enabled; `Disabled` is the variant that actually rejects.
