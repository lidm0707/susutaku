use katgpt_types::SenseKind;
use knowledge_graph::{EntityEmbedder, KnowledgeGraph, Triple, hash_name};

#[test]
fn triple_hashes_are_stable() {
    assert_eq!(hash_name("agent"), hash_name("agent"));
    assert_ne!(hash_name("agent"), hash_name("agent-2"));
}

#[test]
fn roundtrip_and_commitment() {
    let subject = hash_name("agent");
    let relation = hash_name("knows");
    let object = hash_name("rust");

    let mut kg = KnowledgeGraph::new();
    kg.add(
        Triple::new(subject, relation, object),
        [0.5, -0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );
    kg.add(
        Triple::new(subject, relation, object)
            .with_confidence(0.5)
            .negated(),
        [-0.5, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );

    assert_eq!(kg.len(), 2);

    let module = kg.module(SenseKind::SpatialSense);
    assert!(module.verify());
    assert_eq!(module.n_directions, 2);

    // Flat commitment vs Merkle root: same leaves, both verify.
    let (merkle_module, tree) = kg.module_with_merkle(SenseKind::SpatialSense);
    assert_eq!(merkle_module.commitment, *tree.root());

    let proof = katgpt_types::MerkleProof::generate(&tree, 0).expect("proof");
    assert!(proof.verify(tree.root()));
}

#[test]
fn merkle_tree_is_deterministic() {
    let t = Triple::new(hash_name("a"), hash_name("r"), hash_name("b"));
    let mut kg_a = KnowledgeGraph::new();
    kg_a.add(t, [0.1; 8]);
    let mut kg_b = KnowledgeGraph::new();
    kg_b.add(t, [0.1; 8]);

    assert_eq!(kg_a.merkle_tree().root(), kg_b.merkle_tree().root());
}

#[test]
fn schema_init_near_centroid_and_bake_refines() {
    let embedder = EntityEmbedder::new();
    let class = hash_name("tool");

    let members: Vec<[f32; 8]> = (0..5)
        .map(|i| {
            let mut v = [0.0f32; 8];
            v[0] = 1.0 + i as f32 * 0.1;
            v
        })
        .collect();
    assert!(embedder.refresh_class(class, &members));

    let mut rng = fastrand::Rng::with_seed(42);
    let (embedding, precision) = embedder.new_entity(&[class], &mut rng);

    // Informed prior: dense class -> precision = n/(n+1) = 5/6.
    let expected = 5.0f32 / 6.0;
    assert!((precision[0] - expected).abs() < 1e-4);
    // Centroid seed: embedding[0] near mean 1.2.
    assert!((embedding[0] - 1.2).abs() < 0.5);

    // BAKE: session observations pull the mean toward the observation.
    let mut session = embedder.session(7);
    for _ in 0..4 {
        session.observe(&[4.0; 8]);
    }
    embedder.end_session(session);

    let mean = embedder.mean(7);
    assert!((mean[0] - 4.0).abs() < 3.2); // moved toward 4.0 from 0
    assert!(embedder.confidence(7) > embedder.confidence(999));
}

#[test]
fn unknown_class_falls_back_to_random() {
    let embedder = EntityEmbedder::new();
    let mut rng = fastrand::Rng::with_seed(1);
    let (embedding, precision) = embedder.new_entity(&[404], &mut rng);

    assert!(precision.iter().all(|&p| (p - 0.1).abs() < 1e-6));
    assert!(embedding.iter().any(|&v| v != 0.0));
}
