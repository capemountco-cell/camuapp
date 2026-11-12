//! App icons for the list, as small PNG data URLs.
//!
//! macOS's own `sips` converts the bundle's `.icns` file, so no image library
//! is needed. Converted icons are cached on disk by path and modification
//! time, so a second launch shows them instantly.

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pixel size of list icons (drawn at 32 points on a Retina screen).
const SIZE: &str = "64";

fn cache_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join("Library/Caches/co.capemount.camu/icons"))
}

/// The icon of the app at `app_path` as a `data:image/png;base64,...` URL.
pub fn data_url(app_path: &Path, icon_file: &str) -> Option<String> {
    if !cfg!(target_os = "macos") || icon_file.contains('/') {
        return None;
    }
    let source = app_path.join("Contents/Resources").join(icon_file);
    let modified = fs::metadata(&source).and_then(|m| m.modified()).ok()?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (source.as_os_str(), modified).hash(&mut hasher);
    let dir = cache_dir()?;
    let cached = dir.join(format!("{:016x}.png", hasher.finish()));

    if !cached.is_file() {
        fs::create_dir_all(&dir).ok()?;
        let ok = Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "-Z", SIZE])
            .arg(&source)
            .arg("--out")
            .arg(&cached)
            .output()
            .ok()?
            .status
            .success();
        if !ok {
            return None;
        }
    }
    let png = fs::read(&cached).ok()?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

/// Standard base64. Twenty lines rather than a dependency.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
