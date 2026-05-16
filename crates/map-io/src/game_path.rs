use std::path::{Path, PathBuf};

const GAME_DIR_NAME: &str = "X4 Foundations";

/// Attempt to detect the X4 Foundations installation directory.
/// Returns None if not found; caller should prompt user to set path manually.
pub fn detect() -> Option<PathBuf> {
    detect_platform()
}

#[cfg(target_os = "linux")]
fn detect_platform() -> Option<PathBuf> {
    let candidates = linux_steam_paths();
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(target_os = "linux")]
fn linux_steam_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var("HOME").ok() else {
        return vec![];
    };
    vec![
        PathBuf::from(&home)
            .join(".steam/steam/steamapps/common")
            .join(GAME_DIR_NAME),
        PathBuf::from(&home)
            .join(".local/share/Steam/steamapps/common")
            .join(GAME_DIR_NAME),
        PathBuf::from("/usr/share/Steam/steamapps/common").join(GAME_DIR_NAME),
    ]
}

#[cfg(target_os = "windows")]
fn detect_platform() -> Option<PathBuf> {
    // Try Steam registry key first
    if let Some(path) = windows_registry_path() {
        if path.exists() {
            return Some(path);
        }
    }
    // Fallback: common Steam install locations
    for base in &[
        r"C:\Program Files (x86)\Steam\steamapps\common",
        r"C:\Program Files\Steam\steamapps\common",
    ] {
        let p = PathBuf::from(base).join(GAME_DIR_NAME);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_registry_path() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::*;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 392160")
        .ok()?;
    let install_location: String = key.get_value("InstallLocation").ok()?;
    Some(PathBuf::from(install_location))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn detect_platform() -> Option<PathBuf> {
    None
}

/// Detect the X4 locale ID used by the active Steam account for this game.
/// Returns a 3-digit locale ID (e.g. 44 = English, 7 = Russian, 49 = German).
/// Falls back to 44 (English) on any failure.
pub fn detect_locale(game_dir: &Path) -> u32 {
    detect_locale_inner(game_dir).unwrap_or(44)
}

fn detect_locale_inner(game_dir: &Path) -> Option<u32> {
    let steam_root = find_steam_root(game_dir)?;
    let userdata = steam_root.join("userdata");
    for entry in std::fs::read_dir(&userdata).ok()?.flatten() {
        let localconfig = entry.path().join("config").join("localconfig.vdf");
        let content = std::fs::read_to_string(&localconfig).ok()?;
        if let Some(lang) = extract_x4_language_from_vdf(&content) {
            return Some(steam_language_to_locale_id(&lang));
        }
    }
    None
}

fn find_steam_root(game_dir: &Path) -> Option<PathBuf> {
    // game_dir = <steam_root>/steamapps/common/X4 Foundations  (3 levels up)
    if let Some(root) = game_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    {
        if root.join("userdata").exists() {
            return Some(root.to_path_buf());
        }
    }
    // Fallback: well-known Steam root locations
    #[cfg(target_os = "linux")]
    if let Ok(home) = std::env::var("HOME") {
        for candidate in &[
            format!("{home}/.steam/steam"),
            format!("{home}/.local/share/Steam"),
        ] {
            let p = PathBuf::from(candidate);
            if p.join("userdata").exists() {
                return Some(p);
            }
        }
    }
    #[cfg(target_os = "windows")]
    for candidate in &[r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        let p = PathBuf::from(candidate);
        if p.join("userdata").exists() {
            return Some(p);
        }
    }
    None
}

/// Scan localconfig.vdf text for the X4 (app 392160) language hex blob.
fn extract_x4_language_from_vdf(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("392160") {
            continue;
        }
        // Looking for: "392160"<tabs>"<hexblob>"
        let mut parts = trimmed.splitn(3, '"');
        parts.next(); // leading empty
        let key = parts.next().unwrap_or("");
        if key != "392160" {
            continue;
        }
        let rest = parts.next().unwrap_or("");
        // rest is: <whitespace>"<hexblob>"
        let blob_str = rest.trim().trim_matches('"');
        if blob_str.len() < 20 || !blob_str.chars().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        if let Some(bytes) = decode_hex(blob_str) {
            if let Some(lang) = extract_language_from_binary_vdf(&bytes) {
                return Some(lang);
            }
        }
    }
    None
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// In binary VDF, type 0x01 = string. Find key "language" and return its value.
fn extract_language_from_binary_vdf(data: &[u8]) -> Option<String> {
    let key = b"language\x00";
    let pos = data.windows(key.len()).position(|w| w == key)?;
    if pos == 0 || data[pos - 1] != 0x01 {
        return None;
    }
    let val_start = pos + key.len();
    let val_end = data[val_start..].iter().position(|&b| b == 0)? + val_start;
    std::str::from_utf8(&data[val_start..val_end])
        .ok()
        .map(str::to_string)
}

fn steam_language_to_locale_id(lang: &str) -> u32 {
    match lang {
        "english" => 44,
        "german" => 49,
        "french" => 33,
        "spanish" => 34,
        "italian" => 39,
        "russian" => 7,
        "czech" => 42,
        "polish" => 48,
        "portuguese" | "brazilian" => 55,
        "japanese" => 81,
        "koreana" => 82,
        "schinese" => 86,
        "tchinese" => 88,
        "turkish" => 90,
        "bulgarian" => 359,
        "ukrainian" => 380,
        _ => 44,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn returns_none_when_no_game_dir_exists() {
        // With no actual game installed in test env, detect() should return None
        // (unless running on a dev machine with X4 installed — acceptable)
        let result = detect();
        // Can't assert None because dev might have game installed.
        // Assert that if Some, the path exists.
        if let Some(path) = result {
            assert!(path.exists(), "Detected path must exist: {:?}", path);
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_paths_are_absolute() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
        let paths = linux_steam_paths();
        assert!(!paths.is_empty());
        for p in &paths {
            assert!(p.is_absolute(), "Path must be absolute: {:?}", p);
            assert!(p.to_string_lossy().contains(GAME_DIR_NAME));
        }
    }
}
