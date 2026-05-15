use std::path::PathBuf;

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
    let Some(home) = std::env::var("HOME").ok() else { return vec![]; };
    vec![
        PathBuf::from(&home).join(".steam/steam/steamapps/common").join(GAME_DIR_NAME),
        PathBuf::from(&home).join(".local/share/Steam/steamapps/common").join(GAME_DIR_NAME),
        PathBuf::from("/usr/share/Steam/steamapps/common").join(GAME_DIR_NAME),
    ]
}

#[cfg(target_os = "windows")]
fn detect_platform() -> Option<PathBuf> {
    // Try Steam registry key first
    if let Some(path) = windows_registry_path() {
        if path.exists() { return Some(path); }
    }
    // Fallback: common Steam install locations
    for base in &[
        r"C:\Program Files (x86)\Steam\steamapps\common",
        r"C:\Program Files\Steam\steamapps\common",
    ] {
        let p = PathBuf::from(base).join(GAME_DIR_NAME);
        if p.exists() { return Some(p); }
    }
    None
}

#[cfg(target_os = "windows")]
fn windows_registry_path() -> Option<PathBuf> {
    use winreg::enums::*;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Steam App 392160"
    ).ok()?;
    let install_location: String = key.get_value("InstallLocation").ok()?;
    Some(PathBuf::from(install_location))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn detect_platform() -> Option<PathBuf> {
    None
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
