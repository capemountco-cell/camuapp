//! CPU architectures: which chip this Mac has, which chips an app's binary
//! runs on, and which chip a download is built for.
//!
//! An app's executable is read directly (its Mach-O header) rather than with
//! `lipo`, which needs the Xcode command line tools.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    /// Apple silicon.
    #[default]
    Arm64,
    /// Intel.
    X86_64,
}

impl Arch {
    /// The chip Camu itself runs on.
    pub fn host() -> Self {
        if std::env::consts::ARCH == "x86_64" {
            Arch::X86_64
        } else {
            Arch::Arm64
        }
    }

    /// The chip family's name for messages.
    pub fn label(self) -> &'static str {
        match self {
            Arch::Arm64 => "Apple silicon",
            Arch::X86_64 => "Intel",
        }
    }
}

const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;

fn cpu(t: u32) -> Option<Arch> {
    match t {
        CPU_TYPE_ARM64 => Some(Arch::Arm64),
        CPU_TYPE_X86_64 => Some(Arch::X86_64),
        _ => None,
    }
}

/// The architectures in a Mach-O file's first bytes (thin or universal).
/// Empty when the bytes aren't a Mach-O header Camu understands.
pub fn macho_archs(bytes: &[u8]) -> Vec<Arch> {
    let be = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let le = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut out = Vec::new();
    match be(0) {
        // Universal ("fat") binary: big-endian header, then one entry per
        // slice. 0xcafebabf is the 64-bit variant (entries are 32 bytes).
        Some(magic @ (0xcafe_babe | 0xcafe_babf)) => {
            let count = be(4).unwrap_or(0).min(16) as usize;
            let entry = if magic == 0xcafe_babf { 32 } else { 20 };
            for i in 0..count {
                if let Some(arch) = be(8 + i * entry).and_then(cpu) {
                    if !out.contains(&arch) {
                        out.push(arch);
                    }
                }
            }
        }
        _ => {
            // Thin 64-bit binary, little-endian on every Mac.
            if le(0) == Some(0xfeed_facf) {
                if let Some(arch) = le(4).and_then(cpu) {
                    out.push(arch);
                }
            }
        }
    }
    out
}

/// The architectures an app bundle's main executable runs on. Empty when it
/// can't be read.
pub fn app_archs(app: &Path, executable: Option<&str>) -> Vec<Arch> {
    use std::io::Read;
    let name = match executable {
        Some(name) => name.to_string(),
        None => match app.file_stem() {
            Some(stem) => stem.to_string_lossy().into_owned(),
            None => return Vec::new(),
        },
    };
    let path = app.join("Contents/MacOS").join(name);
    let mut head = [0u8; 4096];
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let n = file.read(&mut head).unwrap_or(0);
    macho_archs(&head[..n])
}

/// Can a binary built for `archs` run on `host`? Apple silicon also runs
/// Intel code through Rosetta. An unreadable binary (empty) gets the benefit
/// of the doubt.
pub fn runs_on(archs: &[Arch], host: Arch) -> bool {
    archs.is_empty()
        || archs.contains(&host)
        || (host == Arch::Arm64 && archs.contains(&Arch::X86_64))
}

/// What a download's file name says it's built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    Universal,
    Only(Arch),
    Unknown,
}

/// Read a download URL or file name for chip hints, by whole words:
/// "Rocket-2.0-arm64.dmg" is Apple silicon, "…-x64.zip" Intel,
/// "…-universal.dmg" both.
pub fn build_of(url: &str) -> Build {
    let name = url.rsplit('/').next().unwrap_or(url).to_ascii_lowercase();
    let words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .collect();
    let has = |w: &[&str]| words.iter().any(|x| w.contains(x));
    if has(&["universal", "univ", "fat"]) {
        return Build::Universal;
    }
    let arm = has(&["arm64", "aarch64", "arm", "applesilicon", "silicon", "m1"]);
    let intel = has(&["x64", "x86_64", "x86", "amd64", "intel", "x86-64"]);
    match (arm, intel) {
        (true, false) => Build::Only(Arch::Arm64),
        (false, true) => Build::Only(Arch::X86_64),
        _ => Build::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_thin_and_universal_headers() {
        let mut thin_arm = vec![0xcf, 0xfa, 0xed, 0xfe];
        thin_arm.extend_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
        assert_eq!(macho_archs(&thin_arm), vec![Arch::Arm64]);

        let mut thin_intel = vec![0xcf, 0xfa, 0xed, 0xfe];
        thin_intel.extend_from_slice(&CPU_TYPE_X86_64.to_le_bytes());
        assert_eq!(macho_archs(&thin_intel), vec![Arch::X86_64]);

        let mut fat = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2];
        for t in [CPU_TYPE_X86_64, CPU_TYPE_ARM64] {
            fat.extend_from_slice(&t.to_be_bytes());
            fat.extend_from_slice(&[0; 16]);
        }
        assert_eq!(macho_archs(&fat), vec![Arch::X86_64, Arch::Arm64]);

        assert!(macho_archs(b"#!/bin/sh\n").is_empty());
    }

    #[test]
    fn rosetta_runs_intel_on_apple_silicon_but_not_the_reverse() {
        assert!(runs_on(&[Arch::X86_64], Arch::Arm64));
        assert!(!runs_on(&[Arch::Arm64], Arch::X86_64));
        assert!(runs_on(&[Arch::Arm64, Arch::X86_64], Arch::X86_64));
        assert!(runs_on(&[], Arch::X86_64));
    }

    #[test]
    fn download_names() {
        assert_eq!(
            build_of("https://x/Rocket-2.0-arm64.dmg"),
            Build::Only(Arch::Arm64)
        );
        assert_eq!(
            build_of("https://x/1Password-8.12-aarch64.zip"),
            Build::Only(Arch::Arm64)
        );
        assert_eq!(
            build_of("https://x/Rocket-2.0-x64.zip"),
            Build::Only(Arch::X86_64)
        );
        assert_eq!(
            build_of("https://x/0ad-macos-x86_64.dmg"),
            Build::Only(Arch::X86_64)
        );
        assert_eq!(
            build_of("https://x/Camu-0.2.0-universal.dmg"),
            Build::Universal
        );
        assert_eq!(build_of("https://x/Rocket.dmg"), Build::Unknown);
        // "darwin" mustn't read as anything.
        assert_eq!(build_of("https://x/rocket-darwin.zip"), Build::Unknown);
    }
}
