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
