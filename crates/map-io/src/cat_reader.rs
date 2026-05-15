use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Read all occurrences of a path across main cats and all extension cats.
/// Used when multiple archives supply the same file (e.g. each DLC's mapdefaults.xml).
pub fn read_all_game_files(game_dir: &Path, internal_path: &str) -> Vec<Vec<u8>> {
    let mut results = Vec::new();

    for n in 1..=99u32 {
        let cat_path = game_dir.join(format!("{:02}.cat", n));
        if !cat_path.exists() { break; }
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
            if !cat_path.exists() { break; }
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
        // Cat lines: "internal/path size timestamp md5hash"
        // File names may contain spaces (e.g. "Copy of file.dds 1234 ...").
        // Split into at most 3 parts so path can contain spaces, then parse size.
        // Lines that don't parse are skipped (bad/comment lines).
        let mut parts = line.rsplitn(3, ' ');
        let _hash = parts.next();
        let _ts   = parts.next();
        let rest  = match parts.next() {
            Some(s) => s,
            None => continue,
        };
        // rest = "internal/path size"
        let sep = match rest.rfind(' ') {
            Some(i) => i,
            None => continue,
        };
        let path = &rest[..sep];
        let size: u64 = match rest[sep + 1..].parse() {
            Ok(n) => n,
            Err(_) => continue,
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
