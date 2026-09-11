use serde::Serialize;

const SYSCTL_MEMSIZE: &[u8] = b"hw.memsize\0";
const SYSCTL_CPU_BRAND: &[u8] = b"machdep.cpu.brand_string\0";

#[cfg(target_os = "linux")]
const MEMINFO_PATH: &str = "/proc/meminfo";
#[cfg(target_os = "linux")]
const CPUINFO_PATH: &str = "/proc/cpuinfo";
#[cfg(target_os = "linux")]
const MEMTOTAL_PREFIX: &str = "MemTotal:";
#[cfg(target_os = "linux")]
const MEMTOTAL_UNIT: &str = "kB";
#[cfg(target_os = "linux")]
const CPU_MODEL_PREFIX: &str = "model name";
#[cfg(target_os = "linux")]
const CPU_MODEL_SEPARATOR: &str = ":";

#[cfg(target_os = "linux")]
const KIB: u64 = 1024;

#[derive(Serialize, utoipa::ToSchema)]
pub struct HostSpec {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub memory_bytes: u64,
}

pub fn host_spec() -> HostSpec {
    let (hostname, os, arch) = crate::infra::client_env::host_fingerprint();
    HostSpec {
        hostname,
        os,
        arch,
        cpu_model: cpu_model(),
        cpu_cores: cpu_cores(),
        memory_bytes: memory_bytes(),
    }
}

fn cpu_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn memory_bytes() -> u64 {
    sysctl_u64(SYSCTL_MEMSIZE)
}

#[cfg(target_os = "macos")]
fn cpu_model() -> String {
    sysctl_string(SYSCTL_CPU_BRAND)
}

#[cfg(target_os = "linux")]
fn memory_bytes() -> u64 {
    std::fs::read_to_string(MEMINFO_PATH)
        .ok()
        .and_then(|text| memtotal_bytes(&text))
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn cpu_model() -> String {
    std::fs::read_to_string(CPUINFO_PATH)
        .ok()
        .and_then(|text| first_cpu_model(&text))
        .unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn memory_bytes() -> u64 {
    0
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn cpu_model() -> String {
    String::new()
}

#[cfg(target_os = "linux")]
fn memtotal_bytes(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with(MEMTOTAL_PREFIX))?;
    let value = line
        .trim_start_matches(MEMTOTAL_PREFIX)
        .trim()
        .trim_end_matches(MEMTOTAL_UNIT)
        .trim();
    value.parse::<u64>().ok().map(|kb| kb * KIB)
}

#[cfg(target_os = "linux")]
fn first_cpu_model(cpuinfo: &str) -> Option<String> {
    cpuinfo
        .lines()
        .find(|l| l.starts_with(CPU_MODEL_PREFIX))
        .and_then(|l| l.split_once(CPU_MODEL_SEPARATOR))
        .map(|(_, model)| model.trim().to_owned())
}

#[cfg(target_os = "macos")]
fn sysctl_u64(name: &[u8]) -> u64 {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let ok = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            (&raw mut value).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ok == 0 { value } else { 0 }
}

#[cfg(target_os = "macos")]
fn sysctl_string(name: &[u8]) -> String {
    let mut buf = [0u8; 256];
    let mut len = buf.len();
    let ok = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if ok != 0 {
        return String::new();
    }
    let bytes = &buf[..len.saturating_sub(1).min(buf.len())];
    String::from_utf8_lossy(bytes).into_owned()
}
