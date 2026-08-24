use std::{collections::BTreeMap, fs};

use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    local_root_binding::{
        LocalChange, LocalRootBinding, RootBindingError, RootHealth, RootSelection,
    },
    membership_log::MembershipProjection,
    workspace_domain::Member,
    workspace_files::{
        authority::WorkspaceFileAuthority,
        blobs::WorkspaceBlobStore,
        projection::{FileRevision, FileTreeProjection, TreeNode},
        FileOperationBody, SignedFileOperation,
    },
    workspace_store::WorkspaceStore,
};

fn plans_projection(file: Option<(&str, &[u8])>) -> (FileTreeProjection, WorkspaceBlobStore) {
    let mut blobs = WorkspaceBlobStore::new();
    let mut revisions = BTreeMap::new();
    let mut children = BTreeMap::new();
    if let Some((name, bytes)) = file {
        let hash = blobs.store(bytes).expect("blob stores");
        revisions.insert(
            "revision-one".to_owned(),
            FileRevision {
                node_id: "file-node".to_owned(),
                revision_id: "revision-one".to_owned(),
                base_revision_id: None,
                content_hash: hash.as_str().to_owned(),
                mime_type: if name.ends_with(".md") {
                    "text/markdown"
                } else {
                    "application/octet-stream"
                }
                .to_owned(),
                byte_length: bytes.len() as u64,
                signer: [1; 32],
            },
        );
        children.insert(
            name.to_owned(),
            TreeNode::File {
                node_id: "file-node".to_owned(),
                name: name.to_owned(),
                current_revision_id: "revision-one".to_owned(),
            },
        );
    }
    let mut root = BTreeMap::new();
    root.insert(
        "plans".to_owned(),
        TreeNode::Directory {
            node_id: "plans-node".to_owned(),
            name: "plans".to_owned(),
            children,
        },
    );
    (
        FileTreeProjection {
            root,
            revisions,
            conflicts: Vec::new(),
            ignore_set: Default::default(),
        },
        blobs,
    )
}

#[test]
fn binds_a_new_root_projects_plans_and_persists_private_binding_state() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let selected = app_data.path().join("selected-root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(None);

    let binding = LocalRootBinding::bind(
        &store,
        &selected,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");

    assert_eq!(binding.health(), RootHealth::Healthy);
    assert!(selected.join("plans").is_dir());
    assert_eq!(fs::read_dir(&selected).expect("root reads").count(), 1);
    assert_eq!(
        store.local_root_health().expect("private binding reads"),
        Some(RootHealth::Healthy)
    );
}

#[test]
fn two_private_roots_materialize_the_same_tree_and_bytes() {
    let first_data = tempfile::tempdir().expect("first app data creates");
    let second_data = tempfile::tempdir().expect("second app data creates");
    let first_root = first_data.path().join("root-a");
    let second_root = second_data.path().join("root-b");
    let first_store = WorkspaceStore::open(first_data.path(), "workspace").expect("store opens");
    let second_store = WorkspaceStore::open(second_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("roadmap.md", b"# Roadmap\n")));

    LocalRootBinding::bind(
        &first_store,
        &first_root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("first root binds");
    LocalRootBinding::bind(
        &second_store,
        &second_root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("second root binds");

    assert_eq!(
        fs::read(first_root.join("plans/roadmap.md")).expect("first bytes read"),
        fs::read(second_root.join("plans/roadmap.md")).expect("second bytes read")
    );
    assert!(fs::read_dir(first_root.join("plans"))
        .expect("first plans reads")
        .all(|entry| !entry
            .expect("entry reads")
            .file_name()
            .to_string_lossy()
            .starts_with(".resonance-")));
}

#[test]
fn repair_relocates_a_materialized_concurrent_create_loser() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let membership = MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    };
    let mut authority = WorkspaceFileAuthority::new("workspace");
    let plans =
        SignedFileOperation::create_directory(&identity, "workspace", None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let bytes = b"materialized file\n";
    let hash = authority
        .blob_store_mut()
        .store(bytes)
        .expect("file bytes store");
    let file = SignedFileOperation::create_file(
        &identity,
        "workspace",
        Some(plans.operation.node_id.clone()),
        "tree-collision",
        hash.as_str(),
        "application/octet-stream",
        bytes.len() as u64,
        vec![plans.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &authority.projection(),
        authority.blob_store(),
    )
    .expect("file projects");
    let directory = (0..100)
        .map(|_| {
            SignedFileOperation::create_directory(
                &identity,
                "workspace",
                Some(plans.operation.node_id.clone()),
                "tree-collision",
                vec![plans.operation.operation_id.clone()],
            )
            .expect("directory signs")
        })
        .find(|candidate| candidate.operation.operation_id < file.operation.operation_id)
        .expect("lower ordered directory operation generates");
    authority
        .apply(&directory, &membership)
        .expect("concurrent directory applies");

    binding
        .repair(&authority.projection(), authority.blob_store())
        .expect("known file relocates before directory projects");

    assert!(root.join("plans/tree-collision").is_dir());
    let sibling = root.join(format!(
        "plans/tree-collision.resonance-conflict-{}",
        &file.operation.operation_id[..8]
    ));
    assert_eq!(fs::read(&sibling).expect("losing file remains"), bytes);
    assert_eq!(binding.health(), RootHealth::Healthy);

    let conflict = authority
        .projection()
        .conflicts
        .into_iter()
        .find(|conflict| !conflict.resolved)
        .expect("create conflict remains");
    let resolution = SignedFileOperation::resolve_conflict(
        &identity,
        "workspace",
        conflict.node_id.clone(),
        conflict.record_id,
        Some(file.operation.operation_id.clone()),
        vec![
            file.operation.operation_id.clone(),
            directory.operation.operation_id.clone(),
        ],
    )
    .expect("directory resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("directory resolution applies");
    binding
        .repair(&authority.projection(), authority.blob_store())
        .expect("stale file leaves before selected directory relocates");

    assert_eq!(
        fs::read(root.join("plans/tree-collision")).expect("selected file remains"),
        bytes
    );
    assert!(!sibling.exists());
    assert_eq!(
        fs::read_dir(root.join("plans"))
            .expect("plans reads")
            .count(),
        1
    );
}

#[test]
fn rejects_nonempty_git_managed_and_unconfirmed_roots() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(None);

    let nonempty = app_data.path().join("nonempty");
    fs::create_dir(&nonempty).expect("nonempty creates");
    fs::write(nonempty.join("existing.txt"), b"data").expect("file writes");
    assert_eq!(
        LocalRootBinding::bind(
            &store,
            &nonempty,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        )
        .expect_err("nonempty root rejects"),
        RootBindingError::NonEmpty
    );

    let git_root = app_data.path().join("git-root");
    fs::create_dir(&git_root).expect("git root creates");
    fs::create_dir(git_root.join(".git")).expect("git metadata creates");
    assert_eq!(
        LocalRootBinding::bind(
            &store,
            &git_root,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        )
        .expect_err("git root rejects"),
        RootBindingError::GitMetadata
    );

    let unconfirmed = app_data.path().join("unconfirmed");
    assert_eq!(
        LocalRootBinding::bind(
            &store,
            &unconfirmed,
            RootSelection::NotConfirmed,
            &projection,
            &blobs,
        )
        .expect_err("unconfirmed root rejects"),
        RootBindingError::GitStatusUnconfirmed
    );
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_roots_and_entries() {
    use std::os::unix::fs::symlink;

    let app_data = tempfile::tempdir().expect("app data creates");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(None);
    let target = app_data.path().join("target");
    fs::create_dir(&target).expect("target creates");
    let root_link = app_data.path().join("root-link");
    symlink(&target, &root_link).expect("root link creates");
    assert_eq!(
        LocalRootBinding::bind(
            &store,
            &root_link,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        )
        .expect_err("symlink root rejects"),
        RootBindingError::Symlink
    );
}

#[test]
fn rejects_non_nfc_and_case_fold_colliding_entries() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(None);

    let non_nfc = app_data.path().join("non-nfc");
    fs::create_dir(&non_nfc).expect("root creates");
    fs::write(non_nfc.join("e\u{301}.md"), b"text").expect("non-NFC file writes");
    assert!(matches!(
        LocalRootBinding::bind(
            &store,
            &non_nfc,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        ),
        Err(RootBindingError::UnsafePath(_))
    ));

    let collision = app_data.path().join("collision");
    fs::create_dir(&collision).expect("root creates");
    fs::create_dir(collision.join("Plans")).expect("first name creates");
    if fs::create_dir(collision.join("plans")).is_err() {
        return; // The local filesystem already refuses the case-fold collision.
    }
    assert_eq!(
        LocalRootBinding::bind(
            &store,
            &collision,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        )
        .expect_err("case collision rejects"),
        RootBindingError::CaseFoldCollision
    );
}

#[test]
fn reports_stable_external_replace_once_and_suppresses_projector_writes() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("roadmap.md", b"old\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");

    assert!(binding.poll_changes().expect("self-write scan").is_empty());
    assert!(binding
        .poll_changes()
        .expect("second self-write scan")
        .is_empty());
    fs::write(root.join("plans/roadmap.md"), b"new\n").expect("external edit writes");
    assert!(binding
        .poll_changes()
        .expect("first stable observation")
        .is_empty());
    let changes = binding.poll_changes().expect("second stable observation");
    assert_eq!(changes.len(), 1);
    assert!(matches!(
        &changes[0],
        LocalChange::ReplaceFile {
            node_id,
            base_revision_id,
            bytes,
            ..
        } if node_id == "file-node" && base_revision_id == "revision-one" && bytes == b"new\n"
    ));
    assert!(binding.poll_changes().expect("repeat scan").is_empty());
}

#[test]
fn reports_stable_external_create_and_delete_intents() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("existing.md", b"existing\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");

    fs::write(root.join("plans/new.bin"), [0_u8, 1, 2]).expect("new file writes");
    assert!(binding
        .poll_changes()
        .expect("first create observation")
        .is_empty());
    let create = binding.poll_changes().expect("second create observation");
    assert!(matches!(
        &create[0],
        LocalChange::CreateFile {
            relative_path,
            mime_type,
            bytes,
            ..
        } if relative_path == "plans/new.bin" && mime_type == "application/octet-stream" && bytes == &[0, 1, 2]
    ));

    fs::remove_file(root.join("plans/existing.md")).expect("known file removes");
    assert!(binding
        .poll_changes()
        .expect("first delete observation")
        .is_empty());
    assert_eq!(
        binding.poll_changes().expect("second delete observation"),
        vec![LocalChange::TombstoneNode {
            node_id: "file-node".to_owned(),
            relative_path: "plans/existing.md".to_owned(),
        }]
    );
}

#[test]
fn recursive_directory_deletion_tombstones_children_before_parent_and_replays() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let peer_data = tempfile::tempdir().expect("peer app data creates");
    let root = app_data.path().join("root");
    let peer_root = peer_data.path().join("peer-root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let peer_store = WorkspaceStore::open(peer_data.path(), "workspace").expect("peer store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let membership = MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    };
    let mut authority = WorkspaceFileAuthority::new("workspace");
    let plans =
        SignedFileOperation::create_directory(&identity, "workspace", None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let archive = SignedFileOperation::create_directory(
        &identity,
        "workspace",
        Some(plans.operation.node_id.clone()),
        "archive",
        vec![plans.operation.operation_id.clone()],
    )
    .expect("archive signs");
    authority
        .apply(&archive, &membership)
        .expect("archive applies");
    let bytes = b"retained on replay\n";
    let hash = authority
        .blob_store_mut()
        .store(bytes)
        .expect("blob stores");
    let file = SignedFileOperation::create_file(
        &identity,
        "workspace",
        Some(archive.operation.node_id.clone()),
        "history.md",
        hash.as_str(),
        "text/markdown",
        bytes.len() as u64,
        vec![archive.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &authority.projection(),
        authority.blob_store(),
    )
    .expect("root binds");

    fs::remove_dir_all(root.join("plans/archive")).expect("known tree removes");
    assert!(binding
        .poll_changes()
        .expect("first deletion observation")
        .is_empty());
    let changes = binding.poll_changes().expect("second deletion observation");
    assert_eq!(changes.len(), 2);
    assert!(matches!(
        &changes[0],
        LocalChange::TombstoneNode { node_id, relative_path }
            if node_id == &file.operation.node_id && relative_path == "plans/archive/history.md"
    ));
    assert!(matches!(
        &changes[1],
        LocalChange::TombstoneNode { node_id, relative_path }
            if node_id == &archive.operation.node_id && relative_path == "plans/archive"
    ));
    let causal_frontier = authority.causal_frontier();
    let deletions = binding
        .author_changes(
            &identity,
            "workspace",
            &changes,
            authority.blob_store_mut(),
            causal_frontier,
        )
        .expect("recursive deletion signs");
    for deletion in &deletions {
        authority
            .apply(deletion, &membership)
            .expect("depth-ordered deletion applies");
    }
    assert!(!projection_names(&authority.projection()).contains(&"archive".to_owned()));

    let mut replayed = WorkspaceFileAuthority::new("workspace");
    replayed
        .blob_store_mut()
        .store(bytes)
        .expect("peer blob stores");
    let mut operations = vec![plans, archive, file];
    operations.extend(deletions);
    replayed
        .replay(&operations, &membership)
        .expect("peer replays recursive deletion");
    LocalRootBinding::bind(
        &peer_store,
        &peer_root,
        RootSelection::ConfirmedNotGitManaged,
        &replayed.projection(),
        replayed.blob_store(),
    )
    .expect("peer root binds");
    assert!(peer_root.join("plans").is_dir());
    assert!(!peer_root.join("plans/archive").exists());
}

#[test]
fn configured_and_permanent_ignores_exclude_external_input() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let membership = MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    };
    let mut authority = WorkspaceFileAuthority::new("workspace");
    let plans =
        SignedFileOperation::create_directory(&identity, "workspace", None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let ignore = authority
        .author_add_ignore_rule(&identity, "scratch/**")
        .expect("ignore rule authors");
    authority
        .apply(&ignore, &membership)
        .expect("ignore rule applies");
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &authority.projection(),
        authority.blob_store(),
    )
    .expect("root binds");

    fs::create_dir(root.join("scratch")).expect("ignored directory creates");
    fs::write(root.join("scratch/private.bin"), [1, 2, 3]).expect("ignored file writes");
    fs::create_dir(root.join(".git")).expect("Git metadata creates");
    fs::write(root.join(".git/config"), b"private\n").expect("Git config writes");
    fs::write(root.join("plans/shared.md"), b"shared\n").expect("shared file writes");
    assert!(binding
        .poll_changes()
        .expect("first stable observation")
        .is_empty());
    let changes = binding.poll_changes().expect("second stable observation");
    assert_eq!(changes.len(), 1);
    assert!(matches!(
        &changes[0],
        LocalChange::CreateFile { relative_path, .. } if relative_path == "plans/shared.md"
    ));
    assert!(binding.poll_changes().expect("repeat scan").is_empty());
}

fn projection_names(projection: &FileTreeProjection) -> Vec<String> {
    fn collect(node: &TreeNode, names: &mut Vec<String>) {
        names.push(node.name().to_owned());
        if let TreeNode::Directory { children, .. } = node {
            for child in children.values() {
                collect(child, names);
            }
        }
    }

    let mut names = Vec::new();
    for node in projection.root.values() {
        collect(node, &mut names);
    }
    names
}

#[test]
fn stable_external_file_becomes_a_member_signed_authority_operation() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let membership = MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    };
    let mut authority = WorkspaceFileAuthority::new("workspace");
    let plans =
        SignedFileOperation::create_directory(&identity, "workspace", None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &authority.projection(),
        authority.blob_store(),
    )
    .expect("root binds");

    fs::write(root.join("plans/new.md"), b"# New\n").expect("external file writes");
    binding.poll_changes().expect("first observation");
    let changes = binding.poll_changes().expect("second observation");
    let operations = binding
        .author_changes(
            &identity,
            "workspace",
            &changes,
            authority.blob_store_mut(),
            vec![plans.operation.operation_id.clone()],
        )
        .expect("changes sign");
    assert_eq!(operations.len(), 1);
    assert_eq!(
        operations[0].operation.signer,
        *identity.public_identity().as_bytes()
    );
    authority
        .apply(&operations[0], &membership)
        .expect("signed file applies");

    let TreeNode::Directory { children, .. } = &authority.projection().root["plans"] else {
        panic!("plans must be a directory");
    };
    assert!(children.contains_key("new.md"));
}

#[test]
fn external_binary_replacement_remains_binary_authority_input() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let (projection, mut blobs) = plans_projection(Some(("asset.bin", &[0, 1, 2])));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");

    fs::write(root.join("plans/asset.bin"), [3_u8, 4, 5]).expect("binary edit writes");
    assert!(binding
        .poll_changes()
        .expect("first binary observation")
        .is_empty());
    let changes = binding.poll_changes().expect("second binary observation");
    let operations = binding
        .author_changes(&identity, "workspace", &changes, &mut blobs, Vec::new())
        .expect("binary replacement authors");

    assert!(matches!(
        &operations[0].operation.body,
        FileOperationBody::ReplaceFileRevision { mime_type, .. }
            if mime_type == "application/octet-stream"
    ));
}

#[test]
fn recognizes_a_one_to_one_same_hash_rename_after_two_stable_scans() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("roadmap.md", b"same\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");
    binding.poll_changes().expect("baseline scans");
    binding.poll_changes().expect("baseline stabilizes");

    fs::rename(root.join("plans/roadmap.md"), root.join("plans/renamed.md")).expect("file renames");
    assert!(binding
        .poll_changes()
        .expect("first observation")
        .is_empty());
    let changes = binding.poll_changes().expect("second observation");
    assert_eq!(
        changes,
        vec![LocalChange::MoveNode {
            node_id: "file-node".to_owned(),
            new_relative_path: "plans/renamed.md".to_owned(),
        }]
    );
}

#[test]
fn replaces_an_unavailable_binding_only_with_an_explicit_empty_root() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let first_root = app_data.path().join("first-root");
    let replacement_root = app_data.path().join("replacement-root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("roadmap.md", b"authority\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &first_root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("first root binds");
    fs::remove_dir_all(&first_root).expect("first root removes");

    binding
        .replace_root(
            &replacement_root,
            RootSelection::ConfirmedNotGitManaged,
            &projection,
            &blobs,
        )
        .expect("replacement binds");
    assert_eq!(binding.health(), RootHealth::Healthy);
    assert_eq!(
        fs::read(replacement_root.join("plans/roadmap.md")).expect("replacement bytes read"),
        b"authority\n"
    );
}

#[test]
fn conflict_artifacts_materialize_and_leave_the_root_after_resolution() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let membership = MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    };
    let mut authority = WorkspaceFileAuthority::new("workspace");
    let plans =
        SignedFileOperation::create_directory(&identity, "workspace", None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let base_hash = authority.blob_store_mut().store(&[0]).expect("base stores");
    let file = SignedFileOperation::create_file(
        &identity,
        "workspace",
        Some(plans.operation.node_id.clone()),
        "asset.bin",
        base_hash.as_str(),
        "application/octet-stream",
        1,
        vec![plans.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let left_hash = authority.blob_store_mut().store(&[1]).expect("left stores");
    let right_hash = authority
        .blob_store_mut()
        .store(&[2])
        .expect("right stores");
    let left = SignedFileOperation::replace_file_revision(
        &identity,
        "workspace",
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        left_hash.as_str(),
        "application/octet-stream",
        1,
        vec![file.operation.operation_id.clone()],
    )
    .expect("left signs");
    let right = SignedFileOperation::replace_file_revision(
        &identity,
        "workspace",
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        right_hash.as_str(),
        "application/octet-stream",
        1,
        vec![file.operation.operation_id.clone()],
    )
    .expect("right signs");
    authority.apply(&left, &membership).expect("left applies");
    authority.apply(&right, &membership).expect("right applies");
    let conflict = authority
        .projection()
        .conflicts
        .into_iter()
        .find(|conflict| !conflict.resolved)
        .expect("binary conflict exists");
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &authority.projection(),
        authority.blob_store(),
    )
    .expect("conflicted root binds");
    assert_eq!(
        fs::read_dir(root.join("plans"))
            .expect("plans reads")
            .count(),
        2
    );
    assert!(binding
        .poll_changes()
        .expect("generated files scan")
        .is_empty());
    fs::write(root.join("plans/new.md"), b"new\n").expect("unrelated file writes");
    assert!(binding
        .poll_changes()
        .expect("first unrelated observation")
        .is_empty());
    let changes = binding
        .poll_changes()
        .expect("second unrelated observation");
    assert_eq!(changes.len(), 1);
    assert!(matches!(
        &changes[0],
        LocalChange::CreateFile { relative_path, .. } if relative_path == "plans/new.md"
    ));
    fs::remove_file(root.join("plans/new.md")).expect("unrelated file removes");

    let resolution = SignedFileOperation::resolve_conflict(
        &identity,
        "workspace",
        conflict.node_id,
        conflict.record_id,
        Some(left.operation.operation_id.clone()),
        vec![
            left.operation.operation_id.clone(),
            right.operation.operation_id.clone(),
        ],
    )
    .expect("resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("resolution applies");
    binding
        .repair(&authority.projection(), authority.blob_store())
        .expect("resolved projection repairs");

    let names = fs::read_dir(root.join("plans"))
        .expect("resolved plans reads")
        .map(|entry| entry.expect("entry reads").file_name())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 1);
    assert_eq!(names[0], "asset.bin");
}

#[cfg(unix)]
#[test]
fn permission_loss_preserves_materialized_bytes_until_repair_succeeds() {
    use std::os::unix::fs::PermissionsExt;

    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (initial_projection, initial_blobs) = plans_projection(Some(("roadmap.md", b"old\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &initial_projection,
        &initial_blobs,
    )
    .expect("root binds");
    let (updated_projection, updated_blobs) = plans_projection(Some(("roadmap.md", b"new\n")));
    let plans = root.join("plans");
    fs::set_permissions(&plans, fs::Permissions::from_mode(0o555))
        .expect("plans becomes read-only");

    let result = binding.repair(&updated_projection, &updated_blobs);
    let preserved = fs::read(root.join("plans/roadmap.md")).expect("old bytes remain readable");
    fs::set_permissions(&plans, fs::Permissions::from_mode(0o755))
        .expect("plans permissions restore");

    assert_eq!(result, Err(RootBindingError::Unwritable));
    assert_eq!(binding.health(), RootHealth::Unwritable);
    assert_eq!(preserved, b"old\n");
    binding
        .repair(&updated_projection, &updated_blobs)
        .expect("projection repairs after permissions return");
    assert_eq!(
        fs::read(root.join("plans/roadmap.md")).expect("updated bytes read"),
        b"new\n"
    );
}

#[test]
fn repair_preserves_an_uningested_external_edit() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (initial_projection, initial_blobs) = plans_projection(Some(("roadmap.md", b"base\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &initial_projection,
        &initial_blobs,
    )
    .expect("root binds");
    fs::write(root.join("plans/roadmap.md"), b"local unsynced\n").expect("external edit writes");
    let (remote_projection, remote_blobs) = plans_projection(Some(("roadmap.md", b"remote\n")));

    assert!(matches!(
        binding.repair(&remote_projection, &remote_blobs),
        Err(RootBindingError::ProjectionCollision(path)) if path == "plans/roadmap.md"
    ));
    assert_eq!(binding.health(), RootHealth::Unhealthy);
    assert_eq!(
        fs::read(root.join("plans/roadmap.md")).expect("local bytes remain"),
        b"local unsynced\n"
    );
}

#[test]
fn repairs_interrupted_projection_and_reports_an_unavailable_root() {
    let app_data = tempfile::tempdir().expect("app data creates");
    let root = app_data.path().join("root");
    let store = WorkspaceStore::open(app_data.path(), "workspace").expect("store opens");
    let (projection, blobs) = plans_projection(Some(("roadmap.md", b"authority\n")));
    let mut binding = LocalRootBinding::bind(
        &store,
        &root,
        RootSelection::ConfirmedNotGitManaged,
        &projection,
        &blobs,
    )
    .expect("root binds");
    fs::write(
        root.join("plans/.resonance-write-interrupted.tmp"),
        b"partial",
    )
    .expect("partial file writes");
    fs::remove_file(root.join("plans/roadmap.md")).expect("interrupted destination removes");

    binding
        .repair(&projection, &blobs)
        .expect("projection repairs");
    assert_eq!(
        fs::read(root.join("plans/roadmap.md")).expect("repaired bytes read"),
        b"authority\n"
    );
    assert!(!root.join("plans/.resonance-write-interrupted.tmp").exists());

    fs::remove_dir_all(&root).expect("root removes");
    assert!(binding.poll_changes().is_err());
    assert_eq!(binding.health(), RootHealth::Unavailable);
}
