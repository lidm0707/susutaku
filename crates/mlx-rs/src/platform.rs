//! Platform info via sysctl (macOS): chip name and installed RAM.

use std::sync::LazyLock;

pub static CHIP: LazyLock<String> = LazyLock::new(chip_name);
pub static INSTALLED_BYTES: LazyLock<usize> = LazyLock::new(installed_bytes);

fn sysctl_str(name: &str) -> Option<String> {
    let mut len = 0usize;
    let status = unsafe {
        libc::sysctlbyname(
            std::ffi::CString::new(name).ok()?.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || len == 0 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let status = unsafe {
        libc::sysctlbyname(
            std::ffi::CString::new(name).ok()?.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    buf.truncate(len.saturating_sub(1));
    String::from_utf8(buf).ok()
}

fn chip_name() -> String {
    sysctl_str("machdep.cpu.brand_string").unwrap_or_else(|| "unknown".to_string())
}

fn installed_bytes() -> usize {
    const GIB: usize = 1024 * 1024 * 1024;
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    assert_eq!(status, 0, "sysctl hw.memsize failed");
    (value as usize / GIB) * GIB
}

pub fn log() {
    eprintln!(
        "[platform] {}, {} GiB",
        *CHIP,
        *INSTALLED_BYTES / (1024 * 1024 * 1024)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_is_apple_silicon() {
        assert!(CHIP.starts_with("Apple "), "chip: {}", *CHIP);
    }

    #[test]
    fn installed_ram_is_aligned_gib() {
        assert!(*INSTALLED_BYTES >= 8 * 1024 * 1024 * 1024);
        assert_eq!(*INSTALLED_BYTES % (1024 * 1024 * 1024), 0);
    }
}
