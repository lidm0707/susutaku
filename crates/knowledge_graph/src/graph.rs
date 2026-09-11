use crate::entity::EntityEmbedder;
use crate::triple::Triple;
use katgpt_core::sense::{KgEmbedding, SenseOctreeBuilder};
use katgpt_types::{MerkleOctree, SenseKind, SenseModule};

/// Octree depth cap enforced by katgpt-sense.
pub const MAX_OCTREE_DEPTH: u8 = 3;

/// In-memory knowledge graph: triples + per-subject latent embeddings,
/// compiled into a committed SenseModule (flat BLAKE3 or Merkle octree).
pub struct KnowledgeGraph {
    builder: SenseOctreeBuilder,
    triples: Vec<Triple>,
    embeddings: Vec<[f32; 8]>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            builder: SenseOctreeBuilder::new(MAX_OCTREE_DEPTH),
            triples: Vec::new(),
            embeddings: Vec::new(),
        }
    }

    /// Insert a triple with the subject's current latent (e.g. the BAKE mean
    /// from an `EntityEmbedder`, or a schema-init embedding).
    pub fn add(&mut self, triple: Triple, embedding: [f32; 8]) {
        self.triples.push(triple);
        self.embeddings.push(embedding);
    }

    pub fn triples(&self) -> &[Triple] {
        &self.triples
    }

    pub fn len(&self) -> usize {
        self.triples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.triples.is_empty()
    }

    /// Compile to a SenseModule with a flat BLAKE3 commitment.
    pub fn module(&self, kind: SenseKind) -> SenseModule {
        self.builder.build(kind, &self.leaves())
    }

    /// Compile to a SenseModule committed by a Merkle octree root —
    /// per-triple inclusion proofs become verifiable against the root.
    pub fn module_with_merkle(&self, kind: SenseKind) -> (SenseModule, MerkleOctree) {
        self.builder.build_with_merkle(kind, &self.leaves())
    }

    /// Merkle octree over the triples alone (no SenseModule).
    pub fn merkle_tree(&self) -> MerkleOctree {
        SenseOctreeBuilder::build_merkle_only(&self.leaves())
    }

    fn leaves(&self) -> Vec<KgEmbedding> {
        self.triples
            .iter()
            .zip(&self.embeddings)
            .map(|(t, &e)| t.to_embedding(e))
            .collect()
    }
}

/// Convenience: build a KG from an embedder's posterior means.
pub fn from_embedder(triples: &[Triple], embedder: &EntityEmbedder) -> KnowledgeGraph {
    let mut kg = KnowledgeGraph::new();
    for t in triples {
        kg.add(*t, embedder.mean(t.subject));
    }
    kg
}
