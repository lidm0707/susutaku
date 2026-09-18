//! Toolcall parse bench + robustness regression.
//!
//! Run: cargo test -p backend --test toolcall_parse_bench -- --nocapture
//! (summary lives in bench/toolcall-parse.md)
//!
//! Two groups:
//! - STABLE: must parse (or must reject) today — asserted, fails loudly on
//!   regression.
//! - KNOWN_GAP: currently unreliable by design (mid-prose markers, partial
//!   XML) — measured and reported, not asserted, so the bench stays green
//!   while the parser is improved.

use backend::domain::ToolCall;
use std::time::Instant;

enum Expect {
    /// Must yield a tool call.
    Parse,
    /// Must yield None.
    Reject,
    /// Measured informationally: today's behavior is a known gap.
    KnownGap,
}

fn stable_cases() -> Vec<(&'static str, String, Expect)> {
    vec![
        (
            "clean shell line",
            "TOOL: SHELL ls -la\n".to_string(),
            Expect::Parse,
        ),
        (
            "lowercase marker",
            "tool: board_list".to_string(),
            Expect::Parse,
        ),
        (
            "after think block",
            "<think>reasoning here</think>\n\nTOOL: CARD_FIND fix login\n".to_string(),
            Expect::Parse,
        ),
        (
            "markdown fenced",
            "Sure, let me look.\n```\nTOOL: SHELL cargo check\n```\n".to_string(),
            Expect::Parse,
        ),
        (
            "git status",
            "TOOL: GIT STATUS".to_string(),
            Expect::Parse,
        ),
        (
            "git push branch",
            "TOOL: GIT PUSH feature/x".to_string(),
            Expect::Parse,
        ),
        (
            "lsp definition",
            "TOOL: LSP DEFINITION src/main.rs 10 5".to_string(),
            Expect::Parse,
        ),
        (
            "math alias geomath",
            "TOOL: GEOMATH area(3,4)".to_string(),
            Expect::Parse,
        ),
        (
            "xml coding block",
            "<invoke name=\"coding\"><parameter name=\"path\">src/a.rs</parameter><parameter name=\"code\">fn main() {}</parameter></invoke>".to_string(),
            Expect::Parse,
        ),
        (
            "xml write_file alias",
            "<invoke name=\"write_file\"><parameter name=\"path\">a.txt</parameter><parameter name=\"code\">hi</parameter></invoke>".to_string(),
            Expect::Parse,
        ),
        (
            "xml shell",
            "<invoke name=\"shell\"><parameter name=\"command\">ls</parameter></invoke>".to_string(),
            Expect::Parse,
        ),
        (
            "prose then clean line",
            "I checked the failing test.\nTOOL: SHELL cargo test\n".to_string(),
            Expect::Parse,
        ),
        (
            "plain answer no tool",
            "The task is done, all tests pass.".to_string(),
            Expect::Reject,
        ),
        (
            "unknown kind after prefix",
            "TOOL: is the marker used by this system".to_string(),
            Expect::Reject,
        ),
        (
            "malformed card_create missing id",
            "TOOL: CARD_CREATE title only".to_string(),
            Expect::Reject,
        ),
    ]
}

/// Cases the strict line scanner provably misses today.
fn known_gap_cases() -> Vec<(&'static str, String)> {
    vec![
        (
            "mid-prose marker",
            "I'll run it now. TOOL: SHELL ls -la".to_string(),
        ),
        (
            "partial xml invoke",
            "<invoke name=\"shell\"><parameter name=\"command\">ls</parameter>".to_string(),
        ),
        (
            "xml coding missing code param",
            "<invoke name=\"coding\"><parameter name=\"path\">src/a.rs</parameter></invoke>"
                .to_string(),
        ),
    ]
}

fn run_group(cases: &[(&'static str, String, Expect)], rounds: u32) -> (usize, usize, usize, f64) {
    let mut pass = 0;
    let mut parsed = 0;
    let mut ns_sum = 0.0;
    for (name, reply, expect) in cases {
        let got = ToolCall::parse(reply).is_some();
        if got {
            parsed += 1;
        }
        let ok = match expect {
            Expect::Parse => got,
            Expect::Reject => !got,
            Expect::KnownGap => true,
        };
        if ok {
            pass += 1;
        } else {
            eprintln!("BENCH FAIL [{name}] parsed={got}");
        }
        let start = Instant::now();
        for _ in 0..rounds {
            let _ = ToolCall::parse(reply);
        }
        ns_sum += start.elapsed().as_nanos() as f64 / rounds as f64;
    }
    (pass, parsed, cases.len(), ns_sum / cases.len() as f64)
}

#[test]
fn toolcall_parse_robustness_and_speed() {
    const ROUNDS: u32 = 2000;

    let (pass, parsed, total, ns) = run_group(&stable_cases(), ROUNDS);
    println!("\n=== toolcall parse bench ===");
    println!(
        "stable   : {pass}/{total} classified correctly ({parsed} parsed), {ns:.0} ns/parse avg"
    );
    assert_eq!(pass, total, "stable toolcall behavior regressed");

    let gaps: Vec<_> = known_gap_cases()
        .into_iter()
        .map(|(n, r)| (n, r, Expect::KnownGap))
        .collect();
    let (gpass, gparsed, gtotal, gns) = run_group(&gaps, ROUNDS);
    println!(
        "knowngap : {gparsed}/{gtotal} parsed (unstable by design, informational), {gns:.0} ns/parse avg — {gpass}/{gtotal} expectations met"
    );
}
