use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::MembershipProjection,
    workspace_domain::Member,
    workspace_files::{
        authority::{AuthorityError, WorkspaceFileAuthority},
        projection::TreeNode,
        SignedFileOperation,
    },
};

const WORKSPACE_ID: &str = "workspace-one";

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
fn rejects_invalid_signature_workspace_and_non_member_before_projection() {
    let member = identity();
    let outsider = identity();
    let membership = membership(&member);

    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let mut invalid_signature =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "tampered", Vec::new())
            .expect("operation signs");
    invalid_signature.signature[0] ^= 0xff;
    assert_eq!(
        authority.apply(&invalid_signature, &membership),
        Err(AuthorityError::FileOperation(
            resonance_runtime::workspace_files::FileOperationError::InvalidSignature
        ))
    );

    let wrong_workspace =
        SignedFileOperation::create_directory(&member, "workspace-two", None, "wrong", Vec::new())
            .expect("operation signs");
    assert_eq!(
        authority.apply(&wrong_workspace, &membership),
        Err(AuthorityError::WrongWorkspace)
    );

    let non_member = SignedFileOperation::create_directory(
        &outsider,
        WORKSPACE_ID,
        None,
        "outsider",
        Vec::new(),
    )
    .expect("operation signs");
    assert_eq!(
        authority.apply(&non_member, &membership),
        Err(AuthorityError::NonMember)
    );
    assert!(authority.projection().root.is_empty());
}

#[test]
fn duplicate_delivery_is_idempotent_but_id_reuse_with_other_bytes_is_rejected() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let operation =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("operation signs");

    authority
        .apply(&operation, &membership)
        .expect("first delivery applies");
    authority
        .apply(&operation, &membership)
        .expect("duplicate delivery is a no-op");
    assert_eq!(authority.applied_operation_ids().len(), 1);
}

#[test]
fn pending_child_applies_after_its_parent_arrives() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let parent =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("parent signs");
    let child = SignedFileOperation::create_directory(
        &member,
        WORKSPACE_ID,
        Some(parent.operation.node_id.clone()),
        "nested",
        vec![parent.operation.operation_id.clone()],
    )
    .expect("child signs");

    authority
        .apply(&child, &membership)
        .expect("child remains pending");
    assert!(authority.projection().root.is_empty());
    authority
        .apply(&parent, &membership)
        .expect("parent applies");

    let TreeNode::Directory { children, .. } = &authority.projection().root["plans"] else {
        panic!("plans must be a directory");
    };
    assert!(children.contains_key("nested"));
}

#[test]
fn create_move_and_replay_keep_stable_node_and_revision_ids() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let archive = SignedFileOperation::create_directory(
        &member,
        WORKSPACE_ID,
        None,
        "archive",
        vec![plans.operation.operation_id.clone()],
    )
    .expect("archive signs");
    authority
        .apply(&archive, &membership)
        .expect("archive applies");

    let content = b"# Plan\n";
    let hash = authority
        .blob_store_mut()
        .store(content)
        .expect("blob stores");
    let file = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "roadmap.md",
        hash.as_str(),
        "text/markdown",
        content.len() as u64,
        vec![archive.operation.operation_id.clone()],
    )
    .expect("file signs");
    let node_id = file.operation.node_id.clone();
    authority.apply(&file, &membership).expect("file applies");
    let move_file = SignedFileOperation::move_node(
        &member,
        WORKSPACE_ID,
        node_id.clone(),
        archive.operation.node_id.clone(),
        "roadmap.md",
        vec![file.operation.operation_id.clone()],
    )
    .expect("move signs");
    authority
        .apply(&move_file, &membership)
        .expect("move applies");

    let projection = authority.projection();
    let TreeNode::Directory { children, .. } = &projection.root["archive"] else {
        panic!("archive must be a directory");
    };
    let TreeNode::File {
        node_id: projected_node,
        current_revision_id,
        ..
    } = &children["roadmap.md"]
    else {
        panic!("roadmap must be a file");
    };
    assert_eq!(projected_node, &node_id);
    assert_eq!(current_revision_id, &file.operation.operation_id);

    let mut replayed = WorkspaceFileAuthority::new(WORKSPACE_ID);
    replayed
        .blob_store_mut()
        .store(content)
        .expect("replay blob stores");
    replayed
        .replay(&[move_file, file, archive, plans], &membership)
        .expect("unordered replay converges");
    assert_eq!(projection, replayed.projection());
}

#[test]
fn three_concurrent_disjoint_markdown_edits_merge_deterministically() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let base = b"one\ntwo\nthree\nfour\nfive\n";
    let base_hash = authority.blob_store_mut().store(base).expect("base stores");
    let file = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "plan.md",
        base_hash.as_str(),
        "text/markdown",
        base.len() as u64,
        vec![plans.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");

    let revisions = [
        b"ONE\ntwo\nthree\nfour\nfive\n".as_slice(),
        b"one\ntwo\nTHREE\nfour\nfive\n".as_slice(),
        b"one\ntwo\nthree\nfour\nFIVE\n".as_slice(),
    ]
    .into_iter()
    .map(|bytes| {
        let hash = authority
            .blob_store_mut()
            .store(bytes)
            .expect("revision stores");
        SignedFileOperation::replace_file_revision(
            &member,
            WORKSPACE_ID,
            file.operation.node_id.clone(),
            file.operation.operation_id.clone(),
            hash.as_str(),
            "text/markdown",
            bytes.len() as u64,
            vec![file.operation.operation_id.clone()],
        )
        .expect("revision signs")
    })
    .collect::<Vec<_>>();
    for revision in revisions.iter().rev() {
        authority
            .apply(revision, &membership)
            .expect("revision applies");
    }

    let projection = authority.projection();
    assert!(projection.conflicts.is_empty());
    let TreeNode::Directory { children, .. } = &projection.root["plans"] else {
        panic!("plans must be a directory");
    };
    let TreeNode::File {
        current_revision_id,
        ..
    } = &children["plan.md"]
    else {
        panic!("plan must be a file");
    };
    let hash = &projection.revisions[current_revision_id].content_hash;
    let merged = authority
        .blob_store()
        .open(&resonance_runtime::workspace_files::blobs::ContentHash(
            hash.clone(),
        ))
        .expect("merged blob opens");
    assert_eq!(merged, b"ONE\ntwo\nTHREE\nfour\nFIVE\n");
}

#[test]
fn binary_conflict_resolution_retains_immutable_history() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
    let base_hash = authority.blob_store_mut().store(&[0]).expect("base stores");
    let file = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
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
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        left_hash.as_str(),
        "application/octet-stream",
        1,
        vec![file.operation.operation_id.clone()],
    )
    .expect("left signs");
    let right = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        right_hash.as_str(),
        "application/octet-stream",
        1,
        vec![file.operation.operation_id.clone()],
    )
    .expect("right signs");
    authority.apply(&right, &membership).expect("right applies");
    authority.apply(&left, &membership).expect("left applies");

    let conflict = authority.projection().conflicts[0].clone();
    assert_eq!(
        conflict.kind,
        resonance_runtime::workspace_files::projection::ConflictKind::BinaryCollision
    );
    let resolution = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        conflict.record_id.clone(),
        Some(right.operation.operation_id.clone()),
        vec![
            left.operation.operation_id.clone(),
            right.operation.operation_id.clone(),
        ],
    )
    .expect("resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("resolution applies");
    let projection = authority.projection();
    assert!(projection.conflicts[0].resolved);
    assert!(projection
        .revisions
        .contains_key(&left.operation.operation_id));
    assert!(projection
        .revisions
        .contains_key(&right.operation.operation_id));
}

#[test]
fn invalid_utf8_and_concurrent_create_become_visible_conflicts() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");

    let first_hash = authority
        .blob_store_mut()
        .store(b"first")
        .expect("first stores");
    let second_hash = authority
        .blob_store_mut()
        .store(b"second")
        .expect("second stores");
    let first = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "same.bin",
        first_hash.as_str(),
        "application/octet-stream",
        5,
        vec![plans.operation.operation_id.clone()],
    )
    .expect("first create signs");
    let second = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "same.bin",
        second_hash.as_str(),
        "application/octet-stream",
        6,
        vec![plans.operation.operation_id.clone()],
    )
    .expect("second create signs");
    authority.apply(&first, &membership).expect("first applies");
    authority
        .apply(&second, &membership)
        .expect("second is preserved");
    let projection = authority.projection();
    assert!(projection.conflicts.iter().any(|conflict| {
        conflict.kind
            == resonance_runtime::workspace_files::projection::ConflictKind::ConcurrentCreate
    }));
    let TreeNode::Directory { children, .. } = &projection.root["plans"] else {
        panic!("plans must be a directory");
    };
    assert_eq!(children.len(), 2);
    assert!(children
        .keys()
        .any(|name| name.contains(".resonance-conflict-")));

    let base_hash = authority
        .blob_store_mut()
        .store(b"base\n")
        .expect("base stores");
    let markdown = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "utf8.md",
        base_hash.as_str(),
        "text/markdown",
        5,
        vec![second.operation.operation_id.clone()],
    )
    .expect("markdown signs");
    authority
        .apply(&markdown, &membership)
        .expect("markdown applies");
    let valid_hash = authority
        .blob_store_mut()
        .store(b"valid\n")
        .expect("valid stores");
    let invalid_hash = authority
        .blob_store_mut()
        .store(&[0xff, 0xfe])
        .expect("invalid stores");
    let valid = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        markdown.operation.node_id.clone(),
        markdown.operation.operation_id.clone(),
        valid_hash.as_str(),
        "text/markdown",
        6,
        vec![markdown.operation.operation_id.clone()],
    )
    .expect("valid edit signs");
    let invalid = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        markdown.operation.node_id.clone(),
        markdown.operation.operation_id.clone(),
        invalid_hash.as_str(),
        "text/markdown",
        2,
        vec![markdown.operation.operation_id.clone()],
    )
    .expect("invalid edit signs");
    authority.apply(&valid, &membership).expect("valid applies");
    authority
        .apply(&invalid, &membership)
        .expect("invalid is preserved");
    assert!(authority.projection().conflicts.iter().any(|conflict| {
        conflict.node_id == markdown.operation.node_id
            && conflict.kind
                == resonance_runtime::workspace_files::projection::ConflictKind::MarkdownOverlap
    }));
}

#[test]
fn delete_edit_and_competing_moves_preserve_conflict_records() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    let archive = SignedFileOperation::create_directory(
        &member,
        WORKSPACE_ID,
        None,
        "archive",
        vec![plans.operation.operation_id.clone()],
    )
    .expect("archive signs");
    let backlog = SignedFileOperation::create_directory(
        &member,
        WORKSPACE_ID,
        None,
        "backlog",
        vec![archive.operation.operation_id.clone()],
    )
    .expect("backlog signs");
    for operation in [&plans, &archive, &backlog] {
        authority
            .apply(operation, &membership)
            .expect("directory applies");
    }
    let base = b"base\n";
    let base_hash = authority.blob_store_mut().store(base).expect("base stores");
    let file = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        Some(plans.operation.node_id.clone()),
        "plan.md",
        base_hash.as_str(),
        "text/markdown",
        base.len() as u64,
        vec![backlog.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let edit_bytes = b"edited\n";
    let edit_hash = authority
        .blob_store_mut()
        .store(edit_bytes)
        .expect("edit stores");
    let edit = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        edit_hash.as_str(),
        "text/markdown",
        edit_bytes.len() as u64,
        vec![file.operation.operation_id.clone()],
    )
    .expect("edit signs");
    let delete = SignedFileOperation::tombstone_node(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        vec![file.operation.operation_id.clone()],
    )
    .expect("delete signs");
    authority
        .apply(&delete, &membership)
        .expect("delete applies");
    authority.apply(&edit, &membership).expect("edit applies");

    let move_archive = SignedFileOperation::move_node(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        archive.operation.node_id.clone(),
        "plan.md",
        vec![
            edit.operation.operation_id.clone(),
            delete.operation.operation_id.clone(),
        ],
    )
    .expect("first move signs");
    let move_backlog = SignedFileOperation::move_node(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        backlog.operation.node_id.clone(),
        "plan.md",
        vec![
            edit.operation.operation_id.clone(),
            delete.operation.operation_id.clone(),
        ],
    )
    .expect("second move signs");
    authority
        .apply(&move_archive, &membership)
        .expect("first move applies");
    authority
        .apply(&move_backlog, &membership)
        .expect("second move applies");

    let projection = authority.projection();
    assert!(projection.conflicts.iter().any(|conflict| {
        conflict.kind == resonance_runtime::workspace_files::projection::ConflictKind::DeleteEdit
    }));
    assert!(projection.conflicts.iter().any(|conflict| {
        conflict.kind == resonance_runtime::workspace_files::projection::ConflictKind::CompetingMove
    }));
}

#[test]
fn rejects_nonportable_and_case_fold_colliding_names() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    for name in [".", "..", "CON", ".resonance-conflict-x", "e\u{301}"] {
        let operation =
            SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, name, Vec::new())
                .expect("operation signs");
        assert!(
            authority.apply(&operation, &membership).is_err(),
            "accepted {name:?}"
        );
    }

    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "Plans", Vec::new())
            .expect("Plans signs");
    authority.apply(&plans, &membership).expect("Plans applies");
    let collision = SignedFileOperation::create_directory(
        &member,
        WORKSPACE_ID,
        None,
        "plans",
        vec![plans.operation.operation_id.clone()],
    )
    .expect("plans signs");
    assert_eq!(
        authority.apply(&collision, &membership),
        Err(AuthorityError::AlreadyExists)
    );
}
