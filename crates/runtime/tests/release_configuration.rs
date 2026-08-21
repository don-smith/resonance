use std::{fs, path::Path};

use resonance_runtime::release::ReleaseConfiguration;

const VALID_CONFIGURATION: &str = r#"{
  "manifestEndpoint": "https://updates.example.invalid/latest.json",
  "artifactBaseUrl": "https://downloads.example.invalid/releases/v0.1.0",
  "publicKey": "ZWQyNTUxOS1wdWJsaWMta2V5LWZvci10ZXN0cy1vbmx5LW5vdC1hLXNlY3JldA==",
  "targets": {
    "darwin-aarch64": { "artifact": "Resonance_aarch64.app.tar.gz" },
    "darwin-x86_64": { "artifact": "Resonance_x64.app.tar.gz" },
    "windows-x86_64": { "artifact": "Resonance_x64-setup.nsis.zip" }
  }
}"#;

fn write_fixture(root: &Path, contents: &str) -> std::path::PathBuf {
    let path = root.join("release.json");
    fs::write(&path, contents).expect("fixture writes");
    path
}

#[test]
fn accepts_a_complete_test_only_static_configuration() {
    let root = tempfile::tempdir().expect("temporary directory");
    let configuration = ReleaseConfiguration::load(write_fixture(root.path(), VALID_CONFIGURATION))
        .expect("valid configuration loads");

    assert_eq!(
        configuration.manifest_endpoint.as_str(),
        "https://updates.example.invalid/latest.json"
    );
    assert_eq!(configuration.targets.len(), 3);
}

#[test]
fn rejects_absent_placeholder_and_non_https_configuration() {
    let root = tempfile::tempdir().expect("temporary directory");
    let absent = root.path().join("absent.json");
    assert!(ReleaseConfiguration::load(absent).is_err());

    let placeholder = VALID_CONFIGURATION.replace(
        "ZWQyNTUxOS1wdWJsaWMta2V5LWZvci10ZXN0cy1vbmx5LW5vdC1hLXNlY3JldA==",
        "REPLACE_WITH_A_PUBLIC_KEY",
    );
    assert!(ReleaseConfiguration::load(write_fixture(root.path(), &placeholder)).is_err());

    let insecure = VALID_CONFIGURATION.replace("https://", "http://");
    assert!(ReleaseConfiguration::load(write_fixture(root.path(), &insecure)).is_err());
}

#[test]
fn rejects_incomplete_target_metadata() {
    let root = tempfile::tempdir().expect("temporary directory");
    let incomplete = VALID_CONFIGURATION.replace(
        "    \"windows-x86_64\": { \"artifact\": \"Resonance_x64-setup.nsis.zip\" }\n",
        "",
    );
    assert!(ReleaseConfiguration::load(write_fixture(root.path(), &incomplete)).is_err());
}
