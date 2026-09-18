use std::fs;

use manager_rs::manager::{AGENTS_ROOT, Manager, RemoteRepo};
use proto_rs::GitTool;

#[test]
fn stale_work_tree_is_reclaimed_on_spawn() {
    let stale = std::path::Path::new(AGENTS_ROOT).join("stale-agent");
    fs::create_dir_all(stale.join("leftover")).expect("create stale tree");
    fs::write(stale.join("leftover").join("junk.txt"), "old run").expect("junk");

    let manager = Manager::new();
    manager.spawn("stale-agent").expect("spawn over stale tree");

    assert!(!stale.join("leftover").exists(), "stale content wiped");
    manager.finish("stale-agent").expect("finish");
}

#[test]
fn spawn_run_finish_round_trip() {
    let manager = Manager::new();
    let work_tree = manager.spawn("tester-1").expect("spawn");
    assert!(work_tree.starts_with(AGENTS_ROOT));

    let out = manager.run("tester-1", "echo hello").expect("run");
    assert_eq!(out.trim(), "hello");

    let agents = manager.snapshot();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].agent, "tester-1");
    assert_eq!(agents[0].runs, 1);

    let outcome = manager.finish("tester-1").expect("finish");
    assert_eq!(outcome.result.as_deref(), Some("hello\n"));
    assert!(!outcome.state.history.is_empty(), "transcript returned");
    assert!(!work_tree.join("sandbox").exists(), "sandbox purged");

    assert!(manager.snapshot().is_empty(), "agent removed after finish");
    assert!(manager.run("tester-1", "echo x").is_err());
}

#[test]
fn spawn_is_idempotent_per_agent() {
    let manager = Manager::new();
    let a = manager.spawn("dup").expect("spawn");
    let b = manager.spawn("dup").expect("spawn again");
    assert_eq!(a, b, "same work tree reused");
    assert_eq!(manager.snapshot().len(), 1);
    manager.finish("dup").expect("finish");
}

#[test]
fn run_unknown_agent_fails() {
    let manager = Manager::new();
    assert!(manager.run("ghost", "echo x").is_err());
    assert!(manager.finish("ghost").is_err());
}

#[test]
fn slot_lookup_rejects_every_operation_for_unknown_agent() {
    let manager = Manager::new();
    assert!(manager.push_context("ghost", "hello").is_err());
    assert!(manager.logs("ghost").is_err());
    assert!(manager.snapshot().is_empty());
}

#[test]
fn sanitize_maps_special_characters_in_work_tree_name() {
    let manager = Manager::new();
    let work_tree = manager.spawn("bad name/x.y").expect("spawn");
    assert_eq!(
        work_tree,
        std::path::Path::new(AGENTS_ROOT).join("bad_name_x_y")
    );
    assert!(work_tree.exists());
    manager.finish("bad name/x.y").expect("finish");
    assert!(!work_tree.exists(), "work tree purged");
}

#[test]
fn logs_reports_runs_transcript_and_last_result() {
    let manager = Manager::new();
    manager.spawn("logger").expect("spawn");
    manager.push_context("logger", "task notes").expect("push");
    manager.run("logger", "echo hi").expect("run");

    let logs = manager.logs("logger").expect("logs");
    assert_eq!(logs.agent, "logger");
    assert_eq!(
        logs.work_tree,
        std::path::Path::new(AGENTS_ROOT).join("logger")
    );
    assert_eq!(logs.runs, 1);
    assert!(logs.transcript.len() >= 2);
    assert!(logs.transcript[0].contains("task notes"));
    assert!(logs.transcript[1].contains("echo hi"));
    assert_eq!(logs.last_result.as_deref(), Some("hi\n"));

    let agents = manager.snapshot();
    assert_eq!(agents[0].runs, 1);
    manager.finish("logger").expect("finish");
}

#[test]
fn finish_without_runs_returns_empty_transcript_and_no_result() {
    let manager = Manager::new();
    manager.spawn("quiet").expect("spawn");

    let outcome = manager.finish("quiet").expect("finish");
    assert_eq!(outcome.agent, "quiet");
    assert_eq!(outcome.result, None);
    assert!(outcome.state.history.is_empty());
    assert_eq!(outcome.state.cwd, ".");
    assert_eq!(outcome.branch, None);
    assert!(manager.logs("quiet").is_err());
}

#[test]
fn per_task_spawns_are_isolated_slots_of_one_agent() {
    let manager = Manager::new();
    let a = manager.spawn_task("par", "42").expect("spawn task 42");
    let b = manager.spawn_task("par", "43").expect("spawn task 43");
    assert_ne!(a, b, "each task gets its own work tree");
    assert!(a.join("sandbox/workspace/.git").exists());
    assert!(b.join("sandbox/workspace/.git").exists());

    // re-spawn of the same (agent, task) pair reuses the slot
    assert_eq!(manager.spawn_task("par", "42").expect("again"), a);

    manager.run("par#42", "echo from-42").expect("run in 42");
    manager.run("par#43", "echo from-43").expect("run in 43");

    let agents = manager.snapshot();
    assert_eq!(agents.len(), 2, "two concurrent task slots");

    let out = manager.finish("par#42").expect("finish 42");
    assert_eq!(out.result.as_deref(), Some("from-42\n"));
    assert_eq!(out.branch, None, "empty tree has no commits -> no branch");
    assert!(!a.join("sandbox").exists(), "sandbox purged on finish");
    assert!(b.join("sandbox").exists(), "sibling task slot untouched");

    manager.finish("par#43").expect("finish 43");
    assert!(manager.snapshot().is_empty());
}

#[test]
fn publish_flow_reaches_pr_step() {
    // local file remote with one commit, so the spawn creates a task branch
    let remote_dir = std::env::temp_dir().join(format!(
        "manager-rs-remote-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let seed = remote_dir.join("seed");
    let repo = git_rs::GitRepo::init(&seed).expect("init seed");
    std::fs::write(seed.join("seed.txt"), "seed\n").expect("seed file");
    repo.commit_all("seed").expect("seed commit");
    drop(repo);
    git_rs::GitRepo::init_bare(&remote_dir.join("origin.git")).expect("bare");
    let remote_url = format!("file://{}", remote_dir.join("origin.git").display());
    let seed_repo = git_rs::GitRepo::open(&seed).expect("open seed");
    seed_repo
        .push_branch(&remote_url, git_rs::DEFAULT_BRANCH, None)
        .expect("push seed");

    let manager = Manager::new();
    manager
        .spawn_task_with_repo(
            "pub",
            "9",
            &RemoteRepo {
                url: remote_url.clone(),
                token: None,
            },
        )
        .expect("spawn task with repo");

    // the seed has commits, so spawn created + checked out the task branch
    // (a linked git worktree: .git is a gitlink file, not a directory)
    let wt_gitlink = std::path::Path::new(AGENTS_ROOT)
        .join("pub_9")
        .join("sandbox")
        .join("workspace")
        .join(".git");
    assert!(wt_gitlink.is_file(), "worktree .git gitlink file");
    let head_branch = git_rs::GitRepo::open(
        &std::path::Path::new(AGENTS_ROOT)
            .join("pub_9")
            .join("sandbox")
            .join("workspace"),
    )
    .unwrap()
    .current_branch()
    .unwrap();
    assert_eq!(head_branch, "task/9-pub");

    // bare remote inside the workspace, so the in-container push can reach
    // it at /workspace/remote.git
    let ws = std::path::Path::new(AGENTS_ROOT)
        .join("pub_9")
        .join("sandbox")
        .join("workspace");
    git_rs::GitRepo::init_bare(&ws.join("remote.git")).expect("init bare remote");

    // push to a remote visible inside the container (mounted at /workspace)
    // succeeds without auth, then the PR step fails: the url is not a
    // github.com repo, so the PR toolcall refuses before any API call
    let container_remote = "/workspace/remote.git";
    let host_remote = ws.join("remote.git").display().to_string();
    manager
        .git_tool(
            "pub#9",
            &GitTool::Commit {
                message: "task work".into(),
            },
        )
        .expect("commit");
    manager
        .git_tool(
            "pub#9",
            &GitTool::Push {
                branch: "task/9-pub".into(),
                url: Some(host_remote.to_owned()),
                token: Some("irrelevant".into()),
            },
        )
        .expect("push");
    let err = manager
        .git_tool(
            "pub#9",
            &GitTool::PullRequest {
                title: "task/9-pub".into(),
                head: "task/9-pub".into(),
                base: String::new(),
                url: Some(container_remote.to_owned()),
                token: Some("irrelevant".into()),
            },
        )
        .expect_err("pr against a non-github url");
    assert!(
        err.contains("github.com"),
        "expected pr-step error, got: {err}"
    );

    // the branch actually reached the remote
    let pushed = std::path::Path::new(AGENTS_ROOT)
        .join("pub_9")
        .join("sandbox")
        .join("workspace")
        .join("remote.git")
        .join("refs")
        .join("heads")
        .join("task")
        .join("9-pub")
        .exists();
    assert!(pushed, "task branch was pushed to the remote");

    manager.finish("pub#9").expect("finish");
    let _ = std::fs::remove_dir_all(&remote_dir);
}

/// Minimal fake GitHub pulls endpoint: answers one POST /repos/.../pulls
/// with 201 + a pulls url, capturing the request body for assertions.
fn spawn_fake_github() -> (u16, std::sync::Arc<std::sync::Mutex<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fake api");
    let port = listener.local_addr().unwrap().port();
    let body = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let captured = std::sync::Arc::clone(&body);
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 8192];
            let n = sock.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let payload = req.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
            *captured.lock().unwrap() = payload;
            let resp = "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\n".to_owned()
                + "\r\n{\"html_url\":\"https://github.com/acme/widget/pull/1\"}";
            let _ = sock.write_all(resp.as_bytes());
            let _ = sock.flush();
        }
    });
    (port, body)
}

#[test]
fn publish_e2e_push_and_pr_succeed() {
    // seed repo + remote, as the card runner would set up
    let remote_dir = std::env::temp_dir().join(format!(
        "manager-rs-e2e-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    let seed = remote_dir.join("seed");
    let repo = git_rs::GitRepo::init(&seed).expect("init seed");
    std::fs::write(seed.join("seed.txt"), "seed\n").expect("seed file");
    repo.commit_all("seed").expect("seed commit");
    drop(repo);
    git_rs::GitRepo::init_bare(&remote_dir.join("origin.git")).expect("bare");
    let remote_url = format!("file://{}", remote_dir.join("origin.git").display());
    let seed_repo = git_rs::GitRepo::open(&seed).expect("open seed");
    seed_repo
        .push_branch(&remote_url, git_rs::DEFAULT_BRANCH, None)
        .expect("push seed");

    // fake GitHub API for the PR step
    let (api_port, captured) = spawn_fake_github();
    // SAFETY: test-only process-global; publish is the only reader
    unsafe {
        std::env::set_var(
            "SUSUTAKU_GH_API_BASE",
            format!("http://127.0.0.1:{api_port}"),
        );
    }

    let manager = Manager::new();
    manager
        .spawn_task_with_repo(
            "e2e",
            "7",
            &RemoteRepo {
                url: remote_url.clone(),
                token: None,
            },
        )
        .expect("spawn task with repo");

    // the agent does real work in its own container
    manager
        .run("e2e#7", "echo e2e-proof > e2e-proof.txt")
        .expect("agent work");

    let ws = std::path::Path::new(AGENTS_ROOT)
        .join("e2e_7")
        .join("sandbox")
        .join("workspace");
    git_rs::GitRepo::init_bare(&ws.join("remote.git")).expect("init bare remote");

    manager
        .git_tool(
            "e2e#7",
            &GitTool::Commit {
                message: "task work".into(),
            },
        )
        .expect("commit");
    manager
        .git_tool(
            "e2e#7",
            &GitTool::Push {
                branch: "task/7-e2e".into(),
                url: Some(ws.join("remote.git").display().to_string()),
                token: Some("e2e-token".into()),
            },
        )
        .expect("push");
    let pr = manager
        .git_tool(
            "e2e#7",
            &GitTool::PullRequest {
                title: "task/7-e2e".into(),
                head: "task/7-e2e".into(),
                base: "main".into(),
                url: Some("https://github.com/acme/widget".to_owned()),
                token: Some("e2e-token".into()),
            },
        )
        .expect("pr must succeed end to end");
    assert!(pr.contains("201"), "PR answered 201: {pr}");
    assert!(pr.contains("pull/1"), "PR url returned: {pr}");

    // the fake API received the right PR request
    let payload = captured.lock().unwrap().clone();
    assert!(
        payload.contains("task/7-e2e"),
        "pr head in payload: {payload}"
    );
    assert!(
        payload.contains("\"base\":\"main\""),
        "pr base in payload: {payload}"
    );

    // the pushed remote carries the agent's commit
    let bare = git_rs::GitRepo::open(&ws.join("remote.git")).expect("open pushed remote");
    let pushed_ref = bare.head_oid().map(|oid| oid.to_string()).ok();
    let pushed_oid = std::fs::read_to_string(
        ws.join("remote.git")
            .join("refs")
            .join("heads")
            .join("task")
            .join("7-e2e"),
    )
    .map(|s| s.trim().to_string())
    .ok();
    let oid = pushed_oid
        .or(pushed_ref)
        .expect("task branch was pushed to the remote");
    assert!(!oid.is_empty(), "pushed branch has the task commit");

    let outcome = manager.finish("e2e#7").expect("finish");
    assert_eq!(outcome.branch.as_deref(), Some("task/7-e2e"));
    unsafe { std::env::remove_var("SUSUTAKU_GH_API_BASE") };
    let _ = std::fs::remove_dir_all(&remote_dir);
}
