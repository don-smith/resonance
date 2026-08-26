use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::MembershipProjection,
    workspace_domain::Member,
    workspace_files::{
        authority::{AuthorityError, WorkspaceFileAuthority},
        projection::TreeNode,
        FileOperationBody, SignedFileOperation,
    },
    workspace_store::WorkspaceStore,
};

const WORKSPACE_ID: &str = "workspace-one";

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("identity creates")
}

fn tree_names(node: &TreeNode, names: &mut Vec<String>) {
    names.push(node.name().to_owned());
    if let TreeNode::Directory { children, .. } = node {
        for child in children.values() {
            tree_names(child, names);
        }
    }
}

fn projection_names(authority: &WorkspaceFileAuthority) -> Vec<String> {
    let projection = authority.projection();
    let mut names = Vec::new();
    for node in projection.root.values() {
        tree_names(node, &mut names);
    }
    names
}

fn membership(identity: &InstallationIdentity) -> MembershipProjection {
    MembershipProjection {
        canonical_head: Some("head".to_owned()),
        canonical_head_id: None,
        members: vec![Member::new(
            identity.public_identity(),
            "Ada",
            "developer",
            identity.public_identity(),
            0,
        )],
        statuses: Default::default(),
        statuses_by_id: Default::default(),
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
    assert!(projection_names(&authority)
        .iter()
        .any(|name| name.contains(".resonance-conflict-") && name.ends_with(".bin")));
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
    assert!(!projection_names(&authority)
        .iter()
        .any(|name| name.contains(".resonance-conflict-")));
}

#[test]
fn delete_edit_resolution_preserves_legacy_keep_and_can_choose_deletion() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let base = b"base\n";
    let base_hash = authority.blob_store_mut().store(base).expect("base stores");
    let file = SignedFileOperation::create_file(
        &member,
        WORKSPACE_ID,
        None,
        "plan.md",
        base_hash.as_str(),
        "text/markdown",
        base.len() as u64,
        Vec::new(),
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let edited = b"edited\n";
    let edited_hash = authority
        .blob_store_mut()
        .store(edited)
        .expect("edited bytes store");
    let edit = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        edited_hash.as_str(),
        "text/markdown",
        edited.len() as u64,
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
    authority.apply(&edit, &membership).expect("edit applies");
    authority
        .apply(&delete, &membership)
        .expect("deletion intent is preserved");
    let conflict = authority
        .projection()
        .conflicts
        .into_iter()
        .find(|conflict| {
            conflict.kind
                == resonance_runtime::workspace_files::projection::ConflictKind::DeleteEdit
        })
        .expect("delete-edit conflict exists");
    let legacy_keep = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        conflict.record_id,
        None,
        vec![
            edit.operation.operation_id.clone(),
            delete.operation.operation_id.clone(),
        ],
    )
    .expect("legacy keep resolution signs");
    authority
        .apply(&legacy_keep, &membership)
        .expect("legacy keep resolution applies");
    assert!(authority.projection().root.contains_key("plan.md"));

    let second_edited = b"edited again\n";
    let second_hash = authority
        .blob_store_mut()
        .store(second_edited)
        .expect("second edit stores");
    let second_edit = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        edit.operation.operation_id.clone(),
        second_hash.as_str(),
        "text/markdown",
        second_edited.len() as u64,
        vec![legacy_keep.operation.operation_id.clone()],
    )
    .expect("second edit signs");
    let second_delete = SignedFileOperation::tombstone_node(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        vec![legacy_keep.operation.operation_id.clone()],
    )
    .expect("second delete signs");
    authority
        .apply(&second_edit, &membership)
        .expect("second edit applies");
    authority
        .apply(&second_delete, &membership)
        .expect("second deletion intent is preserved");
    let unresolved = authority
        .projection()
        .conflicts
        .into_iter()
        .filter(|candidate| !candidate.resolved)
        .collect::<Vec<_>>();
    assert_eq!(unresolved.len(), 1);
    let resolution = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        unresolved[0].record_id.clone(),
        Some(second_delete.operation.operation_id.clone()),
        vec![
            second_edit.operation.operation_id.clone(),
            second_delete.operation.operation_id.clone(),
        ],
    )
    .expect("explicit deletion resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("explicit deletion resolution applies");

    let projection = authority.projection();
    assert!(!projection.root.contains_key("plan.md"));
    assert!(projection
        .conflicts
        .iter()
        .all(|conflict| conflict.resolved));
    assert!(projection
        .revisions
        .contains_key(&file.operation.operation_id));
    assert!(projection
        .revisions
        .contains_key(&second_edit.operation.operation_id));
    assert!(authority
        .applied_operation_ids()
        .contains(&second_delete.operation.operation_id));
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
    let create_conflict = projection
        .conflicts
        .iter()
        .find(|conflict| {
            conflict.kind
                == resonance_runtime::workspace_files::projection::ConflictKind::ConcurrentCreate
        })
        .expect("create conflict exists")
        .clone();
    let resolve_create = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        create_conflict.node_id,
        create_conflict.record_id,
        Some(first.operation.operation_id.clone()),
        vec![
            first.operation.operation_id.clone(),
            second.operation.operation_id.clone(),
        ],
    )
    .expect("create resolution signs");
    authority
        .apply(&resolve_create, &membership)
        .expect("create resolution applies");
    let resolved_projection = authority.projection();
    let TreeNode::Directory { children, .. } = &resolved_projection.root["plans"] else {
        panic!("plans must be a directory");
    };
    assert_eq!(children.len(), 1);
    assert!(!children
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
    let names = projection_names(&authority);
    assert!(names.iter().any(|name| name.ends_with(".deleted")));
    assert!(names.iter().any(|name| name.ends_with(".move")));

    let move_conflict = projection
        .conflicts
        .into_iter()
        .find(|conflict| {
            conflict.kind
                == resonance_runtime::workspace_files::projection::ConflictKind::CompetingMove
        })
        .expect("competing move conflict exists");
    let resolution = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        move_conflict.record_id.clone(),
        Some(move_backlog.operation.operation_id.clone()),
        vec![
            move_archive.operation.operation_id.clone(),
            move_backlog.operation.operation_id.clone(),
        ],
    )
    .expect("move resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("move resolution applies");

    let projection = authority.projection();
    let TreeNode::Directory {
        children: backlog_children,
        ..
    } = projection.root.get("backlog").expect("backlog remains")
    else {
        panic!("backlog remains a directory");
    };
    assert_eq!(
        backlog_children.get("plan.md").map(TreeNode::node_id),
        Some(file.operation.node_id.as_str())
    );
    assert!(projection
        .conflicts
        .iter()
        .find(|conflict| conflict.record_id == move_conflict.record_id)
        .is_some_and(|conflict| conflict.resolved));
    assert!(!projection_names(&authority)
        .iter()
        .any(|name| name.ends_with(".move")));
}

#[test]
fn durable_replay_retains_resolved_delete_edit_history() {
    let application_data = tempfile::tempdir().expect("application data creates");
    let store = WorkspaceStore::open(application_data.path(), WORKSPACE_ID).expect("store opens");
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");
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
        vec![plans.operation.operation_id.clone()],
    )
    .expect("file signs");
    authority.apply(&file, &membership).expect("file applies");
    let edited = b"edited\n";
    let edited_hash = authority
        .blob_store_mut()
        .store(edited)
        .expect("edited bytes store");
    let edit = SignedFileOperation::replace_file_revision(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        file.operation.operation_id.clone(),
        edited_hash.as_str(),
        "text/markdown",
        edited.len() as u64,
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
    let conflict = authority
        .projection()
        .conflicts
        .into_iter()
        .find(|conflict| {
            conflict.kind
                == resonance_runtime::workspace_files::projection::ConflictKind::DeleteEdit
        })
        .expect("delete-edit conflict exists");
    let resolution = SignedFileOperation::resolve_conflict(
        &member,
        WORKSPACE_ID,
        file.operation.node_id.clone(),
        conflict.record_id.clone(),
        Some(edit.operation.operation_id.clone()),
        vec![
            delete.operation.operation_id.clone(),
            edit.operation.operation_id.clone(),
        ],
    )
    .expect("resolution signs");
    authority
        .apply(&resolution, &membership)
        .expect("resolution applies");
    store
        .record_file_operations(&[
            plans,
            file.clone(),
            delete.clone(),
            edit.clone(),
            resolution.clone(),
        ])
        .expect("history stores atomically");

    let mut replayed = WorkspaceFileAuthority::new(WORKSPACE_ID);
    replayed
        .blob_store_mut()
        .store(base)
        .expect("base restores");
    replayed
        .blob_store_mut()
        .store(edited)
        .expect("edited bytes restore");
    replayed
        .replay(
            &store.file_operations().expect("history reloads"),
            &membership,
        )
        .expect("durable history replays");

    let projection = replayed.projection();
    let replayed_conflict = projection
        .conflicts
        .iter()
        .find(|candidate| candidate.record_id == conflict.record_id)
        .expect("conflict record remains");
    assert!(replayed_conflict.resolved);
    assert!(projection
        .revisions
        .contains_key(&file.operation.operation_id));
    assert!(projection
        .revisions
        .contains_key(&edit.operation.operation_id));
    assert!(replayed
        .applied_operation_ids()
        .contains(&delete.operation.operation_id));
    assert!(replayed
        .applied_operation_ids()
        .contains(&resolution.operation.operation_id));
}

#[test]
fn replicated_ignore_rules_replay_and_cannot_hide_live_nodes() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    let plans =
        SignedFileOperation::create_directory(&member, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("plans signs");
    authority.apply(&plans, &membership).expect("plans applies");

    let add = authority
        .author_add_ignore_rule(&member, "scratch/**")
        .expect("anchored ignore rule authors");
    assert!(matches!(
        add.operation.body,
        FileOperationBody::AddIgnoreRule { ref pattern } if pattern == "scratch/**"
    ));
    authority
        .apply(&add, &membership)
        .expect("ignore rule applies");
    assert_eq!(authority.projection().ignore_set.rules().len(), 1);
    assert!(authority
        .projection()
        .ignore_set
        .matches_configured("scratch/cache/data.bin"));

    let mut replayed = WorkspaceFileAuthority::new(WORKSPACE_ID);
    replayed
        .replay(&[add.clone(), plans.clone()], &membership)
        .expect("replicated rule replays out of order");
    assert_eq!(
        replayed.projection().ignore_set,
        authority.projection().ignore_set
    );

    assert_eq!(
        authority.author_add_ignore_rule(&member, "plans/**"),
        Err(AuthorityError::IgnoreRuleMatchesLiveNode)
    );
    let bypassed = SignedFileOperation::add_ignore_rule(
        &member,
        WORKSPACE_ID,
        "plans/**",
        authority.causal_frontier(),
    )
    .expect("direct rule signs");
    authority
        .apply(&bypassed, &membership)
        .expect("direct rule remains harmless");
    assert!(!authority
        .projection()
        .ignore_set
        .matches_configured("plans/anything"));

    let remove = authority
        .author_remove_ignore_rule(&member, &add.operation.operation_id)
        .expect("rule removal authors");
    authority
        .apply(&remove, &membership)
        .expect("rule removal applies");
    assert!(authority.projection().ignore_set.rules().is_empty());
}

#[test]
fn rejects_nonportable_and_case_fold_colliding_names() {
    let member = identity();
    let membership = membership(&member);
    let mut authority = WorkspaceFileAuthority::new(WORKSPACE_ID);
    for name in [
        ".",
        "..",
        "CON",
        ".git",
        "file.resonance-conflict-x.md",
        "e\u{301}",
    ] {
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
