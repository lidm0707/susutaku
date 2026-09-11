use backend::infra::host_spec::host_spec;

#[test]
fn host_spec_reports_this_machine() {
    let spec = host_spec();

    assert!(!spec.hostname.is_empty(), "hostname non-empty");
    assert!(!spec.os.is_empty(), "os non-empty");
    assert!(!spec.arch.is_empty(), "arch non-empty");
    assert!(spec.cpu_cores >= 1, "cpu_cores >= 1");

    #[cfg(target_os = "macos")]
    assert!(spec.memory_bytes > 0, "macos reports physical memory");

    #[cfg(not(target_os = "macos"))]
    let _ = spec.memory_bytes;
}
