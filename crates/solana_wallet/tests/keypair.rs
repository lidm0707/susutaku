use solana_wallet::keypair::LAMPORTS_PER_SOL;
use solana_wallet::keypair::WalletKeypair;

#[test]
fn generate_gives_valid_base58_address() {
    let kp = WalletKeypair::generate();
    let addr = kp.address();
    assert!(!addr.is_empty());
    assert!(addr.chars().all(|c| !"0OIl".contains(c)));
}

#[test]
fn sign_verify_roundtrip() {
    let kp = WalletKeypair::generate();
    let msg = b"hello susutaku";
    let sig = kp.sign(msg);
    assert!(WalletKeypair::verify(&kp.address(), msg, &sig));
    assert!(!WalletKeypair::verify(&kp.address(), b"tampered", &sig));
}

#[test]
fn secret_roundtrip_reproduces_address() {
    let kp = WalletKeypair::generate();
    let restored = WalletKeypair::from_secret_bytes(&kp.secret_bytes()).expect("valid seed");
    assert_eq!(kp.address(), restored.address());
}

#[test]
fn rejects_bad_seed_length() {
    assert!(WalletKeypair::from_secret_bytes(&[0u8; 31]).is_err());
}

#[test]
fn lamports_constant() {
    assert_eq!(LAMPORTS_PER_SOL, 1_000_000_000);
}
