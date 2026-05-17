# Parallel save parse — measured perf

**Date:** 2026-05-17
**Save:** real `quicksave.xml.gz`, 96 MB compressed / 813 MB raw / 35 M XML events / 145 sectors / 12.2k entities

## Baseline (sequential, single-threaded parser)

~7.7 s wall.

## After parallel pipeline (this work)

```
[parse] stage1+2=1826ms stage3=275ms stage4=43ms total=2145ms chunks=145 entities=12242
```

- Stage 1 (gzip) + Stage 2 (byte scan) overlap on two threads → wall = ~max(1.9 s gzip, ~0.5 s scan) ≈ 1.8 s.
- Stage 3 (rayon per-sector quick_xml) → 275 ms across all cores.
- Stage 4 (merge into World) → 43 ms.
- **Total: 2.15 s** (≈ 3.6× over baseline; well under the 3.5 s acceptance target).

## Bugs caught during smoke benchmark

1. Byte-scanner needle `<component` also matched `<components>` (86 occurrences). Fixed by requiring trailing space (`<component `).
2. ~896 self-closing `<component .../>` tags inflated nesting depth (no matching `</component>`). Fixed by detecting `/>` suffix on opens and skipping depth increment for them.

Without those fixes the scanner emitted 2 chunks instead of 145 — caught by comparing override / entity counts to the old sequential parser's logged numbers, not by unit tests (the test fixture was too clean).

## Lessons

- Byte-level scanners need fuzz/real-data tests, not just hand-crafted fixtures.
- Spec target was 3.5 s; achieving 2.1 s confirms gzip decompression is the floor (1.9 s release) and everything else now fits in its shadow.
