use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{MembershipLog, MembershipProjection, SignedMembershipOperation},
    workspace_file_transport::{
        FileRecoveryError, FileRecoveryService, FileRecoveryTarget, FileRequest, FileResponse,
        MAX_BLOB_CHUNK_BYTES,
    },
    workspace_files::{blobs::ContentHash, SignedFileOperation},
    workspace_store::WorkspaceStore,
};

const WORKSPACE_ID: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("identity creates")
}

fn membership(owner: &InstallationIdentity, peer: &InstallationIdentity) -> MembershipProjection {
    let genesis =
        SignedMembershipOperation::genesis(owner, WORKSPACE_ID, "Owner", 1).expect("genesis signs");
    let mut log = MembershipLog::new();
    let head = log.insert(genesis).expect("genesis inserts");
    log.insert(
        SignedMembershipOperation::add_member(
            owner,
            WORKSPACE_ID,
            head,
            1,
            *peer.public_identity().as_bytes(),
            "Peer",
            2,
        )
        .expect("addition signs"),
    )
    .expect("addition inserts");
    log.projection(WORKSPACE_ID)
}

#[test]
fn two_peers_recover_reordered_offline_operations_idempotently_and_after_restart() {
    let owner = identity();
    let peer = identity();
    let membership = membership(&owner, &peer);
    let create = SignedFileOperation::create_file(
        &owner,
        WORKSPACE_ID,
        None,
        "notes.md",
        ContentHash::from_bytes(b"one").as_str(),
        "text/markdown",
        3,
        Vec::new(),
    )
    .expect("create signs");
    let replace = SignedFileOperation::replace_file_revision(
        &owner,
        WORKSPACE_ID,
        create.operation.node_id.clone(),
        create.operation.operation_id.clone(),
        ContentHash::from_bytes(b"two").as_str(),
        "text/markdown",
        3,
        vec![create.operation.operation_id.clone()],
    )
    .expect("replace signs");

    let directory = tempfile::tempdir().expect("workspace directory creates");
    let store = WorkspaceStore::open(directory.path(), WORKSPACE_ID).expect("store opens");
    let mut target = FileRecoveryTarget::new(WORKSPACE_ID, membership.clone());
    assert_eq!(
        target
            .recover_operations_to_store(
                FileResponse::Operations(vec![replace.encode().expect("replace encodes")]),
                &store,
            )
            .expect("unknown-base operation remains pending"),
        1
    );
    assert!(target.authority().projection().root.is_empty());
    assert_eq!(
        target
            .recover_operations_to_store(
                FileResponse::Operations(vec![create.encode().expect("create encodes")]),
                &store,
            )
            .expect("base recovery converges"),
        1
    );
    assert!(target
        .authority()
        .projection()
        .root
        .contains_key("notes.md"));
    assert_eq!(
        target
            .recover_operations(FileResponse::Operations(vec![create
                .encode()
                .expect("create encodes"),]))
            .expect("duplicate is idempotent"),
        0
    );

    let durable = store
        .file_operations()
        .expect("durable operations load")
        .into_iter()
        .map(|operation| operation.encode().expect("stored operation encodes"))
        .collect();
    let mut restarted = FileRecoveryTarget::new(WORKSPACE_ID, membership);
    restarted
        .replay_durable(durable)
        .expect("durable history replays after restart");
    assert_eq!(
        restarted.authority().applied_operation_ids(),
        target.authority().applied_operation_ids()
    );
}

#[test]
fn failed_recovery_storage_leaves_the_target_unchanged_for_retry() {
    let owner = identity();
    let peer = identity();
    let membership = membership(&owner, &peer);
    let operation =
        SignedFileOperation::create_directory(&owner, WORKSPACE_ID, None, "plans", Vec::new())
            .expect("operation signs");
    let encoded = operation.encode().expect("operation encodes");
    let directory = tempfile::tempdir().expect("workspace directory creates");
    let store = WorkspaceStore::open(directory.path(), WORKSPACE_ID).expect("store opens");
    let database = directory
        .path()
        .join(".resonance/workspaces")
        .join(WORKSPACE_ID)
        .join("workspace.sqlite3");
    let failure = rusqlite::Connection::open(database).expect("failure connection opens");
    failure
        .execute_batch(
            "CREATE TRIGGER fail_file_operation_insert
             BEFORE INSERT ON workspace_file_operations
             BEGIN
               SELECT RAISE(FAIL, 'simulated recovery storage failure');
             END;",
        )
        .expect("failure trigger installs");
    let mut target = FileRecoveryTarget::new(WORKSPACE_ID, membership);

    assert!(matches!(
        target
            .recover_operations_to_store(FileResponse::Operations(vec![encoded.clone()]), &store,),
        Err(FileRecoveryError::Store(_))
    ));
    assert!(target.authority().projection().root.is_empty());
    assert!(target.durable_operations().is_empty());

    failure
        .execute_batch("DROP TRIGGER fail_file_operation_insert;")
        .expect("failure trigger removes");
    assert_eq!(
        target
            .recover_operations_to_store(FileResponse::Operations(vec![encoded]), &store)
            .expect("operation retries"),
        1
    );
    assert!(target.authority().projection().root.contains_key("plans"));
}

#[test]
fn service_authorizes_current_members_and_rejects_wrong_workspace_or_hash() {
    let mut service = FileRecoveryService::new(WORKSPACE_ID);
    service.set_members(["member".to_owned()]);
    service.insert_operation("a", vec![1]);

    assert_eq!(
        service.handle(
            "member",
            FileRequest::MissingOperations {
                workspace_id: WORKSPACE_ID.to_owned(),
                known_operation_ids: Vec::new(),
            }
        ),
        FileResponse::Operations(vec![vec![1]])
    );
    assert_eq!(
        service.handle(
            "member",
            FileRequest::MissingOperations {
                workspace_id: "wrong".to_owned(),
                known_operation_ids: Vec::new(),
            }
        ),
        FileResponse::Invalid
    );
    assert_eq!(
        service.handle(
            "member",
            FileRequest::BlobChunk {
                workspace_id: WORKSPACE_ID.to_owned(),
                content_hash: "not-a-hash".to_owned(),
                offset: 0,
                max_bytes: 1,
            }
        ),
        FileResponse::Invalid
    );
    assert_eq!(
        service.handle(
            "member",
            FileRequest::BlobChunk {
                workspace_id: WORKSPACE_ID.to_owned(),
                content_hash: "f".repeat(64),
                offset: 0,
                max_bytes: 1,
            }
        ),
        FileResponse::Missing
    );

    service.set_members(Vec::new());
    assert_eq!(
        service.handle(
            "member",
            FileRequest::MissingOperations {
                workspace_id: WORKSPACE_ID.to_owned(),
                known_operation_ids: Vec::new(),
            }
        ),
        FileResponse::Denied
    );
    assert_eq!(
        service.handle(
            "outsider",
            FileRequest::MissingOperations {
                workspace_id: WORKSPACE_ID.to_owned(),
                known_operation_ids: Vec::new(),
            }
        ),
        FileResponse::Denied
    );
}

#[test]
fn resumes_bounded_blob_transfer_and_promotes_only_verified_bytes() {
    let owner = identity();
    let peer = identity();
    let membership = membership(&owner, &peer);
    let mut service = FileRecoveryService::new(WORKSPACE_ID);
    service.set_members([peer.public_identity().to_string()]);
    let bytes = (0..=255)
        .cycle()
        .take(MAX_BLOB_CHUNK_BYTES + 17)
        .collect::<Vec<_>>();
    let hash = service.insert_blob(bytes.clone());
    let mut target = FileRecoveryTarget::new(WORKSPACE_ID, membership);

    let first = service.handle(
        &peer.public_identity().to_string(),
        target.next_blob_request(&hash),
    );
    assert!(!target
        .recover_blob_chunk(first)
        .expect("first chunk remains partial"));
    let resumed = target.next_blob_request(&hash);
    assert!(matches!(
        resumed,
        FileRequest::BlobChunk { offset, .. } if offset == MAX_BLOB_CHUNK_BYTES as u64
    ));
    let second = service.handle(&peer.public_identity().to_string(), resumed);
    assert!(target
        .recover_blob_chunk(second)
        .expect("completed hash is promoted"));
    assert_eq!(
        target
            .authority()
            .blob_store()
            .open(&ContentHash(hash.clone()))
            .expect("verified blob opens"),
        bytes
    );

    let corrupt_hash = ContentHash::from_bytes(b"expected");
    let result = target.recover_blob_chunk(FileResponse::BlobChunk {
        content_hash: corrupt_hash.as_str().to_owned(),
        offset: 0,
        bytes: b"corrupt".to_vec(),
        complete: true,
    });
    assert!(matches!(result, Err(FileRecoveryError::Blob(_))));
    assert!(!target.authority().blob_store().contains(&corrupt_hash));
}

#[test]
fn concurrent_markdown_operation_waits_for_its_blob_then_merges() {
    let owner = identity();
    let peer = identity();
    let membership = membership(&owner, &peer);
    let base = b"one\ntwo\nthree\n";
    let left = b"ONE\ntwo\nthree\n";
    let right = b"one\ntwo\nTHREE\n";
    let create = SignedFileOperation::create_file(
        &owner,
        WORKSPACE_ID,
        None,
        "notes.md",
        ContentHash::from_bytes(base).as_str(),
        "text/markdown",
        base.len() as u64,
        Vec::new(),
    )
    .expect("create signs");
    let left_operation = SignedFileOperation::replace_file_revision(
        &owner,
        WORKSPACE_ID,
        create.operation.node_id.clone(),
        create.operation.operation_id.clone(),
        ContentHash::from_bytes(left).as_str(),
        "text/markdown",
        left.len() as u64,
        vec![create.operation.operation_id.clone()],
    )
    .expect("left edit signs");
    let right_operation = SignedFileOperation::replace_file_revision(
        &peer,
        WORKSPACE_ID,
        create.operation.node_id.clone(),
        create.operation.operation_id.clone(),
        ContentHash::from_bytes(right).as_str(),
        "text/markdown",
        right.len() as u64,
        vec![create.operation.operation_id.clone()],
    )
    .expect("right edit signs");
    let mut target = FileRecoveryTarget::new(WORKSPACE_ID, membership);
    target
        .recover_operations(FileResponse::Operations(vec![
            create.encode().expect("create encodes"),
            left_operation.encode().expect("left encodes"),
        ]))
        .expect("local history recovers");
    for bytes in [base.as_slice(), left.as_slice()] {
        target
            .recover_blob_chunk(FileResponse::BlobChunk {
                content_hash: ContentHash::from_bytes(bytes).as_str().to_owned(),
                offset: 0,
                bytes: bytes.to_vec(),
                complete: true,
            })
            .expect("known blob promotes");
    }

    assert_eq!(
        target
            .recover_operations(FileResponse::Operations(vec![right_operation
                .encode()
                .expect("right encodes"),]))
            .expect("operation remains pending without its blob"),
        1
    );
    target
        .recover_blob_chunk(FileResponse::BlobChunk {
            content_hash: ContentHash::from_bytes(right).as_str().to_owned(),
            offset: 0,
            bytes: right.to_vec(),
            complete: true,
        })
        .expect("missing blob promotes and rebuilds authority");

    let projection = target.authority().projection();
    let resonance_runtime::workspace_files::projection::TreeNode::File {
        current_revision_id,
        ..
    } = &projection.root["notes.md"]
    else {
        panic!("notes must remain a file");
    };
    let merged_hash = &projection.revisions[current_revision_id].content_hash;
    assert_eq!(
        target
            .authority()
            .blob_store()
            .open(&ContentHash(merged_hash.clone()))
            .expect("merged bytes open"),
        b"ONE\ntwo\nTHREE\n"
    );
}

#[test]
fn malformed_or_unauthorized_recovered_operations_have_no_durable_side_effect() {
    let owner = identity();
    let peer = identity();
    let outsider = identity();
    let membership = membership(&owner, &peer);
    let unauthorized =
        SignedFileOperation::create_directory(&outsider, WORKSPACE_ID, None, "private", Vec::new())
            .expect("operation signs");
    let invalid_hash = SignedFileOperation::create_file(
        &owner,
        WORKSPACE_ID,
        None,
        "invalid.bin",
        "not-a-hash",
        "application/octet-stream",
        1,
        Vec::new(),
    )
    .expect("invalid hash operation signs");
    let directory = tempfile::tempdir().expect("workspace directory creates");
    let store = WorkspaceStore::open(directory.path(), WORKSPACE_ID).expect("store opens");
    let mut target = FileRecoveryTarget::new(WORKSPACE_ID, membership);

    assert!(target
        .recover_operations_to_store(
            FileResponse::Operations(vec![unauthorized.encode().expect("operation encodes"),]),
            &store,
        )
        .is_err());
    assert!(target.durable_operations().is_empty());
    assert!(store.file_operations().expect("store reads").is_empty());
    assert!(target
        .recover_operations_to_store(
            FileResponse::Operations(vec![invalid_hash.encode().expect("operation encodes")]),
            &store,
        )
        .is_err());
    assert!(target.durable_operations().is_empty());
    assert!(store.file_operations().expect("store reads").is_empty());
    assert!(target
        .recover_operations_to_store(FileResponse::Operations(vec![vec![0xff, 0x00]]), &store,)
        .is_err());
    assert!(target.durable_operations().is_empty());
    assert!(store.file_operations().expect("store reads").is_empty());
}
