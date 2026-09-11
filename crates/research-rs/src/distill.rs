use katgpt_pruners::{
    BanditPruner, BanditStrategy, ConvergenceMetrics, Episode, EpisodeLookup, MemoryEpisodeLookup,
    SelfDistillingBandit, SelfDistillingConfig, compute_match_ratio,
};
use katgpt_speculative::{NoScreeningPruner, ScreeningPruner};

pub const DEFAULT_ALPHA: f32 = 0.3;
pub const DEFAULT_REWARD_K: f32 = 4.0;
pub const DEFAULT_MIN_DOMAIN_SAMPLES: usize = 10;
pub const DEFAULT_MAX_DOMAINS: usize = 64;
pub const DEFAULT_CONVERGENCE_WINDOW: usize = 100;
pub const DEFAULT_NUM_ARMS: usize = 8;

pub fn match_ratio(generated: &[usize], reference: &[usize]) -> f32 {
    compute_match_ratio(generated, reference)
}

#[derive(Clone, Debug)]
pub struct DistillConfig {
    pub alpha: f32,
    pub reward_k: f32,
    pub min_domain_samples: usize,
    pub max_domains: usize,
    pub convergence_window: usize,
    pub num_arms: usize,
    pub strategy: BanditStrategy,
}

impl Default for DistillConfig {
    fn default() -> Self {
        Self {
            alpha: DEFAULT_ALPHA,
            reward_k: DEFAULT_REWARD_K,
            min_domain_samples: DEFAULT_MIN_DOMAIN_SAMPLES,
            max_domains: DEFAULT_MAX_DOMAINS,
            convergence_window: DEFAULT_CONVERGENCE_WINDOW,
            num_arms: DEFAULT_NUM_ARMS,
            strategy: BanditStrategy::Ucb1,
        }
    }
}

impl From<DistillConfig> for SelfDistillingConfig {
    fn from(c: DistillConfig) -> Self {
        Self {
            alpha: c.alpha,
            k: c.reward_k,
            min_domain_samples: c.min_domain_samples,
            max_domains: c.max_domains,
            convergence_window: c.convergence_window,
        }
    }
}

/// Episode-guided distillation session over a bandit of screening pruners.
///
/// Thin wrapper around katgpt's `SelfDistillingBandit`: after each generated
/// sequence, feed the arm, generated tokens, and acceptance reward — the
/// blended episode/acceptance reward updates the inner bandit.
pub struct DistillSession<P: ScreeningPruner, L: EpisodeLookup> {
    bandit: SelfDistillingBandit<P, L>,
}

impl<P: ScreeningPruner, L: EpisodeLookup> DistillSession<P, L> {
    pub fn new(inner: BanditPruner<P>, lookup: L, config: DistillConfig) -> Self {
        Self {
            bandit: SelfDistillingBandit::new(inner, lookup, SelfDistillingConfig::from(config)),
        }
    }

    pub fn observe(
        &mut self,
        prompt_hash: u64,
        arm: usize,
        generated: &[usize],
        acceptance_reward: f32,
        domain_hash: u64,
    ) {
        self.bandit
            .episode_update(prompt_hash, arm, generated, acceptance_reward, domain_hash);
    }

    pub fn best_arm(&self, domain_hash: u64) -> usize {
        self.bandit.best_arm_for_domain(domain_hash)
    }

    pub fn convergence(&self) -> ConvergenceMetrics {
        self.bandit.convergence_metrics()
    }
}

/// Convenience constructor: fresh `NoScreeningPruner` bandit + in-memory episode store.
pub fn mem_session(
    config: DistillConfig,
) -> DistillSession<NoScreeningPruner, MemoryEpisodeLookup> {
    DistillSession::new(
        BanditPruner::new(NoScreeningPruner, config.strategy.clone(), config.num_arms),
        MemoryEpisodeLookup::new(),
        config,
    )
}

/// Add an episode reference so later `observe` calls blend the match reward.
pub fn store_episode(
    store: &mut MemoryEpisodeLookup,
    prompt_hash: u64,
    reference_tokens: Vec<usize>,
) {
    store.insert(Episode {
        prompt_hash,
        reference_tokens,
        metadata: Default::default(),
    });
}
