/// Returns the host platform key used in `service.json` binary maps,
/// e.g. `"darwin/arm64"` or `"linux/amd64"`.
pub fn host_platform_key() -> &'static str {
    _host_platform_key()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn _host_platform_key() -> &'static str {
    "darwin/arm64"
}

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn _host_platform_key() -> &'static str {
    "darwin/amd64"
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn _host_platform_key() -> &'static str {
    "linux/arm64"
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn _host_platform_key() -> &'static str {
    "linux/amd64"
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
)))]
fn _host_platform_key() -> &'static str {
    "unknown"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_key_is_known() {
        let key = host_platform_key();
        assert!(
            ["darwin/arm64", "darwin/amd64", "linux/arm64", "linux/amd64"].contains(&key),
            "unexpected platform key: {key}"
        );
    }

    #[test]
    fn platform_key_contains_slash() {
        assert!(host_platform_key().contains('/'));
    }
}
