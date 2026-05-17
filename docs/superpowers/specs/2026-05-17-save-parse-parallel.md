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
                  │ gzip → Vec<u8> chunks ──► fast byte scan:           │
                  │                              meta (regex on info)   │
                  │                              + overrides             │
                  │                              + per-sector byte ranges│
                  │ overlap: max(1.9s, ~0.5s) ≈ 1.9s                    │
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

Expected wall: max(1.9, 0.5) + 1.0 + 0.2 ≈ **3.1s** (down from 7.7s; gzip is the floor).

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

Returns a `Receiver<Vec<u8>>` (extracted from the tx); caller drains chunks straight into a single growing `accumulated: Vec<u8>` and runs the byte scanner over the tail as new bytes arrive (carry-over handling described in Stage 2).

Backpressure: bounded queue size 32 chunks × 64 KB = 2 MB buffer. Producer blocks when consumer falls behind.

The `accumulated: Vec<u8>` becomes the input for Stage 3 (wrapped in `Arc<Vec<u8>>`). Memory peak: ~813 MB.

## Stage 2 — fast byte scanner

Hand-rolled byte-level scanner over the chunk-fed buffer. Does NOT use quick_xml. Three things to extract:

1. `SnapshotMeta` from the `<info>` block
2. `overrides: HashMap<String, String>` — sector_macro → owner
3. `chunks: Vec<SectorChunk>` — byte ranges of each `<component class="sector"…>…</component>` subtree

```rust
struct SectorChunk {
    sector_macro: String,    // lowercase
    byte_range: Range<usize>, // into the accumulated buffer
}
```

### Algorithm

Maintain a cursor `pos` over `accumulated`. As bytes arrive from Stage 1, advance the cursor; carry-over partial-tag state across chunk boundaries by retaining the index of the last `<` we haven't yet matched a closing `>` for.

Three patterns to detect via `memchr::memchr(b'<', &buf[pos..])`:

- `<component class="sector"…>` — extract `macro="…"` and (optional) `owner="…"` substrings from inside the tag (search `macro=\"` / `owner=\"`, take until next `\"`). Push `(macro_lowercase, owner)` into overrides. Record `sector_start = pos`, `sector_macro = macro_lower`. Increment `comp_depth` to 1.
- `<component ` (any other class, or any other component nesting inside) — increment `comp_depth`. Don't parse attrs.
- `</component>` — decrement `comp_depth`. If `comp_depth == 0` and we were inside a sector, emit `SectorChunk { sector_macro, byte_range: sector_start..end_of_tag }` and clear sector state.

For `<info>` extraction: same byte-scan approach, look for one-off needles `<game ` and `<player `, extract `time="…"`, `version="…"`, `build="…"`, `money="…"`, `location="…"`. Stop info scanning once we see `</info>`.

### Implementation note

Use a tiny helper:

```rust
fn find_attr<'a>(tag: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    // Build needle: b"<name>=\""  but just  name + b"=\""
    let mut needle = name.to_vec();
    needle.extend_from_slice(b"=\"");
    let start = twoway_find(tag, &needle)? + needle.len();
    let rest = &tag[start..];
    let end = memchr::memchr(b'"', rest)?;
    Some(&rest[..end])
}
```

(`twoway_find` = the `memchr::memmem::find` two-way matcher; already in the `memchr` crate.)

### Validity assumptions

- X4 saves are machine-generated and well-formed: no XML comments containing literal `<component`, no CDATA sections, no escape sequences inside `class="sector"` tag text, no `<component` tokens inside attribute values.
- Tag opens always close on the same line region (no embedded newlines mid-attribute that span chunk boundaries within carry-over budget — keep carry-over to a generous 8 KB).
- If any assumption breaks, scanner produces no chunks for the affected sector → its entities don't load. Single sector lost, not whole parse. We log a warning when `comp_depth` is unbalanced at EOF.

### Output

End of Stage 2: `(meta, overrides, chunks, accumulated_buf)`. No quick_xml objects.

### Why faster

quick_xml's per-event handling: ~6 ns/event × 35 M events ≈ 210 ms minimum overhead, plus our match arms add allocation pressure → ~2 s total. Byte scan with memchr only handles the `<` density (~2.8 M tags), each compared against ≤ 3 fixed needles → expected ~0.5 s on 813 MB.

### New dependency

Add `memchr = "2"` to `crates/map-io/Cargo.toml`.

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
    mod.rs              — public parse_save; orchestrates stages
    decompress.rs       — Stage 1 producer (gzip → mpsc)
    scan.rs             — Stage 2 byte scanner (meta + overrides + sector chunks)
    sector_chunk.rs     — Stage 3 worker (parse_sector_chunk uses quick_xml on the small slice)
    merge.rs            — Stage 4 (World assembly)
    types.rs            — SectorChunk, EntityRecord, FactionOverrides
```

`mod.rs` exposes the same `pub fn parse_save(path, sector_macros) -> Result<(SnapshotMeta, World, FactionOverrides), ParseError>`.

Stage 3 keeps using quick_xml for its per-sector parse — sectors are small (~5 KB each on average), per-sector cost is dominated by allocation not tokenization, and quick_xml's correctness handles whatever odd attribute orderings appear inside.

## Dependency Changes

Add to `crates/map-io/Cargo.toml`:
```toml
rayon = "1"
memchr = "2"
```

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
  - `parse_mini_save_parallel_matches_sequential` — golden: parse the mini_save fixture, assert same World size + same faction count + same SnapshotMeta values as the existing (now-deleted) sequential parser produced (use a baseline saved-in-test as the golden numbers).
  - `parse_sector_chunk_extracts_ships_and_stations` — unit on a hand-crafted sector subtree byte slice.
  - `decompress_producer_handles_partial_reads` — unit with a small in-memory gzip stream.
  - `byte_scanner_finds_all_sector_chunks` — unit on a synthetic XML buffer with 5 nested sectors; assert exact count + byte ranges.
  - `byte_scanner_extracts_owner_and_macro` — unit on a single `<component class="sector" macro="x" owner="y">…</component>` fragment.
  - `byte_scanner_handles_chunk_boundary_split_tag` — fuzz-style: split same input at every byte boundary, assert same output. (Tests carry-over correctness.)
- Smoke benchmark: log wall time of each stage `[parse] stage1=Nms stage2=Nms stage3=Nms stage4=Nms total=Nms` on real save. Should show total < 4000 ms on the user's machine.

## Acceptance Criteria

- [ ] `cargo test` passes (all 53 + new tests)
- [ ] Real-save wall time ≤ **3.5 s** on the user's machine (currently 7.7 s; target derived from gzip floor 1.9 s + rayon ~1 s + merge ~0.2 s + ~0.4 s margin)
- [ ] No new compile warnings
- [ ] Peak RAM during parse ≤ 1.2 GB (measured loosely; not enforced)
- [ ] Same World contents as the sequential parser for the same input (mini_save fixture and real save spot-check via `world.names.len()`)

## Out of Scope

- Persistent cache (user opted out)
- Incremental diff vs prior snapshot
- Switching XML library
- mmap of decompressed bytes
- Lower memory streaming Stage 3
