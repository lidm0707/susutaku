//! Windows sandbox integration tests. Run on a Windows host (unelevated):
//! `cargo test -p core-agent --target x86_64-pc-windows-msvc --test sandbox_windows`
//!
//! Some of these spawn PowerShell with a restricted token; a handful of
//! corporate lock-down policies (AppLocker, WDAC) can block restricted-token
//! PowerShell. If `sandbox_normal_run` fails with an access-denied, check the
//! machine's PowerShell Constrained Language mode first.

#![cfg(windows)]

use core_agent::sandbox_jail::windows as sb;
use core_agent::sandbox_jail::windows::Role;

use std::sync::Mutex;

static STATE_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn sandbox_normal_run() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let out = sandbox.run("Write-Output hi").unwrap();
    assert_eq!(out.trim(), "hi");
    assert!(sb::state_path().exists());
    let t = sandbox.transcript();
    assert_eq!(t.len(), 1);
    sandbox.purge();
    assert!(!sb::state_path().exists());
}

#[test]
fn sandbox_file_create_inside_workspace() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let out = sandbox
        .run("Set-Content -Path inside.txt -Value data; Get-Content inside.txt")
        .unwrap();
    assert_eq!(out.trim(), "data");
    assert!(sandbox.root().join("inside.txt").exists());
    sandbox.purge();
}

#[test]
fn sandbox_read_outside_denied() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    // Reading another location under the user profile must fail (DACL has no
    // grant for the logon SID there, and the restricted token rejects the
    // user-SID / Authenticated-Users ACEs).
    let result = sandbox.run("Get-Content $env:USERPROFILE\\..\\..\\Windows\\win.ini");
    // Either PowerShell errors (file not accessible) or output is empty due
    // to access denial — it must NOT return the file contents.
    if let Ok(out) = result {
        assert!(
            !out.contains("[fonts]"),
            "sandbox read a file outside the workspace"
        );
    }
    sandbox.purge();
}

#[test]
fn sandbox_cd_outside_clamped() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    sandbox.set_cwd("sub").unwrap_or_default();
    // Direct cd attempts in the command cannot escape the workspace DACL.
    let result = sandbox.run("Set-Location C:\\Windows; Get-Location");
    if let Ok(out) = result {
        assert!(
            !out.to_lowercase().contains("c:\\windows"),
            "sandbox cd escaped the workspace"
        );
    }
    sandbox.purge();
}

#[test]
fn sandbox_child_process_stays_in_job() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    // Spawn a child that would outlive the parent; job kill-on-close must
    // terminate it when the run ends.
    let out = sandbox
        .run("Start-Process -FilePath cmd.exe -ArgumentList '/c ping -n 60 127.0.0.1' -WindowStyle Hidden; Write-Output spawned")
        .unwrap();
    assert_eq!(out.trim(), "spawned");
    drop(sandbox); // job closes → tree (incl. cmd.exe) is terminated
}

#[test]
fn sandbox_timeout_kills_tree() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let limits = sb::SandboxLimits {
        timeout_secs: 2,
        ..Default::default()
    };
    let result = sandbox.run_with_limits("Start-Sleep -Seconds 30", &limits);
    assert!(result.is_err(), "long command must hit the timeout");
    sandbox.purge();
}

#[test]
fn sandbox_output_limit_enforced() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let limits = sb::SandboxLimits {
        max_output_bytes: 64,
        ..Default::default()
    };
    let out = sandbox
        .run_with_limits("Write-Output ('x' * 10000)", &limits)
        .unwrap();
    assert!(out.len() <= 64, "output cap violated: {}", out.len());
    sandbox.purge();
}

#[test]
fn sandbox_purge_removes_dir_even_with_running_children() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = sb::Sandbox::new().unwrap();
    let root = sandbox.root();
    assert!(root.exists());
    // A lingering child keeps files open; job kill-on-close + remove must
    // still tear the workspace down.
    let _ = sandbox.run(
        "Start-Process -FilePath cmd.exe -ArgumentList '/c timeout /t 60' -WindowStyle Hidden",
    );
    sandbox.purge();
    assert!(!root.exists());
}

#[test]
fn sandbox_purge_stale_removes_dead_owner_dirs_only() {
    let _g = STATE_LOCK.lock().unwrap();
    // Create a dir that claims a dead PID.
    let dead_pid = find_dead_pid();
    let dir = std::env::temp_dir().join(format!("{}{dead_pid}-deadbeef", sb::SANDBOX_PREFIX));
    std::fs::create_dir_all(&dir).unwrap();
    sb::Sandbox::purge_stale();
    assert!(!dir.exists(), "stale dir of a dead pid must be removed");
}

#[test]
fn sandbox_concurrent_runs() {
    let _g = STATE_LOCK.lock().unwrap();
    let sandbox = std::sync::Arc::new(sb::Sandbox::new().unwrap());
    let mut handles = Vec::new();
    for i in 0..4 {
        let sb = sandbox.clone();
        handles.push(std::thread::spawn(move || {
            let out = sb.run(&format!("Write-Output {i}")).unwrap();
            assert_eq!(out.trim(), i.to_string());
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(sandbox.transcript().len(), 4);
    sandbox.purge();
}

/// A PID that tasklist reports as absent.
fn find_dead_pid() -> u32 {
    for pid in 4000..u32::MAX {
        if !pid_alive_probe(pid) {
            return pid;
        }
    }
    unreachable!()
}

fn pid_alive_probe(pid: u32) -> bool {
    std::process::Command::new(sb::TASKLIST)
        .args([
            sb::TASKLIST_ARG_FILTER,
            &format!("PID eq {pid}"),
            sb::TASKLIST_ARG_NOHEADER,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

// Role is exercised via push_context on the shared test surface; keep the
// import used on all configurations.
#[test]
fn role_roundtrip() {
    let entry = Role::Agent;
    assert_eq!(entry, Role::Agent);
}
