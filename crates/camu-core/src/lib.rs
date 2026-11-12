//! The app's logic.
//!
//! Everything that makes a decision lives here, not in the Tauri crate and not
//! in React. This crate does not depend on Tauri, so it builds and its tests
//! run on Linux in CI, without a window, a webview or a Mac.

/// Build the greeting shown by the starter screen.
///
/// Blank input gets a neutral greeting rather than "Hello, !".
pub fn greeting(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "Hello from Rust.".to_string()
    } else {
        format!("Hello, {name}. This came from Rust.")
    }
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn greets_by_name() {
        assert_eq!(greeting("Ada"), "Hello, Ada. This came from Rust.");
    }

    #[test]
    fn trims_and_handles_blank_input() {
        assert_eq!(greeting("  Ada "), "Hello, Ada. This came from Rust.");
        assert_eq!(greeting("   "), "Hello from Rust.");
    }
}
