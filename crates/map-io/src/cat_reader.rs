use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rayon::prelude::*;

/// Read a known set of files in a single parallel pass over the archives.
///
/// Each cat index must still be fully line-scanned to compute byte offsets, but
/// bytes are only read (and allocated) for paths in `wanted` — far cheaper than
/// indexing all ~330k archive entries to fetch a few thousand. First archive
/// wins on duplicates (main `01..08` before DLC `ext_*`), matching
/// [`read_game_file`] resolution.
pub fn read_files(
    game_dir: &Path,
    wanted: &std::collections::HashSet<String>,
) -> HashMap<String, Vec<u8>> {
    // Per-archive (in load order): matched (path, bytes) pairs.
    let per_archive: Vec<Vec<(String, Vec<u8>)>> = archive_sources(game_dir)
        .par_iter()
        .map(|(cat_path, dat_path)| {
            let mut out: Vec<(String, Vec<u8>)> = Vec::new();
            let Ok(content) = std::fs::read_to_string(cat_path) else {
                return out;
            };
            let Ok(mut dat) = std::fs::File::open(dat_path) else {
                return out;
            };
            let mut offset: u64 = 0;
            for line in content.lines() {
                let Some((path, size)) = parse_cat_line(line) else {
                    continue;
                };
                if wanted.contains(path) && dat.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buf = vec![0u8; size as usize];
                    if dat.read_exact(&mut buf).is_ok() {
                        out.push((path.to_string(), buf));
                    }
                }
                offset += size;
            }
            out
        })
        .collect();

    let mut result: HashMap<String, Vec<u8>> = HashMap::new();
    for archive in per_archive {
        for (path, bytes) in archive {
            result.entry(path).or_insert(bytes);
        }
    }
    result
}

/// Enumerate (cat, dat) archive pairs: main `NN.cat` then each DLC `ext_NN.cat`,
/// in load order (main first, DLC dirs sorted).
fn archive_sources(game_dir: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut sources: Vec<(PathBuf, PathBuf)> = Vec::new();
    for n in 1..=99u32 {
        let cat = game_dir.join(format!("{:02}.cat", n));
        if !cat.exists() {
            break;
        }
        sources.push((cat, game_dir.join(format!("{:02}.dat", n))));
    }
    if let Ok(entries) = std::fs::read_dir(game_dir.join("extensions")) {
        let mut ext_dirs: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        ext_dirs.sort();
        for ext_dir in ext_dirs {
            for n in 1..=99u32 {
                let cat = ext_dir.join(format!("ext_{:02}.cat", n));
                if !cat.exists() {
                    break;
                }
                sources.push((cat, ext_dir.join(format!("ext_{:02}.dat", n))));
            }
        }
    }
    sources
}

/// Parse one cat index line `"internal/path size timestamp md5hash"` → (path, size).
/// File names may contain spaces, so split from the right. Returns `None` for
/// malformed lines.
fn parse_cat_line(line: &str) -> Option<(&str, u64)> {
    let mut parts = line.rsplitn(3, ' ');
    let _hash = parts.next();
    let _ts = parts.next();
    let rest = parts.next()?;
    let sep = rest.rfind(' ')?;
    let path = &rest[..sep];
    let size: u64 = rest[sep + 1..].parse().ok()?;
    Some((path, size))
}

/// Read all occurrences of a path across main cats and all extension cats.
/// Used when multiple archives supply the same file (e.g. each DLC's mapdefaults.xml).
pub fn read_all_game_files(game_dir: &Path, internal_path: &str) -> Vec<Vec<u8>> {
    let mut results = Vec::new();

    for n in 1..=99u32 {
        let cat_path = game_dir.join(format!("{:02}.cat", n));
        if !cat_path.exists() {
            break;
        }
        let dat_path = game_dir.join(format!("{:02}.dat", n));
        if let Some(data) = search_cat(&cat_path, &dat_path, internal_path) {
            results.push(data);
        }
    }

    for ext_dir in std::fs::read_dir(game_dir.join("extensions"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        for n in 1..=99u32 {
            let cat_path = ext_dir.join(format!("ext_{:02}.cat", n));
            if !cat_path.exists() {
                break;
            }
            let dat_path = ext_dir.join(format!("ext_{:02}.dat", n));
            if let Some(data) = search_cat(&cat_path, &dat_path, internal_path) {
                results.push(data);
            }
        }
    }

    results
}

/// Read a file from X4's cat/dat archive pairs.
///
/// X4 stores game data in numbered pairs: 01.cat/01.dat, 02.cat/02.dat, etc.
/// Cat file format (one entry per line): `internal/path size unix_ts md5hash`
/// Dat file: raw concatenated file data; offset = cumulative sum of previous entry sizes.
pub fn read_game_file(game_dir: &Path, internal_path: &str) -> Option<Vec<u8>> {
    // Search cat files in order: 01, 02, ... stop when none found
    for n in 1..=99u32 {
        let cat_path = game_dir.join(format!("{:02}.cat", n));
        if !cat_path.exists() {
            break;
        }
        let dat_path = game_dir.join(format!("{:02}.dat", n));

        if let Some(data) = search_cat(&cat_path, &dat_path, internal_path) {
            return Some(data);
        }
    }

    // Also check extension cat files: ext_01, ext_02, ...
    for ext_dir in std::fs::read_dir(game_dir.join("extensions"))
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
    {
        for n in 1..=99u32 {
            let cat_path = ext_dir.join(format!("ext_{:02}.cat", n));
            if !cat_path.exists() {
                break;
            }
            let dat_path = ext_dir.join(format!("ext_{:02}.dat", n));
            if let Some(data) = search_cat(&cat_path, &dat_path, internal_path) {
                return Some(data);
            }
        }
    }

    None
}

fn search_cat(cat_path: &Path, dat_path: &Path, target: &str) -> Option<Vec<u8>> {
    let cat_content = std::fs::read_to_string(cat_path).ok()?;
    let mut dat = std::fs::File::open(dat_path).ok()?;

    let mut offset: u64 = 0;
    for line in cat_content.lines() {
        let Some((path, size)) = parse_cat_line(line) else {
            continue;
        };
        if path == target {
            dat.seek(SeekFrom::Start(offset)).ok()?;
            let mut buf = vec![0u8; size as usize];
            dat.read_exact(&mut buf).ok()?;
            return Some(buf);
        }
        offset += size;
    }

    None
}

/// Enumerate all (path, data) pairs across main and DLC cats whose internal path
/// starts with `prefix` and ends with `suffix`. Both prefix and suffix can be empty.
pub fn list_files_matching(game_dir: &Path, prefix: &str, suffix: &str) -> Vec<(String, Vec<u8>)> {
    let mut results = Vec::new();
    for (cat_path, dat_path) in archive_sources(game_dir) {
        let Ok(content) = std::fs::read_to_string(&cat_path) else {
            continue;
        };
        let Ok(mut dat) = std::fs::File::open(&dat_path) else {
            continue;
        };
        let mut offset: u64 = 0;
        for line in content.lines() {
            let Some((path, size)) = parse_cat_line(line) else {
                continue;
            };
            if path.starts_with(prefix) && path.ends_with(suffix) {
                if dat.seek(SeekFrom::Start(offset)).is_ok() {
                    let mut buf = vec![0u8; size as usize];
                    if dat.read_exact(&mut buf).is_ok() {
                        results.push((path.to_string(), buf));
                    }
                }
            }
            offset += size;
        }
    }
    results
}
