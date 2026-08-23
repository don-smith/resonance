use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::MembershipProjection,
    workspace_domain::Member,
    workspace_files::{authority::WorkspaceFileAuthority, SignedFileOperation},
};

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("identity creates")
}

fn membership(identity: &InstallationIdentity) -> MembershipProjection {
    MembershipProjection {
        canonical_head: Some("head".to_owned()),
        members: vec![Member::new(
            identity.public_identity().to_string(),
            "Ada",
            "developer",
            "Ada",
            0,
        )],
        statuses: Default::default(),
    }
}

#[test]
fn creates_root_directory_and_restores_projection_after_replay() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let op = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("op creates");
    authority.apply(&op, &membership).expect("op applies");

    let projection = authority.projection();
    assert!(projection.root.contains_key("plans"));

    let mut replayed = WorkspaceFileAuthority::new();
    replayed
        .replay(&[op], &membership)
        .expect("replay succeeds");
    let replayed_projection = replayed.projection();
    assert!(replayed_projection.root.contains_key("plans"));
}

#[test]
fn rejects_duplicate_root_directory_names() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let op1 = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("op1");
    authority.apply(&op1, &membership).expect("first op");

    let op2 = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("op2");
    assert!(authority.apply(&op2, &membership).is_err());
}

#[test]
fn rejects_invalid_operation_signature() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let mut op = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("op creates");
    if !op.signature.is_empty() {
        op.signature[0] ^= 0xff;
    }
    assert!(authority.apply(&op, &membership).is_err());
}

#[test]
fn idempotent_accepts_duplicate_operation() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let op = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("op creates");
    authority.apply(&op, &membership).expect("first");
    authority
        .apply(&op, &membership)
        .expect("second idempotent");
    assert_eq!(authority.applied_operation_ids().len(), 1);
}

#[test]
fn rejects_non_member() {
    let id = identity();
    let outsider =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("outsider");
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let op = SignedFileOperation::create_directory(&outsider, "ws-1", "plans").expect("op");
    assert!(authority.apply(&op, &membership).is_err());
}

#[test]
fn rejects_invalid_portable_paths() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let op = SignedFileOperation::create_directory(&id, "ws-1", ".").expect("op");
    assert!(authority.apply(&op, &membership).is_err());
}

#[test]
fn creates_multiple_root_directories_in_projection() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let dir1 = SignedFileOperation::create_directory(&id, "ws-1", "plans").expect("dir1");
    authority.apply(&dir1, &membership).expect("dir1 applies");

    let dir2 = SignedFileOperation::create_directory(&id, "ws-1", "docs").expect("dir2");
    authority.apply(&dir2, &membership).expect("dir2 applies");

    let projection = authority.projection();
    assert!(projection.root.contains_key("plans"));
    assert!(projection.root.contains_key("docs"));
}

#[test]
fn full_authority_replay_restores_deterministic_projection() {
    let id = identity();
    let membership = membership(&id);
    let mut authority = WorkspaceFileAuthority::new();

    let ops: Vec<SignedFileOperation> = (0..5)
        .map(|i| {
            SignedFileOperation::create_directory(&id, "ws-1", format!("dir-{i}")).expect("op")
        })
        .collect();

    for op in &ops {
        authority.apply(op, &membership).expect("applies");
    }

    let projection = authority.projection();
    assert_eq!(projection.root.len(), 5);

    let mut replayed = WorkspaceFileAuthority::new();
    replayed.replay(&ops, &membership).expect("replay");
    let replayed_projection = replayed.projection();
    assert_eq!(replayed_projection.root.len(), 5);
    for i in 0..5 {
        assert!(replayed_projection.root.contains_key(&format!("dir-{i}")));
    }
}
