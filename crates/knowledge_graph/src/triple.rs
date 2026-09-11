use katgpt_core::sense::KgEmbedding;

/// A knowledge-graph triple: subject --relation--> object, with a signed
/// confidence. One triple maps to exactly one `KgEmbedding` leaf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triple {
    pub subject: u64,
    pub relation: u64,
    pub object: u64,
    /// Confidence in [0, 1]; feeds the module's mean-confidence bridge.
    pub confidence: f32,
    /// false = negated triple (object does NOT hold).
    pub sign: bool,
}

/// Stable blake3-derived 64-bit name hash (entity / relation ids).
pub fn hash_name(name: &str) -> u64 {
    let digest = blake3::hash(name.as_bytes());
    let bytes = digest.as_bytes();
    u64::from_le_bytes(bytes[0..8].try_into().expect("slice is 8 bytes"))
}

impl Triple {
    pub const fn new(subject: u64, relation: u64, object: u64) -> Self {
        Self {
            subject,
            relation,
            object,
            confidence: 1.0,
            sign: true,
        }
    }

    pub const fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }

    pub const fn negated(mut self) -> Self {
        self.sign = false;
        self
    }

    /// Convert to the katgpt-sense leaf used by the octree builder.
    ///
    /// `embedding` is the (schema-init or BAKE-refined) 8-dim latent for the
    /// subject entity; the triple's identity hashes ride along for Merkle
    /// leaf derivation.
    pub const fn to_embedding(&self, embedding: [f32; 8]) -> KgEmbedding {
        KgEmbedding {
            entity_hash: self.subject,
            relation_hash: self.relation,
            embedding,
            confidence: self.confidence,
            sign: self.sign,
        }
    }
}
