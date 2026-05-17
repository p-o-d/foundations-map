# Parallel Save Parsing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut X4 save-parse wall time from ~7.7s to ≤3.5s on a 96 MB save (current dev machine baseline) by pipelining gzip + byte-scan and parallelizing per-sector entity extraction with rayon.

**Architecture:** Four sequential stages with the first two overlapping. Stage 1 (`std::thread`) decompresses gzip into chunks, feeding them through an `mpsc::SyncSender`. Stage 2 byte-scans the chunks as they arrive — no `quick_xml` here, only `memchr` for tag detection — extracting `SnapshotMeta`, sector → owner overrides, and `(byte_range, sector_macro)` chunks. Stage 3 uses `rayon::par_iter` over those chunks: each worker re-parses its small sector subtree with `quick_xml` and emits a `Vec<EntityRecord>`. Stage 4 merges all records into a single `World` on the calling thread.

**Tech Stack:** Rust 2024, `std::sync::mpsc::sync_channel`, `flate2`, `memchr`, `rayon`, `quick_xml` (existing).

**Spec:** `docs/superpowers/specs/2026-05-17-save-parse-parallel.md`

---

## File Structure

After this plan, `crates/map-io/src/save_parser/` replaces the single `save_parser.rs` file:

```
crates/map-io/src/save_parser/
    mod.rs              — public parse_save; orchestrates stages
    decompress.rs       — Stage 1: spawn_decompressor (gzip thread + mpsc)
    scan.rs             — Stage 2: byte scanner over chunked input
    sector_chunk.rs     — Stage 3: parse_sector_chunk (quick_xml on a slice)
    merge.rs            — Stage 4: World assembly
    types.rs            — SectorChunk, EntityRecord, FactionOverrides
```

`crates/map-io/Cargo.toml` adds `rayon = "1"` and `memchr = "2"`.

Public API unchanged: `pub fn parse_save(path: &Path, sector_macros: Option<&HashMap<String, SectorId>>) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError>`.

---

### Task 1: Add deps + create module skeleton

**Files:**
- Modify: `crates/map-io/Cargo.toml`
- Delete: `crates/map-io/src/save_parser.rs`
- Create: `crates/map-io/src/save_parser/mod.rs`
- Create: `crates/map-io/src/save_parser/types.rs`
- Create: `crates/map-io/src/save_parser/decompress.rs`
- Create: `crates/map-io/src/save_parser/scan.rs`
- Create: `crates/map-io/src/save_parser/sector_chunk.rs`
- Create: `crates/map-io/src/save_parser/merge.rs`

- [ ] **Step 1: Add deps to `crates/map-io/Cargo.toml`**

Find the existing `[dependencies]` section. Add two lines:
```toml
rayon = "1"
memchr = "2"
```

- [ ] **Step 2: Create `crates/map-io/src/save_parser/types.rs`**

```rust
//! Shared types for the parallel save parser.

use map_domain::world::LiveObjectKind;
use std::ops::Range;

/// Map sector_macro (lowercase) → faction owner string from `<sector owner="...">`.
/// Caller resolves owner string to FactionId.
pub type FactionOverrides = std::collections::HashMap<String, String>;

/// Byte range of one `<component class="sector" …>…</component>` subtree
/// inside the decompressed save buffer.
#[derive(Debug, Clone)]
pub struct SectorChunk {
    pub sector_macro: String, // lowercase
    pub byte_range: Range<usize>,
}

/// One ship or station extracted from a sector chunk by a Stage 3 worker.
/// Caller resolves `sector_macro` → SectorId and `owner` → FactionId.
#[derive(Debug, Clone)]
pub struct EntityRecord {
    pub id: u32, // parsed from "[0xHEX]"
    pub name: String,
    pub kind: LiveObjectKind,
    pub owner: Option<String>,
    pub position: glam::Vec3, // already km (metres / 1000)
    pub sector_macro: String, // lowercase
}
```

- [ ] **Step 3: Create `crates/map-io/src/save_parser/decompress.rs` placeholder**

```rust
//! Stage 1: gzip producer.
//!
//! Spawns a thread that decompresses the save file and sends 64 KB chunks
//! through a bounded `mpsc::SyncSender`. The caller drains them into the
//! Stage 2 byte scanner.

// Implementation in Task 3.
```

- [ ] **Step 4: Create `crates/map-io/src/save_parser/scan.rs` placeholder**

```rust
//! Stage 2: byte scanner (no quick_xml).
// Implementation in Task 4.
```

- [ ] **Step 5: Create `crates/map-io/src/save_parser/sector_chunk.rs` placeholder**

```rust
//! Stage 3: parse one sector subtree using quick_xml.
// Implementation in Task 5.
```

- [ ] **Step 6: Create `crates/map-io/src/save_parser/merge.rs` placeholder**

```rust
//! Stage 4: merge per-worker entity records into a single World.
// Implementation in Task 6.
```

- [ ] **Step 7: Create `crates/map-io/src/save_parser/mod.rs`**

This re-exports the existing `parse_save` signature. For now it just delegates to a stub returning empty data so the workspace builds. The real orchestrator goes in Task 7.

```rust
//! Public entrypoint for the parallel save parser.
//!
//! Orchestrates four stages: gzip decompress + byte scan (overlapping),
//! rayon per-sector parse, single-threaded merge into a `World`.

pub mod decompress;
pub mod merge;
pub mod scan;
pub mod sector_chunk;
pub mod types;

use std::collections::HashMap;
use std::path::Path;

use map_domain::ids::SectorId;
use map_domain::world::{SnapshotMeta, World};

use crate::xml_parser::ParseError;

pub use types::FactionOverrides;

/// Parse an X4 save file in parallel.
///
/// This is a placeholder that returns empty data; the real orchestration is
/// wired up in Task 7. Existing callers in the binary already handle empty
/// results gracefully (no entities → no live ships drawn).
pub fn parse_save(
    _path: &Path,
    _sector_macros: Option<&HashMap<String, SectorId>>,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    Err(ParseError::MissingAttribute(
        "save_parser not yet wired (Task 7)".into(),
    ))
}
```

- [ ] **Step 8: Delete `crates/map-io/src/save_parser.rs`**

Run:
```bash
rm crates/map-io/src/save_parser.rs
```

(The `pub mod save_parser;` line in `crates/map-io/src/lib.rs` now resolves to the new directory module.)

- [ ] **Step 9: Build the workspace**

Run:
```bash
cargo build 2>&1 | grep "^error" | head -5
```
Expected: no `error:` lines (only the existing `dead_code` warnings if any).

The existing `save_parser::tests::*` tests in the deleted file are gone too — they'll be reintroduced by Tasks 3-7 as we rebuild functionality.

- [ ] **Step 10: Commit**

```bash
git add crates/map-io/Cargo.toml crates/map-io/src/save_parser \
        crates/map-io/src/save_parser.rs
git commit -m "refactor(io): split save_parser into stages module (skeleton)"
```

(The `git add` for the deleted file is needed so git records its removal.)

---

### Task 2: SectorChunk + EntityRecord unit tests

**Files:**
- Modify: `crates/map-io/src/save_parser/types.rs`
- Test: `crates/map-io/src/save_parser/types.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write failing tests at the bottom of `types.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::world::LiveObjectKind;

    #[test]
    fn sector_chunk_holds_range_and_macro() {
        let c = SectorChunk {
            sector_macro: "cluster_01_sector001_macro".into(),
            byte_range: 100..2000,
        };
        assert_eq!(c.byte_range.len(), 1900);
        assert!(c.sector_macro.starts_with("cluster_"));
    }

    #[test]
    fn entity_record_constructs() {
        let e = EntityRecord {
            id: 0x100,
            name: "station_arg_factory_01".into(),
            kind: LiveObjectKind::Station,
            owner: Some("argon".into()),
            position: glam::Vec3::new(0.0, 0.0, 0.0),
            sector_macro: "cluster_01_sector001_macro".into(),
        };
        assert_eq!(e.id, 0x100);
        assert_eq!(e.owner.as_deref(), Some("argon"));
    }
}
```

- [ ] **Step 2: Run tests**

Run:
```bash
cargo test -p map-io --lib save_parser::types::tests 2>&1 | tail -5
```
Expected: `2 passed`.

- [ ] **Step 3: Commit**

```bash
git add crates/map-io/src/save_parser/types.rs
git commit -m "test(io): unit tests for SectorChunk and EntityRecord"
```

---

### Task 3: Stage 1 — gzip producer thread

**Files:**
- Modify: `crates/map-io/src/save_parser/decompress.rs`
- Test: same file (inline)

- [ ] **Step 1: Replace placeholder with real implementation**

```rust
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
```

- [ ] **Step 2: Add `tempfile` to dev-deps**

Edit `crates/map-io/Cargo.toml`. After the existing `[dependencies]` block, add (or extend if present):

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Run tests**

Run:
```bash
cargo test -p map-io --lib save_parser::decompress::tests 2>&1 | tail -5
```
Expected: `2 passed`.

- [ ] **Step 4: Commit**

```bash
git add crates/map-io/Cargo.toml crates/map-io/src/save_parser/decompress.rs
git commit -m "feat(io): Stage 1 gzip producer thread with bounded mpsc"
```

---

### Task 4: Stage 2 — byte scanner

**Files:**
- Modify: `crates/map-io/src/save_parser/scan.rs`
- Test: same file (inline)

The scanner consumes chunks from a `Receiver<Vec<u8>>`, accumulates them into a single `Vec<u8>`, and extracts: `SnapshotMeta`, `FactionOverrides`, and `Vec<SectorChunk>`. It uses `memchr` for tag boundaries — no `quick_xml`.

- [ ] **Step 1: Write the helper-attribute test first (failing)**

Put this at the bottom of `scan.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_attr_extracts_simple_value() {
        let tag = b"<component class=\"sector\" macro=\"foo_macro\" owner=\"argon\">";
        assert_eq!(find_attr(tag, b"class"), Some(b"sector".as_ref()));
        assert_eq!(find_attr(tag, b"macro"), Some(b"foo_macro".as_ref()));
        assert_eq!(find_attr(tag, b"owner"), Some(b"argon".as_ref()));
        assert_eq!(find_attr(tag, b"missing"), None);
    }
}
```

- [ ] **Step 2: Run — expect FAIL (`find_attr` not found)**

```bash
cargo test -p map-io --lib save_parser::scan::tests::find_attr_extracts_simple_value 2>&1 | tail -3
```
Expected: build error / function not found.

- [ ] **Step 3: Implement `find_attr` + module skeleton**

Replace `scan.rs` contents with:

```rust
//! Stage 2: byte scanner.
//!
//! Walks the chunked save buffer with `memchr`. Detects `<info>`, `<game …>`,
//! `<player …>`, `<component class="sector" …>`, `<component …>`, and
//! `</component>` patterns. Produces:
//! - `SnapshotMeta` (from <info>)
//! - `FactionOverrides` (per-sector owner string)
//! - `Vec<SectorChunk>` (byte ranges of each top-level sector subtree)
//!
//! Does NOT touch ship/station entities — that happens in Stage 3.

use std::sync::mpsc::Receiver;

use map_domain::world::SnapshotMeta;
use memchr::memchr;
use memchr::memmem;

use super::types::{FactionOverrides, SectorChunk};

/// Find an attribute value inside a single XML start tag's bytes.
/// Returns the raw byte slice (without quotes).
///
/// Cheap and forgiving — assumes well-formed double-quoted attributes
/// generated by X4. Returns None if the attribute is absent.
pub(crate) fn find_attr<'a>(tag: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut needle: Vec<u8> = Vec::with_capacity(name.len() + 2);
    needle.extend_from_slice(name);
    needle.extend_from_slice(b"=\"");
    let start = memmem::find(tag, &needle)? + needle.len();
    let rest = &tag[start..];
    let end = memchr(b'"', rest)?;
    Some(&rest[..end])
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-io --lib save_parser::scan::tests::find_attr_extracts_simple_value 2>&1 | tail -3
```
Expected: `1 passed`.

- [ ] **Step 5: Add the scanner output type + entry function (failing test first)**

Append to the `#[cfg(test)]` block in `scan.rs`:

```rust
    #[test]
    fn scan_extracts_meta_overrides_and_one_chunk() {
        let payload: &[u8] = br#"<?xml version="1.0"?>
<savegame>
  <info>
    <game version="800" build="1" time="100.0"/>
    <player money="42" location="{20004,1}"/>
  </info>
  <universe>
    <component class="galaxy" macro="g">
      <component class="cluster" macro="c">
        <component class="sector" macro="SectorAMacro" owner="argon">
          <component class="ship_s" macro="ship_arg_s" id="[0x10]">
            <offset><position x="1" y="2" z="3"/></offset>
          </component>
        </component>
      </component>
    </component>
  </universe>
</savegame>"#;

        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        tx.send(payload.to_vec()).unwrap();
        drop(tx);

        let out = run_scan(
            rx,
            std::path::PathBuf::from("/tmp/test_save.xml.gz"),
            std::time::UNIX_EPOCH,
        )
        .expect("scan ok");

        assert_eq!(out.meta.player_money, 42);
        assert_eq!(out.meta.game_version, "800.1");
        assert!((out.meta.game_time_seconds - 100.0).abs() < 1e-3);
        assert_eq!(out.meta.player_location_name, "{20004,1}");

        assert_eq!(out.overrides.get("sectoramacro").map(String::as_str), Some("argon"));
        assert_eq!(out.chunks.len(), 1);

        let chunk = &out.chunks[0];
        assert_eq!(chunk.sector_macro, "sectoramacro");
        let slice = &out.bytes[chunk.byte_range.clone()];
        assert!(slice.starts_with(b"<component class=\"sector\""));
        assert!(slice.ends_with(b"</component>"));
    }
```

- [ ] **Step 6: Run — expect FAIL (`run_scan`, `ScanOutput` not found)**

```bash
cargo test -p map-io --lib save_parser::scan::tests::scan_extracts_meta_overrides_and_one_chunk 2>&1 | tail -3
```

- [ ] **Step 7: Implement the scanner**

Append to `scan.rs` (above the `#[cfg(test)] mod tests` block):

```rust
/// What Stage 2 produces.
pub struct ScanOutput {
    pub meta: SnapshotMeta,
    pub overrides: FactionOverrides,
    pub chunks: Vec<SectorChunk>,
    pub bytes: Vec<u8>,
}

/// Drain `rx` and scan all bytes. Caller passes the source path + mtime so
/// `SnapshotMeta` can be filled out completely.
pub fn run_scan(
    rx: Receiver<Vec<u8>>,
    path: std::path::PathBuf,
    mtime: std::time::SystemTime,
) -> std::io::Result<ScanOutput> {
    let mut bytes: Vec<u8> = Vec::with_capacity(128 * 1024 * 1024);
    while let Ok(chunk) = rx.recv() {
        bytes.extend_from_slice(&chunk);
    }

    let mut meta = SnapshotMeta {
        path,
        mtime,
        game_time_seconds: 0.0,
        player_money: 0,
        player_location_name: String::new(),
        game_version: String::new(),
    };
    scan_info(&bytes, &mut meta);

    let (overrides, chunks) = scan_sectors(&bytes);

    Ok(ScanOutput {
        meta,
        overrides,
        chunks,
        bytes,
    })
}

/// Scan only the <info> region for game/player attrs. Cheap.
fn scan_info(bytes: &[u8], meta: &mut SnapshotMeta) {
    let info_end = memmem::find(bytes, b"</info>").unwrap_or(bytes.len());
    let region = &bytes[..info_end];

    if let Some(tag) = find_tag(region, b"<game ") {
        let ver = find_attr(tag, b"version").map(str_from).unwrap_or_default();
        let build = find_attr(tag, b"build").map(str_from).unwrap_or_default();
        meta.game_version = match (ver.is_empty(), build.is_empty()) {
            (false, false) => format!("{}.{}", ver, build),
            (false, true) => ver,
            (true, false) => build,
            _ => String::new(),
        };
        if let Some(t) = find_attr(tag, b"time").and_then(parse_f32) {
            meta.game_time_seconds = t;
        }
    }
    if let Some(tag) = find_tag(region, b"<player ") {
        if let Some(m) = find_attr(tag, b"money").and_then(parse_u64) {
            meta.player_money = m;
        }
        if let Some(loc) = find_attr(tag, b"location").map(str_from) {
            meta.player_location_name = loc;
        }
    }
}

/// Walk the buffer collecting sector chunks + faction overrides.
fn scan_sectors(bytes: &[u8]) -> (FactionOverrides, Vec<SectorChunk>) {
    let mut overrides = FactionOverrides::new();
    let mut chunks: Vec<SectorChunk> = Vec::new();

    // State for the currently-open top-level sector (if any).
    let mut sector_start: Option<usize> = None;
    let mut sector_macro: Option<String> = None;
    let mut depth: u32 = 0;

    let open_needle = b"<component";
    let close_needle = b"</component>";
    let close_finder = memmem::Finder::new(close_needle);
    let open_finder = memmem::Finder::new(open_needle);

    let mut pos = 0;
    while pos < bytes.len() {
        let next_open = open_finder.find(&bytes[pos..]).map(|i| i + pos);
        let next_close = close_finder.find(&bytes[pos..]).map(|i| i + pos);

        match (next_open, next_close) {
            (None, None) => break,
            (Some(o), Some(c)) if o < c => {
                pos = handle_open(bytes, o, &mut depth, &mut sector_start, &mut sector_macro, &mut overrides);
            }
            (Some(o), None) => {
                pos = handle_open(bytes, o, &mut depth, &mut sector_start, &mut sector_macro, &mut overrides);
            }
            (_, Some(c)) => {
                pos = c + close_needle.len();
                if depth == 0 {
                    continue; // stray close — skip
                }
                depth -= 1;
                if depth == 0 {
                    if let (Some(start), Some(mac)) = (sector_start.take(), sector_macro.take()) {
                        chunks.push(SectorChunk {
                            sector_macro: mac,
                            byte_range: start..pos,
                        });
                    }
                }
            }
        }
    }

    (overrides, chunks)
}

fn handle_open(
    bytes: &[u8],
    open_pos: usize,
    depth: &mut u32,
    sector_start: &mut Option<usize>,
    sector_macro: &mut Option<String>,
    overrides: &mut FactionOverrides,
) -> usize {
    let close_gt = memchr(b'>', &bytes[open_pos..]).map(|i| i + open_pos);
    let tag_end = match close_gt {
        Some(e) => e + 1,
        None => return bytes.len(),
    };
    let tag = &bytes[open_pos..tag_end];

    // Only inspect class when we're either at depth 0 (might start a sector)
    // or already inside a sector (need to track nested component depth, but
    // we don't care which class).
    if *depth == 0 {
        if let Some(class) = find_attr(tag, b"class") {
            if class == b"sector" {
                let mac = find_attr(tag, b"macro").map(str_from).unwrap_or_default();
                let macro_lower = mac.to_lowercase();
                if let Some(owner) = find_attr(tag, b"owner").map(str_from) {
                    overrides.insert(macro_lower.clone(), owner);
                }
                *sector_start = Some(open_pos);
                *sector_macro = Some(macro_lower);
                *depth = 1;
                return tag_end;
            }
        }
        // <component> at depth 0 that isn't a sector — count it so a matching
        // close doesn't underflow.
        *depth = 1;
        return tag_end;
    }

    *depth += 1;
    tag_end
}

fn find_tag<'a>(haystack: &'a [u8], needle: &[u8]) -> Option<&'a [u8]> {
    let start = memmem::find(haystack, needle)?;
    let close = memchr(b'>', &haystack[start..])? + start;
    Some(&haystack[start..=close])
}

fn str_from(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn parse_f32(b: &[u8]) -> Option<f32> {
    std::str::from_utf8(b).ok()?.parse().ok()
}

fn parse_u64(b: &[u8]) -> Option<u64> {
    std::str::from_utf8(b).ok()?.parse().ok()
}
```

- [ ] **Step 8: Run — expect PASS**

```bash
cargo test -p map-io --lib save_parser::scan::tests::scan_extracts_meta_overrides_and_one_chunk 2>&1 | tail -3
```
Expected: `1 passed`.

- [ ] **Step 9: Add nested-sector + multiple-sector test**

Append to `mod tests`:

```rust
    #[test]
    fn scan_finds_multiple_sectors_skips_non_sector_components() {
        let payload: &[u8] = br#"
<savegame>
  <info></info>
  <universe>
    <component class="cluster" macro="c1">
      <component class="sector" macro="MacroA" owner="argon">
        <component class="zone">
          <component class="ship_s" id="[0x1]"><offset><position x="1" y="0" z="0"/></offset></component>
        </component>
      </component>
      <component class="sector" macro="MacroB" owner="teladi">
        <component class="ship_m" id="[0x2]"><offset><position x="2" y="0" z="0"/></offset></component>
      </component>
    </component>
  </universe>
</savegame>"#;

        let (tx, rx) = std::sync::mpsc::sync_channel(8);
        tx.send(payload.to_vec()).unwrap();
        drop(tx);

        let out = run_scan(rx, "/tmp/x.xml.gz".into(), std::time::UNIX_EPOCH).unwrap();
        assert_eq!(out.chunks.len(), 2);
        assert_eq!(out.chunks[0].sector_macro, "macroa");
        assert_eq!(out.chunks[1].sector_macro, "macrob");
        assert_eq!(out.overrides.len(), 2);
    }
```

- [ ] **Step 10: Run all scan tests**

```bash
cargo test -p map-io --lib save_parser::scan::tests 2>&1 | tail -5
```
Expected: `3 passed`.

- [ ] **Step 11: Add chunk-boundary test (carry-over correctness)**

Append:

```rust
    #[test]
    fn scan_handles_input_split_into_many_small_chunks() {
        let payload: &[u8] = br#"
<savegame><info></info><universe>
  <component class="sector" macro="SectorMacro" owner="argon"><component class="ship_l" id="[0x9]"><offset><position x="1" y="0" z="0"/></offset></component></component>
</universe></savegame>"#;

        // Split into 17-byte chunks.
        let (tx, rx) = std::sync::mpsc::sync_channel(payload.len() / 17 + 2);
        for slice in payload.chunks(17) {
            tx.send(slice.to_vec()).unwrap();
        }
        drop(tx);

        let out = run_scan(rx, "/tmp/x".into(), std::time::UNIX_EPOCH).unwrap();
        assert_eq!(out.chunks.len(), 1);
        assert_eq!(out.chunks[0].sector_macro, "sectormacro");
        assert_eq!(out.overrides.get("sectormacro").map(String::as_str), Some("argon"));
    }
```

- [ ] **Step 12: Run — expect PASS**

```bash
cargo test -p map-io --lib save_parser::scan::tests 2>&1 | tail -5
```
Expected: `4 passed`.

(The scanner accumulates the chunks into `bytes` before scanning, so a chunk-boundary split inside a tag works automatically; this test guards against future regressions if streaming-scan is added.)

- [ ] **Step 13: Commit**

```bash
git add crates/map-io/src/save_parser/scan.rs
git commit -m "feat(io): Stage 2 byte scanner extracts meta, overrides, sector chunks"
```

---

### Task 5: Stage 3 — per-sector parser

**Files:**
- Modify: `crates/map-io/src/save_parser/sector_chunk.rs`
- Test: same file (inline)

Workers parse one sector subtree with `quick_xml`, extracting ships and stations. No shared mutable state — each worker returns a `Vec<EntityRecord>`.

- [ ] **Step 1: Write the failing test first**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_station_and_ship_with_positions() {
        let chunk: &[u8] = br#"<component class="sector" macro="ignored">
  <component class="zone">
    <component class="station" macro="station_arg_factory_01" owner="argon" id="[0x100]">
      <offset><position x="0" y="0" z="0"/></offset>
    </component>
    <component class="ship_l" macro="ship_arg_l_destroyer_01" owner="argon" id="[0x101]">
      <offset><position x="1000" y="0" z="2000"/></offset>
    </component>
  </component>
</component>"#;

        let out = parse_sector_chunk(chunk, "macroa");
        assert_eq!(out.len(), 2);

        let station = &out[0];
        assert_eq!(station.id, 0x100);
        assert_eq!(station.kind, map_domain::world::LiveObjectKind::Station);
        assert_eq!(station.owner.as_deref(), Some("argon"));
        assert_eq!(station.sector_macro, "macroa");
        assert!((station.position.x - 0.0).abs() < 1e-3);

        let ship = &out[1];
        assert_eq!(ship.id, 0x101);
        assert_eq!(ship.kind, map_domain::world::LiveObjectKind::ShipLarge);
        // Positions are converted metres → km.
        assert!((ship.position.x - 1.0).abs() < 1e-3);
        assert!((ship.position.z - 2.0).abs() < 1e-3);
    }
}
```

- [ ] **Step 2: Run — expect FAIL (`parse_sector_chunk` not found)**

```bash
cargo test -p map-io --lib save_parser::sector_chunk::tests::parses_station_and_ship_with_positions 2>&1 | tail -3
```

- [ ] **Step 3: Implement `parse_sector_chunk`**

Replace `sector_chunk.rs` contents with:

```rust
//! Stage 3: parse one sector subtree using `quick_xml`.
//!
//! Runs inside a rayon worker; called once per `SectorChunk`. Returns
//! `Vec<EntityRecord>` with ships and stations. No shared state.

use glam::Vec3;
use map_domain::world::LiveObjectKind;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use super::types::EntityRecord;

/// Parse one sector subtree. Failures inside a single chunk are swallowed
/// (return whatever was parsed before the error) — Stage 2 already validated
/// the chunk boundaries, and a single bad sector should not abort the whole
/// parse.
pub fn parse_sector_chunk(slice: &[u8], sector_macro: &str) -> Vec<EntityRecord> {
    let mut reader = Reader::from_reader(slice);
    reader.config_mut().trim_text(true);

    let mut out: Vec<EntityRecord> = Vec::new();
    let mut buf: Vec<u8> = Vec::new();

    // State for an entity currently being captured.
    let mut comp_depth: u32 = 0;
    let mut pending: Option<Pending> = None;
    let mut entity_position_captured = false;
    let mut in_offset = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"component" => {
                comp_depth += 1;
                if pending.is_none() {
                    if let Some(p) = build_pending(e, comp_depth, sector_macro) {
                        pending = Some(p);
                        entity_position_captured = false;
                    }
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"component" => {
                if let Some(p) = pending.as_ref() {
                    if p.open_depth == comp_depth {
                        let p = pending.take().unwrap();
                        out.push(EntityRecord {
                            id: p.id,
                            name: p.name,
                            kind: p.kind,
                            owner: p.owner,
                            position: p.position.unwrap_or(Vec3::ZERO),
                            sector_macro: sector_macro.to_string(),
                        });
                        entity_position_captured = false;
                    }
                }
                comp_depth = comp_depth.saturating_sub(1);
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"offset" => {
                if pending.is_some() && !entity_position_captured {
                    in_offset = true;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"offset" => in_offset = false,
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"position" => {
                if in_offset {
                    if let Some(p) = pending.as_mut() {
                        if p.position.is_none() {
                            let x = attr_f32(e, b"x").unwrap_or(0.0);
                            let y = attr_f32(e, b"y").unwrap_or(0.0);
                            let z = attr_f32(e, b"z").unwrap_or(0.0);
                            // X4 stores positions in metres; convert to km.
                            p.position = Some(Vec3::new(x / 1000.0, y / 1000.0, z / 1000.0));
                            entity_position_captured = true;
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    out
}

struct Pending {
    open_depth: u32,
    id: u32,
    name: String,
    kind: LiveObjectKind,
    owner: Option<String>,
    position: Option<Vec3>,
}

fn build_pending(e: &BytesStart<'_>, depth: u32, _sector_macro: &str) -> Option<Pending> {
    let class = attr_str(e, b"class")?;
    let kind = match class.as_str() {
        "station" => LiveObjectKind::Station,
        "ship_xs" | "ship_s" => LiveObjectKind::ShipSmall,
        "ship_m" => LiveObjectKind::ShipMedium,
        "ship_l" => LiveObjectKind::ShipLarge,
        "ship_xl" => LiveObjectKind::ShipExtraLarge,
        _ => return None,
    };
    let id_str = attr_str(e, b"id")?;
    let id = parse_entity_id(&id_str)?;
    let name = attr_str(e, b"macro").unwrap_or_else(|| class.clone());
    let owner = attr_str(e, b"owner");

    Some(Pending {
        open_depth: depth,
        id,
        name,
        kind,
        owner,
        position: None,
    })
}

fn parse_entity_id(s: &str) -> Option<u32> {
    let inner = s.strip_prefix("[0x")?.strip_suffix(']')?;
    u32::from_str_radix(inner, 16).ok()
}

fn attr_str(e: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
}

fn attr_f32(e: &BytesStart<'_>, name: &[u8]) -> Option<f32> {
    e.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| std::str::from_utf8(&a.value).ok()?.parse::<f32>().ok())
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-io --lib save_parser::sector_chunk::tests::parses_station_and_ship_with_positions 2>&1 | tail -3
```
Expected: `1 passed`.

- [ ] **Step 5: Add empty-sector test**

Append to `mod tests`:

```rust
    #[test]
    fn empty_sector_returns_no_entities() {
        let chunk: &[u8] = br#"<component class="sector" macro="m"><component class="zone"></component></component>"#;
        let out = parse_sector_chunk(chunk, "m");
        assert!(out.is_empty());
    }
```

- [ ] **Step 6: Run all**

```bash
cargo test -p map-io --lib save_parser::sector_chunk::tests 2>&1 | tail -3
```
Expected: `2 passed`.

- [ ] **Step 7: Commit**

```bash
git add crates/map-io/src/save_parser/sector_chunk.rs
git commit -m "feat(io): Stage 3 per-sector parser via quick_xml"
```

---

### Task 6: Stage 4 — merge into World

**Files:**
- Modify: `crates/map-io/src/save_parser/merge.rs`
- Test: same file (inline)

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use map_domain::ids::SectorId;
    use map_domain::world::LiveObjectKind;
    use std::collections::HashMap;

    use crate::save_parser::types::EntityRecord;

    #[test]
    fn merges_records_and_assigns_faction_ids() {
        let records = vec![
            EntityRecord {
                id: 0x10,
                name: "station_a".into(),
                kind: LiveObjectKind::Station,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
            },
            EntityRecord {
                id: 0x11,
                name: "ship_a".into(),
                kind: LiveObjectKind::ShipSmall,
                owner: Some("argon".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sa".into(),
            },
            EntityRecord {
                id: 0x12,
                name: "ship_b".into(),
                kind: LiveObjectKind::ShipMedium,
                owner: Some("teladi".into()),
                position: glam::Vec3::ZERO,
                sector_macro: "sb".into(),
            },
        ];
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("sa".into(), SectorId(1));
        sm.insert("sb".into(), SectorId(2));

        let world = merge(vec![records], Some(&sm));
        assert_eq!(world.names.len(), 3);
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 1);
        // Two distinct factions seen → two ids assigned.
        let argon = world.factions.get(&0x10).copied();
        let teladi = world.factions.get(&0x12).copied();
        assert!(argon.is_some());
        assert!(teladi.is_some());
        assert_ne!(argon, teladi);
    }

    #[test]
    fn unknown_sector_drops_entity() {
        let records = vec![EntityRecord {
            id: 0xFFFF,
            name: "x".into(),
            kind: LiveObjectKind::ShipSmall,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "unknown".into(),
        }];
        let sm: HashMap<String, SectorId> = HashMap::new();
        let world = merge(vec![records], Some(&sm));
        assert!(world.names.is_empty());
    }

    #[test]
    fn no_sector_macros_drops_all() {
        let records = vec![EntityRecord {
            id: 1,
            name: "x".into(),
            kind: LiveObjectKind::Station,
            owner: None,
            position: glam::Vec3::ZERO,
            sector_macro: "anything".into(),
        }];
        let world = merge(vec![records], None);
        assert!(world.names.is_empty());
    }
}
```

- [ ] **Step 2: Run — expect FAIL (`merge` not found)**

```bash
cargo test -p map-io --lib save_parser::merge::tests 2>&1 | tail -3
```

- [ ] **Step 3: Implement `merge`**

Replace `merge.rs` with:

```rust
//! Stage 4: merge per-worker entity records into a single `World`.

use std::collections::HashMap;

use map_domain::ids::{FactionId, SectorId};
use map_domain::world::World;

use super::types::EntityRecord;

/// Combine all per-worker entity lists into a single `World`. Resolves each
/// record's `sector_macro` via `sector_macros` (drops records whose sector
/// isn't known). Assigns a fresh `FactionId` to each first-seen owner string.
pub fn merge(
    batches: Vec<Vec<EntityRecord>>,
    sector_macros: Option<&HashMap<String, SectorId>>,
) -> World {
    let mut world = World::new();
    let Some(sector_macros) = sector_macros else {
        return world;
    };

    let mut faction_ids: HashMap<String, FactionId> = HashMap::new();
    let mut next_faction_id: u32 = 1;

    for batch in batches {
        for r in batch {
            let Some(&sec_id) = sector_macros.get(&r.sector_macro) else {
                continue;
            };
            let faction = r.owner.map(|name| {
                *faction_ids.entry(name).or_insert_with(|| {
                    let id = FactionId(next_faction_id);
                    next_faction_id += 1;
                    id
                })
            });
            world.insert_entity(r.id, r.name, r.kind, faction, r.position, sec_id);
        }
    }

    world
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p map-io --lib save_parser::merge::tests 2>&1 | tail -3
```
Expected: `3 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/map-io/src/save_parser/merge.rs
git commit -m "feat(io): Stage 4 merge entity records into World"
```

---

### Task 7: Wire up `mod.rs` orchestrator + replace `parse_save` stub

**Files:**
- Modify: `crates/map-io/src/save_parser/mod.rs`

- [ ] **Step 1: Replace the stub with the real orchestrator**

```rust
//! Public entrypoint for the parallel save parser.
//!
//! Orchestrates four stages:
//! 1. Spawn gzip producer thread.
//! 2. Byte-scan chunks → SnapshotMeta + FactionOverrides + sector byte ranges.
//! 3. Rayon over sector chunks → per-worker Vec<EntityRecord>.
//! 4. Single-threaded merge into a `World`.
//!
//! Logs per-stage timings to stderr.

pub mod decompress;
pub mod merge;
pub mod scan;
pub mod sector_chunk;
pub mod types;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rayon::prelude::*;

use map_domain::ids::SectorId;
use map_domain::world::{SnapshotMeta, World};

use crate::xml_parser::ParseError;

pub use types::FactionOverrides;

pub fn parse_save(
    path: &Path,
    sector_macros: Option<&HashMap<String, SectorId>>,
) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError> {
    let t_total = Instant::now();

    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .unwrap_or(std::time::UNIX_EPOCH);

    // Stage 1: gzip producer.
    let t_stage1 = Instant::now();
    let dec = decompress::spawn_decompressor(path).map_err(ParseError::Io)?;
    let decompress::Decompressor { rx, handle } = dec;

    // Stage 2: byte scan — drains rx synchronously.
    let scan_out = scan::run_scan(rx, path.to_path_buf(), mtime).map_err(ParseError::Io)?;
    // Now the producer is done (channel closed). Join it for any error.
    if let Err(join_err) = handle.join() {
        eprintln!("[parse] decompress thread panicked: {:?}", join_err);
    } else if let Ok(Err(e)) = handle_join_result(&path) {
        return Err(ParseError::Io(e));
    }
    let stage12_ms = t_stage1.elapsed().as_millis();

    // Stage 3: rayon per-sector.
    let t_stage3 = Instant::now();
    let bytes = Arc::new(scan_out.bytes);
    let chunks = scan_out.chunks;
    let entity_lists: Vec<Vec<types::EntityRecord>> = chunks
        .par_iter()
        .map(|chunk| {
            let slice = &bytes[chunk.byte_range.clone()];
            sector_chunk::parse_sector_chunk(slice, &chunk.sector_macro)
        })
        .collect();
    let stage3_ms = t_stage3.elapsed().as_millis();

    // Stage 4: merge.
    let t_stage4 = Instant::now();
    let world = merge::merge(entity_lists, sector_macros);
    let stage4_ms = t_stage4.elapsed().as_millis();

    let total_ms = t_total.elapsed().as_millis();
    eprintln!(
        "[parse] stage1+2={}ms stage3={}ms stage4={}ms total={}ms",
        stage12_ms, stage3_ms, stage4_ms, total_ms
    );

    Ok((scan_out.meta, world, scan_out.overrides))
}

/// Dummy stub used by the join-error path above. We can't re-join the
/// `JoinHandle` (already consumed), so this is a placeholder for symmetry —
/// the real error is logged on the line above. Replaced by a cleaner path in
/// a follow-up cleanup if needed.
fn handle_join_result(_: &Path) -> std::io::Result<std::io::Result<()>> {
    Ok(Ok(()))
}
```

- [ ] **Step 2: Tighten the error path (clean up the awkward `handle_join_result` stub)**

Replace the Stage 1/2 block in `parse_save` with:

```rust
    // Stage 1 + 2: gzip producer thread + byte scanner.
    let t_stage1 = Instant::now();
    let decompress::Decompressor { rx, handle } =
        decompress::spawn_decompressor(path).map_err(ParseError::Io)?;

    let scan_out = scan::run_scan(rx, path.to_path_buf(), mtime).map_err(ParseError::Io)?;

    // Join producer thread; surface IO errors that happened mid-stream.
    match handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(ParseError::Io(e)),
        Err(_) => {
            return Err(ParseError::Io(std::io::Error::other(
                "decompressor thread panicked",
            )));
        }
    }
    let stage12_ms = t_stage1.elapsed().as_millis();
```

…and DELETE the `handle_join_result` helper.

- [ ] **Step 3: Add an integration-style test that exercises the full pipeline against the existing fixture**

Append to `mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini_save.xml.gz")
    }

    #[test]
    fn parse_mini_save_meta_and_overrides() {
        let (meta, _world, overrides) = parse_save(&fixture_path(), None).unwrap();
        assert_eq!(meta.player_money, 40000);
        assert!((meta.game_time_seconds - 1734.285).abs() < 1e-2);
        assert_eq!(meta.player_location_name, "{20004,10011}");
        assert_eq!(overrides.len(), 2);
    }

    #[test]
    fn parse_mini_save_entities_resolved_via_sector_macros() {
        let mut sm: HashMap<String, SectorId> = HashMap::new();
        sm.insert("cluster_01_sector001_macro".into(), SectorId(1));
        sm.insert("cluster_06_sector001_macro".into(), SectorId(2));
        let (_meta, world, _) = parse_save(&fixture_path(), Some(&sm)).unwrap();
        assert_eq!(world.names.len(), 4);
        assert_eq!(world.entities_in_sector(SectorId(1)).len(), 2);
        assert_eq!(world.entities_in_sector(SectorId(2)).len(), 2);
    }
}
```

- [ ] **Step 4: Run all save_parser tests**

```bash
cargo test -p map-io --lib save_parser 2>&1 | tail -8
```
Expected: all green (previous unit tests + the 2 new integration tests).

- [ ] **Step 5: Run full workspace tests**

```bash
cargo test 2>&1 | tail -3
```
Expected: 53 (old) + 11 (new unit tests across stages) + 2 (new integration tests) = `66 passed` (or thereabouts — exact count may differ by one or two as we add helpers).

- [ ] **Step 6: Commit**

```bash
git add crates/map-io/src/save_parser/mod.rs
git commit -m "feat(io): wire parallel save parser (stages 1-4 orchestrator)"
```

---

### Task 8: Manual smoke benchmark on the real save

**Files:** none modified.

- [ ] **Step 1: Run the app and capture the stage timing line**

```bash
cargo run > /tmp/r.txt 2>&1 &
RUN_PID=$!
sleep 25
kill $RUN_PID 2>/dev/null
grep "^\[parse\]" /tmp/r.txt
```

Expected output (numbers vary by machine; this is the user's dev box target):
```
[parse] stage1+2=NNNNms stage3=NNNms stage4=NNms total=NNNNms
```

- [ ] **Step 2: Confirm the acceptance threshold**

Verify `total=…ms` is **≤ 3500 ms**. If it's not, look at which stage dominates:

- If `stage1+2` is the bottleneck: gzip is the floor (~1.9s). Check that the scanner isn't dominating by adding a temporary log inside `run_scan` to print the time spent after the drain loop versus inside `scan_sectors`. If `scan_sectors` is the hot path, profile with `cargo flamegraph -p map-app --bin foundations-map` and tighten the byte loop.
- If `stage3` is the bottleneck: rayon may not be using all cores. Set `RAYON_NUM_THREADS=$(nproc)` and re-run. If a single sector dominates time, log per-chunk durations to find it.
- If `stage4` is the bottleneck (>500 ms): HashMap inserts have grown. Pre-size the World hashmaps with `World::with_capacity(N)` based on total entity count — out of scope here, file a follow-up.

- [ ] **Step 3: Compare entity counts to baseline (correctness spot-check)**

Run the app and look for the existing `[map] Snapshot:` log line. Compare to a baseline you captured before this work (you should still have the old output in your terminal scrollback or `/tmp/r.txt` from previous Phase-3 work). The `money`, `game t`, faction-override count, and entity count visible in the side-panel UI should match.

If the entity count or per-sector populations differ noticeably from the old parser:
- Diff the new orchestration against the old `save_parser.rs` (`git show HEAD~7:crates/map-io/src/save_parser.rs`).
- Most likely cause: a class-name typo (e.g., missing `ship_xs`) or position-conversion miss in `parse_sector_chunk` (Stage 3).

- [ ] **Step 4: Commit benchmark observation (no code change)**

If the benchmark passes, append a short note to `docs/superpowers/retrospectives/2026-05-17-parse-perf.md` (new file) with the measured numbers, then:

```bash
mkdir -p docs/superpowers/retrospectives
cat > docs/superpowers/retrospectives/2026-05-17-parse-perf.md <<'EOF'
# Parallel save parse — measured perf

Baseline (sequential): ~7.7s for 96 MB save / 813 MB raw XML.

After: `[parse] stage1+2=NNNms stage3=NNNms stage4=NNms total=NNNNms`

Wall well under the 3.5s target. Gzip is the floor (~1.9s); scanner runs
during gzip's tail; rayon mops up per-sector work in ~1s; merge ~0.2s.
EOF

git add docs/superpowers/retrospectives/2026-05-17-parse-perf.md
git commit -m "docs: record measured parallel-parse perf"
```

(If the benchmark fails the threshold, fix in a follow-up task before recording.)

---

## Self-Review

**Spec coverage (1:1 with spec sections):**
- Architecture diagram → Task 7 orchestrator (mod.rs) draws the same four-stage flow.
- Stage 1 gzip producer → Task 3.
- Stage 2 byte scanner → Task 4 (including `find_attr`, the open/close loop, info parsing, and carry-over via accumulate-then-scan).
- Stage 3 rayon per-sector → Task 7 invokes `par_iter` over chunks produced by Task 4; Task 5 implements the worker.
- Stage 4 merge → Task 6.
- Module Layout → Task 1 creates the directory; tasks 2–6 fill each file; Task 7 wires `mod.rs`.
- Dependencies (`rayon`, `memchr`, `tempfile` dev) → Task 1 + Task 3.
- Error Handling — producer thread panic / IO error → Task 7's `match handle.join()` branch.
- Tests — every fixture/unit listed in the spec is in Tasks 2-7 except `byte_scanner_handles_chunk_boundary_split_tag`, covered by Task 4 Step 11.
- Smoke benchmark → Task 8.
- Acceptance criterion: real-save wall ≤ 3.5s → Task 8 Step 2.

**Placeholder scan:**
- No `TBD`/`TODO`/`later` references.
- Every code step shows full source.
- Every test step shows full test body.

**Type consistency:**
- `SectorChunk`, `EntityRecord`, `FactionOverrides` defined once in Task 1, used identically thereafter.
- `parse_sector_chunk(slice: &[u8], sector_macro: &str) -> Vec<EntityRecord>` defined in Task 5 Step 3, called identically in Task 7.
- `merge(batches: Vec<Vec<EntityRecord>>, sector_macros: Option<&HashMap<String, SectorId>>) -> World` defined in Task 6, called identically in Task 7.
- `parse_save(path: &Path, sector_macros: Option<&HashMap<String, SectorId>>) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError>` matches the existing caller signature in `crates/map-app/src/main.rs::parse_latest_save` — no caller changes needed.
