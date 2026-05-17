# Parallel Save Parsing

**Date:** 2026-05-17
**Goal:** Cut X4 save-parse wall time from ~7.7s to ~3s for the user's real save (96 MB gzip / ~813 MB raw / 35M XML events / ~50k entities).

## Why

Profile (release, user's `quicksave.xml.gz`):

| Phase | Time | Parallelizable |
|---|---|---|
| gzip decompress | 1.9s | No (sequential gzip); overlappable |
| XML tokenize (full) | 1.6s | No (sequential XML); per-subtree yes |
| Our event handling + World build | ~4s | Yes — per-sector independent |

`tokio` doesn't help — work is CPU-bound, not async I/O. `std::thread` + `rayon` are the right tools.

User scope: no persistent cache. Only the live parse path matters.

## Architecture

Three stages, two layers of parallelism:

```
                  ┌────────────────────────────────────────────────────┐
                  │ Stage 1 (thread G)        Stage 2 (caller thread)  │
                  │ gzip → Vec<u8> chunks ──► tokenize: extract        │
                  │                              meta + overrides +     │
                  │                              per-sector byte slices │
                  │ overlap: max(1.9s, 2s)                              │
                  └─────────────────────┬──────────────────────────────┘
                                        │ Arc<Vec<u8>> + Vec<SectorChunk>
                                        ▼
                  ┌────────────────────────────────────────────────────┐
                  │ Stage 3 (rayon pool, ~N cores)                     │
                  │ par_iter sector chunks → per-worker mini-parse →   │
                  │   Vec<EntityRecord>                                │
                  │ wall: ~1s on 4-8 cores                             │
                  └─────────────────────┬──────────────────────────────┘
                                        │ Vec<Vec<EntityRecord>>
                                        ▼
                  ┌────────────────────────────────────────────────────┐
                  │ Stage 4 (caller thread)                            │
                  │ flatten → resolve SectorId via sector_macros →     │
                  │ World::insert_entity in loop                       │
                  │ wall: ~0.2s                                        │
                  └────────────────────────────────────────────────────┘
```

Expected wall: max(1.9, 2.0) + 1.0 + 0.2 ≈ **3.2s** (down from 7.7s).

## Stage 1 — gzip producer

`std::thread::spawn` consumer; main thread runs Stage 2. Producer:

```rust
fn spawn_decompressor(path: &Path) -> (mpsc::SyncSender<Vec<u8>>, JoinHandle<io::Result<()>>) {
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(32);
    let path = path.to_path_buf();
    let h = std::thread::spawn(move || {
        let mut gz = GzDecoder::new(File::open(path)?);
        loop {
            let mut chunk = vec![0u8; 64 * 1024];
            let n = gz.read(&mut chunk)?;
            if n == 0 { break; }
            chunk.truncate(n);
            if tx.send(chunk).is_err() { break; }
        }
        Ok(())
    });
    (tx, h)
}
```

Returns a `Receiver<Vec<u8>>` (extracted from the tx); caller wraps in a `ChunkReader` implementing `std::io::Read` that drains chunks, then feeds to `quick_xml::Reader`.

Backpressure: bounded queue size 32 chunks × 64 KB = 2 MB buffer. Producer blocks when consumer falls behind.

Bonus: Stage 2 needs full decompressed bytes for Stage 3 (per-sector random access). `ChunkReader` has dual role: implement `Read` for quick_xml AND accumulate every byte handed out into an owned `accumulated: Vec<u8>`. After Stage 2 finishes, caller takes the buffer (`reader.into_buffer() -> Vec<u8>`) and passes it to Stage 3 wrapped in `Arc<Vec<u8>>`. Memory peak: ~813 MB.

Crucially, byte positions reported by `quick_xml::Reader::buffer_position()` must match offsets in `accumulated` (they will, because both count bytes consumed from the input stream — chunk boundaries are invisible to the XML reader).

## Stage 2 — tokenize, extract slices

Single quick_xml pass over the chunk-fed reader. State:

- `comp_depth: u32` — `<component>` nesting depth
- `sector_open_depth: Option<u32>` + `sector_macro: Option<String>` + `sector_start_pos: usize`
- `overrides: HashMap<String, String>` — sector → owner
- `meta: SnapshotMeta` — from `<info>` (existing logic)
- `chunks: Vec<SectorChunk>` — appended on sector close

```rust
struct SectorChunk {
    sector_macro: String,    // lowercase
    byte_range: Range<usize>, // into the decompressed buffer
}
```

When entering a `<component class="sector" macro="…" owner="…">`:
- `sector_start_pos = reader.buffer_position()`
- Record macro + owner

When leaving (matching `</component>`):
- `chunks.push(SectorChunk { sector_macro, byte_range: sector_start_pos..reader.buffer_position() })`

No entity work in Stage 2. Skip all non-sector class checks (cheap).

End of Stage 2: have `(meta, overrides, chunks, decompressed_bytes)`.

## Stage 3 — rayon per-sector

```rust
let buf = Arc::new(decompressed_bytes);
let entity_lists: Vec<Vec<EntityRecord>> = chunks.par_iter()
    .map(|chunk| {
        let slice = &buf[chunk.byte_range.clone()];
        parse_sector_chunk(slice, &chunk.sector_macro)
    })
    .collect();
```

`parse_sector_chunk(slice, sector_macro) -> Vec<EntityRecord>`:
- Fresh `quick_xml::Reader::from_reader(slice)`
- Walks once, extracts ships+stations with position/owner/kind/id
- Returns plain records; no SectorId resolution yet (worker doesn't have the map)

`EntityRecord`:
```rust
struct EntityRecord {
    id: u32,              // parsed from "[0xHEX]"
    name: String,         // macro string
    kind: LiveObjectKind,
    owner: Option<String>,
    position: Vec3,       // already km
    sector_macro: String, // for caller to resolve
}
```

rayon's default pool uses all logical cores. Override with `RAYON_NUM_THREADS` env if needed.

## Stage 4 — merge into World

```rust
let mut faction_ids: HashMap<String, FactionId> = HashMap::new();
let mut next_id: u32 = 1;
let mut world = World::new();
for batch in entity_lists {
    for e in batch {
        let Some(&sec_id) = sector_macros.get(&e.sector_macro) else { continue };
        let faction = e.owner.map(|name| *faction_ids.entry(name).or_insert_with(|| {
            let id = FactionId(next_id);
            next_id += 1;
            id
        }));
        world.insert_entity(e.id, e.name, e.kind, faction, e.position, sec_id);
    }
}
```

Sequential. ~50k inserts × 4µs ≈ 200ms.

## Module Layout

Rename existing `save_parser.rs` to `save_parser/mod.rs`, split into:

```
crates/map-io/src/save_parser/
    mod.rs              — public parse_save (+ backward-compat shim if needed)
    decompress.rs       — Stage 1 producer + ChunkReader
    extract.rs          — Stage 2 (meta + overrides + sector chunks)
    sector_chunk.rs     — Stage 3 worker (parse_sector_chunk)
    merge.rs            — Stage 4 (World assembly)
    types.rs            — SectorChunk, EntityRecord, FactionOverrides
```

`mod.rs` orchestrates the four stages, exposes the same `pub fn parse_save(path, sector_macros) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError>`.

## Dependency Changes

Add to `crates/map-io/Cargo.toml`:
```toml
rayon = "1"
```

No other new deps.

## Error Handling

- Producer thread error (IO/gzip failure) → propagate to caller via `JoinHandle` after Stage 2 detects channel close. Map to `ParseError::Io`.
- Per-sector chunk parse failure (malformed XML inside one sector): log + skip that sector's entities. Other sectors still load. Failure doesn't abort the whole parse.
- Empty save / no sectors found: returns empty World, empty overrides, valid meta. Same as today.

## Memory Profile

- Decompressed bytes: ~813 MB held in Arc<Vec<u8>> for Stage 3.
- Chunk index: ~144 entries × ~50 bytes = ~7 KB.
- Per-worker entity lists: ~6 entries per worker on average (50k entities / 144 sectors).
- Final World: ~10 MB (positions + names + factions).
- Peak: ~830 MB during parse. Released after Stage 4 (Arc drops when last worker finishes).

Acceptable on dev machine (32 GB) and target users (8 GB+). For lower-memory machines, future optimization: stream Stage 3 (drop sectors of buf as they're processed) — out of scope.

## Failure Modes

| Failure | Behavior |
|---|---|
| File not found | Existing `ParseError::Io` propagates |
| Gzip CRC error mid-stream | Stage 1 thread fails; Stage 2 sees short read; ParseError::Io |
| Malformed XML in info block | ParseError; whole parse aborts (same as today) |
| Malformed XML in one sector chunk | Skip that sector; log; other sectors succeed |
| Sector macro unknown (not in `sector_macros` map) | Entity skipped silently (same as today) |
| OOM on Stage 1 | Producer panics — std::thread propagates as `Err` from JoinHandle |

## Tests

- All 53 existing tests still pass.
- New tests in `save_parser/tests.rs`:
  - `parse_mini_save_parallel_matches_sequential` — golden test: parse the mini_save fixture, assert same World size + same faction count.
  - `parse_sector_chunk_extracts_ships_and_stations` — unit test on a hand-crafted sector subtree byte slice.
  - `decompress_producer_handles_partial_reads` — unit test with a small in-memory gzip stream.
- Smoke benchmark: log wall time of each stage `[parse] stage1=Nms stage2=Nms stage3=Nms stage4=Nms total=Nms` on real save. Should show total < 4000 ms on the user's machine.

## Acceptance Criteria

- [ ] `cargo test` passes (all 53 + new tests)
- [ ] Real-save wall time ≤ 4 s on the user's machine (currently 7.7 s)
- [ ] No new compile warnings
- [ ] Peak RAM during parse ≤ 1.2 GB (measured loosely; not enforced)
- [ ] Same World contents as the sequential parser for the same input (mini_save fixture and real save spot-check via `world.names.len()`)

## Out of Scope

- Persistent cache (user opted out)
- Incremental diff vs prior snapshot
- Switching XML library
- mmap of decompressed bytes
- Lower memory streaming Stage 3
