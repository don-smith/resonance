use commonware_codec::DecodeExt;
use commonware_cryptography::{ed25519, Signer, Verifier};
use resonance_runtime::identity::{IdentityError, InMemoryKeyCustody, InstallationIdentity};

#[test]
fn creates_one_keychain_secret_then_reloads_a_stable_public_identity() {
    let custody = InMemoryKeyCustody::default();

    let created = InstallationIdentity::load_or_create(&custody).expect("identity creates");
    let reloaded = InstallationIdentity::load_or_create(&custody).expect("identity reloads");

    assert_eq!(created.public_identity(), reloaded.public_identity());
    assert_eq!(custody.stored_secret_len(), Some(32));
}

#[test]
fn the_installation_secret_has_the_same_iroh_and_commonware_public_key() {
    let secret = [42; 32];
    let custody = InMemoryKeyCustody::with_secret(secret.to_vec());
    let identity = InstallationIdentity::load_or_create(&custody).expect("identity loads");
    let commonware = ed25519::PrivateKey::decode(secret.as_slice()).expect("secret decodes");

    assert_eq!(
        commonware.public_key().as_ref(),
        identity.public_identity().as_bytes()
    );
    assert_eq!(custody.stored_secret_len(), Some(32));
}

#[test]
fn the_commonware_view_of_the_installation_secret_signs_and_verifies() {
    let secret = [73; 32];
    let custody = InMemoryKeyCustody::with_secret(secret.to_vec());
    let identity = InstallationIdentity::load_or_create(&custody).expect("identity loads");
    let commonware = ed25519::PrivateKey::decode(secret.as_slice()).expect("secret decodes");
    let namespace = b"resonance.identity-commonware.test";
    let message = b"same installation signer";
    let signature = commonware.sign(namespace, message);

    assert_eq!(
        commonware.public_key().as_ref(),
        identity.public_identity().as_bytes()
    );
    assert!(commonware
        .public_key()
        .verify(namespace, message, &signature));
    assert!(!commonware
        .public_key()
        .verify(namespace, b"different message", &signature));
    assert_eq!(custody.stored_secret_len(), Some(32));
}

#[test]
fn rejects_malformed_keychain_bytes_without_replacing_them() {
    let custody = InMemoryKeyCustody::with_secret(vec![7; 31]);

    assert!(matches!(
        InstallationIdentity::load_or_create(&custody),
        Err(IdentityError::MalformedStoredSecret)
    ));
    assert_eq!(custody.stored_secret_len(), Some(31));
}

#[test]
fn reports_a_failed_keychain_write() {
    let custody = InMemoryKeyCustody::failing_write();

    assert!(matches!(
        InstallationIdentity::load_or_create(&custody),
        Err(IdentityError::StoreUnavailable)
    ));
    assert_eq!(custody.stored_secret_len(), None);
}

#[test]
fn never_treats_a_non_missing_read_error_as_a_new_identity() {
    let custody = InMemoryKeyCustody::failing_read();

    assert!(matches!(
        InstallationIdentity::load_or_create(&custody),
        Err(IdentityError::StoreUnavailable)
    ));
    assert_eq!(custody.stored_secret_len(), None);
}
