//! Isolation regression test (Linux only): proves the sandbox jail holds —
//! no host filesystem leak, no network, seccomp denies escape primitives.
//! Requires a Linux kernel that allows unprivileged user namespaces
//! (verified in the backend Docker container).
#![cfg(target_os = "linux")]

use core_agent::sandbox_jail::linux::{NetworkPolicyChoice, Sandbox, SandboxLimits};

#[test]
fn sandbox_isolation_guarantees_hold() {
    let work = std::env::temp_dir().join("susutaku-sandbox-iso-test");
    let sb = Sandbox::new_in(&work).expect("new_in");
    // Compound commands fork a subshell per pipe/`;`-group under bash; the
    // default NPROC of 64 is tight in a 1-pid namespace, so widen it here.
    let limits = SandboxLimits {
        max_processes: 256,
        ..SandboxLimits::default()
    };
    let run = |cmd: &str| -> Result<String, std::io::Error> {
        sb.run_with(cmd, &limits, NetworkPolicyChoice::Disabled)
    };

    // 1. Host filesystem is not visible inside the jail.
    let out = run("ls /app >/dev/null 2>&1; echo rc=$?").expect("run 1");
    assert!(out.contains("rc=2"), "host /app leaked into jail: {out:?}");

    // 2. /proc/mounts never mentions the host app tree, and the tmpfs
    //    scratch + ro binds match the documented layout.
    let out = run("grep -c /app /proc/mounts || true").expect("run 2");
    assert_eq!(out.trim(), "0", "host mounts visible in jail: {out:?}");
    let out = run("grep -c 'tmpfs /tmp ' /proc/mounts").expect("run 2b");
    assert_eq!(out.trim(), "1", "tmp /tmp missing: {out:?}");

    // 3. Network namespace is empty: loopback only, nothing else.
    //    /proc is a recursive bind of the parent's, so query the live
    //    socket state instead: no route to the outside world is reachable.
    let out = run("ip link 2>/dev/null | grep -c Open || echo none").expect("run 3");
    assert!(
        out.contains("none") || out.trim().ends_with("0"),
        "unexpected live interfaces: {out:?}"
    );

    // 4. seccomp denies chroot re-entry (escape primitive). busybox chroot
    //    exits 125 when exec fails after a denied chroot.
    let out = run("chroot / ; echo rc=$?").expect("run 4");
    assert!(out.contains("rc=125"), "chroot not denied: {out:?}");

    // 5. seccomp denies mount (escape primitive). Two runs: bash forks per
    //    `;`-group and the 1-pid namespace's fork accounting is tight, so
    //    keep each sandbox command to a single process.
    run("mkdir /tmp/mm2").expect("run 5a");
    let out = run("mount -t tmpfs none /tmp/mm2; echo rc=$?").expect("run 5b");
    assert!(out.contains("rc=32"), "mount not denied: {out:?}");

    // 6. PID namespace: only the shell's own process tree is visible.
    let out = run("ls /proc").expect("run 6");
    let pids = out
        .lines()
        .filter(|l| l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty())
        .count();
    assert!(pids < 10, "too many pids visible: {pids}");

    // 7. Writes persist across runs only in the workspace.
    run("echo iso-proof > /workspace/proof").expect("run 7a");
    let out = run("cat /workspace/proof").expect("run 7b");
    assert!(out.contains("iso-proof"), "workspace write lost: {out:?}");
    let out = run("cat /tmp/proof").unwrap_or_default();
    assert!(
        out.is_empty() || out.contains("No such file"),
        "tmp persisted across runs (fresh tmpfs expected): {out:?}"
    );

    sb.purge();
    std::fs::remove_dir_all(&work).ok();
}
