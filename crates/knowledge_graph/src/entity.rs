use fastrand::Rng;
use katgpt_core::sense::{
    BakePrecisionStore, BakeSession, KgEmbedding, SchemaCentroidCache, informed_prior_precision,
    precision_to_confidence, schema_init_with_precision,
};

/// Perturbation strength for centroid-based init (one-sigma scale).
pub const INIT_GAMMA: f32 = 0.3;

/// Creates, initializes, and refines entity embeddings.
///
/// - Init: schema centroids (Plan 237) — new entities start near their class
///   centroid with informed prior precision, no cold-start randomness.
/// - Refine: BAKE precision-gated Bayesian updates (Plan 236) — high
///   precision resists change, low precision absorbs eagerly.
pub struct EntityEmbedder {
    centroids: SchemaCentroidCache,
    precision: BakePrecisionStore,
}

impl Default for EntityEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityEmbedder {
    pub fn new() -> Self {
        Self {
            centroids: SchemaCentroidCache::new(),
            precision: BakePrecisionStore::new(),
        }
    }

    /// Recompute a class centroid from current class members (call after KG
    /// snapshot updates). Returns false for an empty class.
    pub fn refresh_class(&self, class_hash: u64, embeddings: &[[f32; 8]]) -> bool {
        let leaves: Vec<_> = embeddings
            .iter()
            .map(|&embedding| zero_ident_leaves(embedding))
            .collect();
        self.centroids.compute_and_insert(class_hash, &leaves)
    }

    /// Initialize a new entity from its schema classes.
    ///
    /// Returns `(embedding, precision)`. Unknown classes fall back to random
    /// init + uninformative prior.
    pub fn new_entity(&self, class_hashes: &[u64], rng: &mut Rng) -> ([f32; 8], [f32; 8]) {
        schema_init_with_precision(class_hashes, &self.centroids, INIT_GAMMA, rng)
    }

    /// Record one observation of the entity's latent (e.g. from an agent
    /// interaction). `lambda_obs` is the observation precision (1.0 default).
    pub fn observe(&self, entity_hash: u64, observation: &[f32; 8], lambda_obs: f32) {
        self.precision.update(entity_hash, observation, lambda_obs);
    }

    /// Batch a session's observations into one Bayesian update.
    pub fn session(&self, entity_hash: u64) -> BakeSession {
        BakeSession::begin(entity_hash, &self.precision)
    }

    /// Commit a session: applies the batched update and stores the result.
    pub fn end_session(&self, session: BakeSession) {
        session.end(&self.precision);
    }

    /// Current BAKE posterior mean for an entity (`[0.0; 8]` if untracked).
    pub fn mean(&self, entity_hash: u64) -> [f32; 8] {
        self.precision.snapshot_mean(entity_hash)
    }

    /// Effective confidence in [0, 1] derived from BAKE precision.
    pub fn confidence(&self, entity_hash: u64) -> f32 {
        self.precision
            .get(entity_hash)
            .map(|e| precision_to_confidence(&e.precision))
            .unwrap_or(0.0)
    }

    /// Informed prior precision for a class of `count` members.
    pub fn informed_prior(&self, class_count: usize) -> [f32; 8] {
        informed_prior_precision(class_count)
    }
}

/// Centroid inputs only need the latent values; identity hashes are unused
/// by `compute_centroid`, so fill them with zeros.
const fn zero_ident_leaves(embedding: [f32; 8]) -> KgEmbedding {
    KgEmbedding {
        entity_hash: 0,
        relation_hash: 0,
        embedding,
        confidence: 1.0,
        sign: true,
    }
}
