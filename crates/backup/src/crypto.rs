//! Chunked authenticated encryption of a byte stream (XChaCha20-Poly1305).
//!
//! Layout of a stream: the magic `DHBK1`, a 16-byte random nonce prefix, then chunks, each
//! `u32 big-endian length` + ciphertext (the plain chunk and a 16-byte tag). A chunk's
//! nonce is the prefix followed by its number (8 bytes, big-endian); the associated data is
//! one byte, `1` for the last chunk and `0` for every other. The last chunk may be empty.

use std::io::{self, Read, Write};

use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};

/// Length of a key.
pub const KEY_LEN: usize = 32;
/// A new random key (for one backup).
///
/// # Errors
/// The system's random source failing.
pub fn new_key() -> io::Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    getrandom::fill(&mut key).map_err(|_| io::Error::other("no random numbers"))?;
    Ok(key)
}

/// The key as the 64 hex digits the node operations carry.
#[must_use]
pub fn key_hex(key: &[u8; KEY_LEN]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

/// Plain bytes per chunk.
pub const CHUNK: usize = 64 * 1024;
const MAGIC: &[u8; 5] = b"DHBK1";
const PREFIX_LEN: usize = 16;
const TAG_LEN: usize = 16;

fn nonce(prefix: &[u8; PREFIX_LEN], index: u64) -> XNonce {
    let mut n = [0u8; 24];
    n[..PREFIX_LEN].copy_from_slice(prefix);
    n[PREFIX_LEN..].copy_from_slice(&index.to_be_bytes());
    XNonce::from(n)
}

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_owned())
}

/// Encrypts what is written to it into `inner`. Call [`StreamWriter::finish`] at the end:
/// without it the stream has no last chunk and will not read.
pub struct StreamWriter<W: Write> {
    inner: W,
    cipher: XChaCha20Poly1305,
    prefix: [u8; PREFIX_LEN],
    index: u64,
    buf: Vec<u8>,
    started: bool,
}

impl<W: Write> StreamWriter<W> {
    /// A writer with a key of [`KEY_LEN`] bytes.
    ///
    /// # Errors
    /// The system has no randomness.
    pub fn new(inner: W, key: &[u8; KEY_LEN]) -> io::Result<Self> {
        let mut prefix = [0u8; PREFIX_LEN];
        getrandom::fill(&mut prefix).map_err(|_| io::Error::other("no randomness"))?;
        Ok(Self {
            inner,
            cipher: XChaCha20Poly1305::new(key.into()),
            prefix,
            index: 0,
            buf: Vec::with_capacity(CHUNK),
            started: false,
        })
    }

    fn put(&mut self, last: bool) -> io::Result<()> {
        if !self.started {
            self.inner.write_all(MAGIC)?;
            self.inner.write_all(&self.prefix)?;
            self.started = true;
        }
        let sealed = self
            .cipher
            .encrypt(
                &nonce(&self.prefix, self.index),
                Payload {
                    msg: &self.buf,
                    aad: &[u8::from(last)],
                },
            )
            .map_err(|_| io::Error::other("encryption failed"))?;
        let len = u32::try_from(sealed.len()).map_err(|_| io::Error::other("chunk too big"))?;
        self.inner.write_all(&len.to_be_bytes())?;
        self.inner.write_all(&sealed)?;
        self.index += 1;
        self.buf.clear();
        Ok(())
    }

    /// Writes the last chunk and returns the inner writer.
    ///
    /// # Errors
    /// Whatever the inner writer says.
    pub fn finish(mut self) -> io::Result<W> {
        self.put(true)?;
        self.inner.flush()?;
        Ok(self.inner)
    }
}

impl<W: Write> Write for StreamWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let mut rest = data;
        while !rest.is_empty() {
            // A full chunk is only written once more data follows it, so the last
            // chunk is never empty unless the whole stream is.
            if self.buf.len() == CHUNK {
                self.put(false)?;
            }
            let room = CHUNK - self.buf.len();
            let take = room.min(rest.len());
            self.buf.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Decrypts a stream made by [`StreamWriter`]; a stream that was changed, cut, extended or
/// opened with another key is an [`io::ErrorKind::InvalidData`] error.
pub struct StreamReader<R: Read> {
    inner: R,
    cipher: XChaCha20Poly1305,
    prefix: Option<[u8; PREFIX_LEN]>,
    index: u64,
    plain: Vec<u8>,
    at: usize,
    done: bool,
}

impl<R: Read> StreamReader<R> {
    /// A reader with the key the stream was made with.
    pub fn new(inner: R, key: &[u8; KEY_LEN]) -> Self {
        Self {
            inner,
            cipher: XChaCha20Poly1305::new(key.into()),
            prefix: None,
            index: 0,
            plain: Vec::new(),
            at: 0,
            done: false,
        }
    }

    fn next_chunk(&mut self) -> io::Result<()> {
        let prefix = match self.prefix {
            Some(p) => p,
            None => {
                let mut head = [0u8; MAGIC.len() + PREFIX_LEN];
                self.inner
                    .read_exact(&mut head)
                    .map_err(|_| bad("not a backup part"))?;
                if &head[..MAGIC.len()] != MAGIC {
                    return Err(bad("not a backup part"));
                }
                let mut p = [0u8; PREFIX_LEN];
                p.copy_from_slice(&head[MAGIC.len()..]);
                self.prefix = Some(p);
                p
            }
        };
        let mut len = [0u8; 4];
        self.inner
            .read_exact(&mut len)
            .map_err(|_| bad("the part ends early"))?;
        let len = u32::from_be_bytes(len) as usize;
        if !(TAG_LEN..=CHUNK + TAG_LEN).contains(&len) {
            return Err(bad("a chunk has an impossible size"));
        }
        let mut sealed = vec![0u8; len];
        self.inner
            .read_exact(&mut sealed)
            .map_err(|_| bad("the part ends early"))?;
        // The last chunk is the one after which nothing follows; it is told apart by trying
        // "last" when a full chunk could not be, then checking for trailing bytes.
        let mut plain = None;
        for last in [false, true] {
            if let Ok(p) = self.cipher.decrypt(
                &nonce(&prefix, self.index),
                Payload {
                    msg: &sealed,
                    aad: &[u8::from(last)],
                },
            ) {
                self.done = last;
                plain = Some(p);
                break;
            }
        }
        let plain =
            plain.ok_or_else(|| bad("a chunk does not check out (changed, or another key)"))?;
        self.index += 1;
        if self.done {
            let mut extra = [0u8; 1];
            if self.inner.read(&mut extra)? != 0 {
                return Err(bad("there is more after the end of the part"));
            }
        }
        self.plain = plain;
        self.at = 0;
        Ok(())
    }
}

impl<R: Read> Read for StreamReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        while self.at >= self.plain.len() {
            if self.done {
                return Ok(0);
            }
            self.next_chunk()?;
        }
        let n = out.len().min(self.plain.len() - self.at);
        out[..n].copy_from_slice(&self.plain[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_LEN] = [7; KEY_LEN];

    fn seal(data: &[u8]) -> Vec<u8> {
        let mut w = StreamWriter::new(Vec::new(), &KEY).unwrap();
        w.write_all(data).unwrap();
        w.finish().unwrap()
    }

    fn open(blob: &[u8], key: &[u8; KEY_LEN]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        StreamReader::new(blob, key).read_to_end(&mut out)?;
        Ok(out)
    }

    #[test]
    fn what_goes_in_comes_out_for_every_size_around_a_chunk() {
        for n in [
            0,
            1,
            100,
            CHUNK - 1,
            CHUNK,
            CHUNK + 1,
            3 * CHUNK,
            3 * CHUNK + 17,
        ] {
            let data: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
            let blob = seal(&data);
            assert_eq!(open(&blob, &KEY).unwrap(), data, "{n}");
        }
    }

    #[test]
    fn nothing_readable_is_in_the_stream_and_two_streams_differ() {
        let data = b"top secret top secret top secret".repeat(100);
        let (a, b) = (seal(&data), seal(&data));
        assert_ne!(a, b, "a new nonce each time");
        assert!(!a.windows(10).any(|w| w == b"top secret"));
    }

    #[test]
    fn a_changed_cut_extended_or_reordered_stream_is_found() {
        let data: Vec<u8> = (0..3 * CHUNK + 5).map(|i| (i % 253) as u8).collect();
        let good = seal(&data);
        // One flipped bit anywhere after the header.
        for at in [30, good.len() / 2, good.len() - 1] {
            let mut bad = good.clone();
            bad[at] ^= 1;
            assert!(open(&bad, &KEY).is_err(), "bit at {at}");
        }
        // Cut short, at any point.
        for cut in [0, 10, 21, 100, good.len() / 2, good.len() - 1] {
            assert!(open(&good[..cut], &KEY).is_err(), "cut at {cut}");
        }
        // Cut exactly at a chunk boundary (a whole chunk dropped from the end).
        let one = 4 + CHUNK + TAG_LEN;
        assert!(
            open(&good[..MAGIC.len() + PREFIX_LEN + one], &KEY).is_err(),
            "no last chunk"
        );
        // Something added after the end.
        let mut longer = good.clone();
        longer.extend_from_slice(b"x");
        assert!(open(&longer, &KEY).is_err());
        // Two chunks swapped.
        let head = MAGIC.len() + PREFIX_LEN;
        let mut swapped = good.clone();
        let (first, second) = (head..head + one, head + one..head + 2 * one);
        let (c1, c2) = (good[first.clone()].to_vec(), good[second.clone()].to_vec());
        swapped[first].copy_from_slice(&c2);
        swapped[second].copy_from_slice(&c1);
        assert!(open(&swapped, &KEY).is_err());
        // Another key, and something that is not a part at all.
        assert!(open(&good, &[8; KEY_LEN]).is_err());
        assert!(open(b"plain text, not a backup", &KEY).is_err());
    }
}
