//! Stage 1: gzip producer.
//!
//! Spawns a worker thread that decompresses the save file and sends 64 KB
//! chunks through a bounded `mpsc::SyncSender`. The caller drains the
//! receiver to feed the Stage 2 byte scanner.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::JoinHandle;

use flate2::read::GzDecoder;

/// Number of chunks the producer may have in flight before blocking.
/// 32 × 64 KB = 2 MB backpressure window.
const CHANNEL_CAPACITY: usize = 32;
/// Size of each chunk pushed onto the channel.
pub const CHUNK_SIZE: usize = 64 * 1024;

/// Handle returned by `spawn_decompressor`. The receiver yields owned chunks
/// in arrival order; the join handle reports any IO/gzip error after EOF.
pub struct Decompressor {
    pub rx: Receiver<Vec<u8>>,
    pub handle: JoinHandle<std::io::Result<()>>,
}

/// Spawn the decompressor thread. Caller must drain `rx` and may join `handle`
/// after the channel disconnects.
pub fn spawn_decompressor(path: &Path) -> std::io::Result<Decompressor> {
    let file = File::open(path)?;
    let (tx, rx): (SyncSender<Vec<u8>>, Receiver<Vec<u8>>) =
        mpsc::sync_channel(CHANNEL_CAPACITY);
    let handle = std::thread::spawn(move || pump(file, tx));
    Ok(Decompressor { rx, handle })
}

fn pump(file: File, tx: SyncSender<Vec<u8>>) -> std::io::Result<()> {
    let mut gz = GzDecoder::new(file);
    loop {
        let mut buf = vec![0u8; CHUNK_SIZE];
        let n = gz.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        buf.truncate(n);
        if tx.send(buf).is_err() {
            // Receiver dropped → caller no longer interested.
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_gzipped(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        enc.write_all(payload).unwrap();
        enc.finish().unwrap();
        out
    }

    fn write_temp_gz(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(bytes).unwrap();
        f
    }

    #[test]
    fn producer_streams_chunks_until_eof() {
        let payload: Vec<u8> = (0..200_000u32).flat_map(|i| (i as u8).to_le_bytes()).collect();
        let gz = make_gzipped(&payload);
        let f = write_temp_gz(&gz);

        let d = spawn_decompressor(f.path()).expect("spawn");
        let mut got: Vec<u8> = Vec::new();
        while let Ok(chunk) = d.rx.recv() {
            got.extend_from_slice(&chunk);
        }
        d.handle.join().unwrap().unwrap();
        assert_eq!(got, payload);
    }

    #[test]
    fn producer_handles_short_payload() {
        let gz = make_gzipped(b"hello");
        let f = write_temp_gz(&gz);

        let d = spawn_decompressor(f.path()).expect("spawn");
        let mut got = Vec::new();
        while let Ok(chunk) = d.rx.recv() {
            got.extend(chunk);
        }
        d.handle.join().unwrap().unwrap();
        assert_eq!(got, b"hello");
    }
}
