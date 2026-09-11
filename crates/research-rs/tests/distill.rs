use research_rs::distill::{DistillConfig, match_ratio, mem_session};

const DOMAIN_A: u64 = 0xA11CE;
const PROMPT_A: u64 = 0xBEEF;
const OBSERVE_ROUNDS: usize = 20;

#[test]
fn match_ratio_identical_is_one() {
    let seq = [1usize, 2, 3, 4];
    assert!((match_ratio(&seq, &seq) - 1.0).abs() < f32::EPSILON);
}

#[test]
fn match_ratio_disjoint_is_zero() {
    assert!(match_ratio(&[1, 2], &[3, 4]).abs() < f32::EPSILON);
}

#[test]
fn session_learns_best_arm_from_rewards() {
    let mut session = mem_session(DistillConfig::default());

    for _ in 0..OBSERVE_ROUNDS {
        session.observe(PROMPT_A, 1, &[1, 2, 3], 0.9, DOMAIN_A);
        session.observe(PROMPT_A, 0, &[9, 9, 9], 0.1, DOMAIN_A);
    }
    assert_eq!(session.best_arm(DOMAIN_A), 1);

    let metrics = session.convergence();
    assert!(metrics.total_updates >= 2 * OBSERVE_ROUNDS);
}

#[test]
fn convergence_tracks_updates() {
    let mut session = mem_session(DistillConfig::default());

    let metrics = session.convergence();
    assert_eq!(metrics.total_updates, 0);
    session.observe(PROMPT_A, 0, &[1], 0.5, DOMAIN_A);
    assert_eq!(session.convergence().total_updates, 1);
}
