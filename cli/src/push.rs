use anyhow::{Result, anyhow, bail, ensure};
use iroh_blobs::{BlobFormat, Hash};

/// The ALPN the receiver accepts pushes on.
///
/// The trailing `/0` is the version of the frame below: any change to the
/// layout must bump it, so a mismatched peer is refused at the handshake rather
/// than misreading a field.
pub const ALPN: &[u8] = b"iroh-app/push/0";

/// Everything arrived and was exported.
pub const CLOSE_OK: u32 = 0;
/// The announce was malformed or named an unsafe path. Nothing was written.
pub const CLOSE_REJECTED: u32 = 1;
/// The transfer was accepted and then broke part way through.
pub const CLOSE_FAILED: u32 = 2;

/// The longest root name accepted, in bytes.
pub const MAX_NAME_LEN: usize = 255;

/// Every field but the name is fixed width.
const HEADER_LEN: usize = 32 + 1 + 8 + 4 + 1 + 2;

/// The largest frame there is. The receiver reads no more than this, so a
/// sender cannot make it buffer without bound.
pub const MAX_ANNOUNCE_LEN: usize = HEADER_LEN + MAX_NAME_LEN;

const FORMAT_RAW: u8 = 0;
const FORMAT_HASH_SEQ: u8 = 1;

/// What the sender writes on a uni stream before it starts serving.
///
/// The layout is fixed rather than serde-encoded because this is the one frame
/// a stranger can hand the receiver, and a hand-written parser is a parser whose
/// failure modes can be enumerated in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announce {
    /// The collection's hash.
    pub hash: Hash,
    /// The receiver requires [`BlobFormat::HashSeq`]; the field exists so a
    /// wrong one is a decoded value it can refuse rather than a parse failure.
    pub format: BlobFormat,
    /// The sum of the file sizes, for the progress line and the accept log.
    pub total_size: u64,
    pub file_count: u32,
    /// Whether the pushed path was a directory.
    ///
    /// This cannot be inferred from `file_count`: a directory holding a single
    /// identically-named file is otherwise indistinguishable from a lone file.
    pub is_dir: bool,
    /// The root file or directory name — one path component.
    pub name: String,
}

impl Announce {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let name = self.name.as_bytes();
        ensure!(!name.is_empty(), "the root name is empty");
        ensure!(
            name.len() <= MAX_NAME_LEN,
            "the root name is {} bytes, over the {MAX_NAME_LEN}-byte limit",
            name.len()
        );

        let mut out = Vec::with_capacity(HEADER_LEN + name.len());
        out.extend_from_slice(self.hash.as_bytes());
        out.push(match self.format {
            BlobFormat::Raw => FORMAT_RAW,
            BlobFormat::HashSeq => FORMAT_HASH_SEQ,
        });
        out.extend_from_slice(&self.total_size.to_le_bytes());
        out.extend_from_slice(&self.file_count.to_le_bytes());
        out.push(u8::from(self.is_dir));
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() >= HEADER_LEN,
            "the announce is {} bytes, short of the {HEADER_LEN}-byte header",
            bytes.len()
        );
        let (header, name) = bytes.split_at(HEADER_LEN);

        let hash = Hash::from_bytes(header[..32].try_into().expect("32 bytes"));
        let format = match header[32] {
            FORMAT_RAW => BlobFormat::Raw,
            FORMAT_HASH_SEQ => BlobFormat::HashSeq,
            other => bail!("unknown format byte {other}"),
        };
        let total_size = u64::from_le_bytes(header[33..41].try_into().expect("8 bytes"));
        let file_count = u32::from_le_bytes(header[41..45].try_into().expect("4 bytes"));
        let is_dir = match header[45] {
            0 => false,
            1 => true,
            other => bail!("unknown is_dir byte {other}"),
        };
        let name_len = usize::from(u16::from_le_bytes(
            header[46..48].try_into().expect("2 bytes"),
        ));

        // Exact rather than "at least": a frame carrying more than it declared is
        // as much a protocol violation as one carrying less.
        ensure!(
            name.len() == name_len,
            "the announce declares a {name_len}-byte name but carries {}",
            name.len()
        );
        let name = String::from_utf8(name.to_vec())
            .map_err(|_| anyhow!("the root name is not valid UTF-8"))?;

        Ok(Self {
            hash,
            format,
            total_size,
            file_count,
            is_dir,
            name,
        })
    }
}

#[cfg(test)]
#[path = "tests/push.rs"]
mod tests;
