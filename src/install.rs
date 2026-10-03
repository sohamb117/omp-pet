//! Bounded, data-only sprite pack installation. Network work never runs on AppKit's thread.
use crate::sprites::Manifest;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
const MAX_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Debug)]
pub struct InstallLink {
    pub url: String,
    pub sha256: String,
}
fn decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let digits = value.get(i + 1..i + 3).ok_or("Invalid URL escape")?;
            out.push(u8::from_str_radix(digits, 16).map_err(|_| "Invalid URL escape")?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "Invalid UTF-8 URL".into())
}
impl InstallLink {
    pub fn parse(link: &str) -> Result<Self, String> {
        if link.len() > 2048 {
            return Err("Install link is too long".into());
        }
        let query = link
            .strip_prefix("omppet://install?")
            .ok_or("Unsupported OMP Pet link")?;
        let mut url = None;
        let mut sha = None;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').ok_or("Invalid install link")?;
            match key {
                "url" if url.is_none() => url = Some(decode(value)?),
                "sha256" if sha.is_none() => sha = Some(value.to_string()),
                _ => return Err("Invalid install link parameters".into()),
            }
        }
        let url = url.ok_or("Missing pack URL")?;
        let sha256 = sha.ok_or("Missing pack checksum")?;
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("Invalid pack checksum".into());
        }
        validate_url(&url)?;
        Ok(Self { url, sha256 })
    }
}
fn validate_url(url: &str) -> Result<(), String> {
    let (origin, path) = url
        .split_once("/api/packs/")
        .ok_or("Invalid pack endpoint")?;
    let trusted = matches!(
        origin,
        "https://morisoba.moe" | "https://www.morisoba.moe" | "https://pets.morisoba.moe"
    );
    // Development links are confined to numeric loopback ports, never arbitrary HTTP hosts.
    let local = origin
        .strip_prefix("http://localhost:")
        .or_else(|| origin.strip_prefix("http://127.0.0.1:"))
        .is_some_and(|port| {
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|p| p > 0)
        });
    if !trusted && !local {
        return Err("Pack links must come from morisoba.moe".into());
    }
    let (id, direction) = path
        .split_once("?direction=")
        .ok_or("Missing pack direction")?;
    let pieces: Vec<_> = id.split('-').collect();
    if pieces.is_empty()
        || pieces.len() > 4
        || pieces
            .iter()
            .any(|p| p.len() != 4 || !p.bytes().all(|b| b.is_ascii_digit()))
        || direction.len() != 1
        || !matches!(direction.as_bytes()[0], b'0'..=b'7')
    {
        return Err("Invalid pack selection".into());
    }
    Ok(())
}
fn allowed_file(name: &str) -> bool {
    matches!(
        name,
        "manifest.json"
            | "CREDITS.md"
            | "ARTWORK-LICENSE.md"
            | "upstream-credits.txt"
            | "artist-names.txt"
    ) || name.strip_suffix(".png").is_some_and(|stem| {
        !stem.is_empty()
            && stem
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    })
}
pub fn extract(bytes: &[u8], expected: &str, destination: &Path) -> Result<(), String> {
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Pack archive is too large".into());
    }
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err("Pack checksum did not match. Please select the pet again on Morisoba.".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if zip.is_empty() || zip.len() > 24 {
        return Err("Invalid pack file count".into());
    }
    let mut total = 0u64;
    let mut names = HashSet::new();
    // Validate every entry before writing any files.
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        if !allowed_file(file.name())
            || !names.insert(file.name().to_owned())
            || file.is_dir()
            || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("Pack contains an unsupported file".into());
        }
        total = total.checked_add(file.size()).ok_or("Pack is too large")?;
        if total > MAX_BYTES || (file.name() == "manifest.json" && file.size() > 65536) {
            return Err("Unpacked files exceed the size limit".into());
        }
    }
    if !names.contains("manifest.json") {
        return Err("Pack has no manifest".into());
    }
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_owned();
        let mut data = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 != file.size() {
            return Err("Invalid unpacked file size".into());
        }
        if name == "manifest.json" {
            let manifest: Manifest = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            manifest.validate()?;
        }
        fs::write(destination.join(name), data).map_err(|e| e.to_string())?;
    }
    Ok(())
}
struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub fn install(link: &InstallLink, root: &Path) -> Result<PathBuf, String> {
    validate_url(&link.url)?;
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let destination = root.join(&link.sha256);
    if destination.join("manifest.json").is_file() {
        return Ok(destination);
    }
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = Staging(root.join(format!(
        ".install-{}-{stamp}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&staging.0).map_err(|e| e.to_string())?;
    let archive = staging.0.join("download.zip");
    let result = Command::new("/usr/bin/curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https,http",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "10",
            "--max-time",
            "60",
            "--max-filesize",
            "16777216",
            "--output",
        ])
        .arg(&archive)
        .arg(&link.url)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "Could not download pet: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    if fs::metadata(&archive).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("Pack archive is too large".into());
    }
    let bytes = fs::read(&archive).map_err(|e| e.to_string())?;
    let pack = staging.0.join("pack");
    fs::create_dir(&pack).map_err(|e| e.to_string())?;
    extract(&bytes, &link.sha256, &pack)?;
    fs::rename(&pack, &destination).map_err(|e| e.to_string())?;
    Ok(destination)
}
pub fn pack_root() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/OMP Pet/packs")
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn link(url: &str) -> String {
        format!("omppet://install?url={url}&sha256={}", "a".repeat(64))
    }
    #[test]
    fn accepts_only_constrained_pack_links() {
        assert!(
            InstallLink::parse(&link(
                "https%3A%2F%2Fmorisoba.moe%2Fapi%2Fpacks%2F0570-0001%3Fdirection%3D1"
            ))
            .is_ok()
        );
        assert!(
            InstallLink::parse(&link("http://localhost:4173/api/packs/0570?direction=1")).is_ok()
        );
        for url in [
            "https://morisoba.moe.evil.com/api/packs/0570?direction=1",
            "https://morisoba.moe@evil.com/api/packs/0570?direction=1",
            "http://morisoba.moe/api/packs/0570?direction=1",
            "https://morisoba.moe/api/packs/../../etc?direction=1",
            "https://morisoba.moe/api/packs/0570?direction=9",
            "http://localhost:4173@evil.com/api/packs/0570?direction=1",
            "https://morisoba.moe/api/packs/0570?direction=1%23bad",
        ] {
            assert!(InstallLink::parse(&link(url)).is_err(), "{url}");
        }
        assert!(
            InstallLink::parse(
                &(link("https://morisoba.moe/api/packs/0570?direction=1") + "&url=x")
            )
            .is_err()
        );
    }
    fn archive(name: &str) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"test").unwrap();
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn rejects_traversal_and_wrong_checksum() {
        let bytes = archive("../manifest.json");
        let sha = format!("{:x}", Sha256::digest(&bytes));
        assert!(
            extract(&bytes, &sha, Path::new("/nonexistent-test-destination"))
                .unwrap_err()
                .contains("unsupported")
        );
        assert!(
            extract(
                &bytes,
                &"0".repeat(64),
                Path::new("/nonexistent-test-destination")
            )
            .unwrap_err()
            .contains("checksum")
        );
    }
    #[test]
    fn rejects_executables_and_nested_paths() {
        for name in ["pet.sh", "x/Idle.png", "/Idle.png", "..\\Idle.png", ".png"] {
            assert!(!allowed_file(name));
        }
        assert!(allowed_file("Idle.png"));
    }
}
