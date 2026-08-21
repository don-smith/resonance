use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{
        MembershipLog, MembershipOperationBody, MembershipStatus, SignedMembershipOperation,
        MEMBERSHIP_PROTOCOL_VERSION,
    },
};

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("identity creates")
}

#[test]
fn projects_the_valid_genesis_and_contributor_addition() {
    let creator = identity();
    let contributor = identity();
    let workspace_id = "a".repeat(64);
    let genesis = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace_id,
        genesis_id,
        1,
        *contributor.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("addition signs");

    let mut log = MembershipLog::new();
    log.insert(genesis).expect("genesis records");
    log.insert(addition).expect("addition records");
    let projection = log.projection(&workspace_id);

    assert_eq!(projection.members.len(), 2);
    assert!(projection.contains(&creator.public_identity().to_string()));
    assert!(projection.contains(&contributor.public_identity().to_string()));
    assert!(projection
        .statuses
        .values()
        .all(|status| status == &MembershipStatus::Canonical));
}

#[test]
fn fails_closed_for_invalid_operations_and_keeps_a_missing_parent_pending() {
    let creator = identity();
    let stranger = identity();
    let workspace_id = "b".repeat(64);
    let genesis = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let mut tampered = genesis.clone();
    tampered.signature[0] ^= 1;
    let unavailable_parent = SignedMembershipOperation::add_member(
        &creator,
        &workspace_id,
        "c".repeat(64),
        1,
        *stranger.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("operation signs");
    let unknown_signer = SignedMembershipOperation::add_member(
        &stranger,
        &workspace_id,
        genesis_id,
        1,
        *identity().public_identity().as_bytes(),
        "Mia",
        2,
    )
    .expect("operation signs");

    let mut log = MembershipLog::new();
    log.insert(genesis).expect("genesis records");
    let tampered_id = log.insert(tampered).expect("syntactic operation records");
    let pending_id = log
        .insert(unavailable_parent)
        .expect("pending operation records");
    let unknown_id = log
        .insert(unknown_signer)
        .expect("rejected operation records");
    let projection = log.projection(&workspace_id);

    assert_eq!(projection.members.len(), 1);
    assert_eq!(
        projection.statuses[&tampered_id],
        MembershipStatus::Rejected
    );
    assert_eq!(projection.statuses[&pending_id], MembershipStatus::Pending);
    assert_eq!(projection.statuses[&unknown_id], MembershipStatus::Rejected);
}

#[test]
fn deterministic_replay_replaces_a_losing_branch_when_the_winner_arrives_late() {
    let creator = identity();
    let first = identity();
    let second = identity();
    let workspace_id = "d".repeat(64);
    let genesis = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let addition_one = SignedMembershipOperation::add_member(
        &creator,
        &workspace_id,
        &genesis_id,
        1,
        *first.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("first child signs");
    let addition_two = SignedMembershipOperation::add_member(
        &creator,
        &workspace_id,
        &genesis_id,
        2,
        *second.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("second child signs");
    let (winner, loser) = if addition_one.operation_id().expect("first ID")
        < addition_two.operation_id().expect("second ID")
    {
        (addition_one, addition_two)
    } else {
        (addition_two, addition_one)
    };

    let mut log = MembershipLog::new();
    log.insert(genesis).expect("genesis records");
    log.insert(loser.clone()).expect("loser records first");
    assert!(log
        .projection(&workspace_id)
        .contains(&member_id(&loser.operation.body)));

    let winner_id = winner.operation_id().expect("winner ID");
    let loser_id = loser.operation_id().expect("loser ID");
    log.insert(winner.clone()).expect("winner records late");
    let projection = log.projection(&workspace_id);

    assert!(projection.contains(&member_id(&winner.operation.body)));
    assert!(!projection.contains(&member_id(&loser.operation.body)));
    assert_eq!(projection.statuses[&winner_id], MembershipStatus::Canonical);
    assert_eq!(projection.statuses[&loser_id], MembershipStatus::Rejected);
}

#[test]
fn rejects_wrong_workspace_and_version_without_granting_membership() {
    let creator = identity();
    let workspace_id = "e".repeat(64);
    let mut wrong_version = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    wrong_version.operation.version = MEMBERSHIP_PROTOCOL_VERSION + 1;
    let wrong_version_id = wrong_version.operation_id().expect("operation ID");
    let mut wrong_workspace =
        SignedMembershipOperation::genesis(&creator, "f".repeat(64), "Ada", 1)
            .expect("genesis signs");
    wrong_workspace.operation.workspace_id = "0".repeat(64);
    let wrong_workspace_id = wrong_workspace.operation_id().expect("operation ID");

    let mut log = MembershipLog::new();
    log.insert(wrong_version).expect("operation records");
    log.insert(wrong_workspace).expect("operation records");
    let projection = log.projection(&workspace_id);

    assert!(projection.members.is_empty());
    assert_eq!(
        projection.statuses[&wrong_version_id],
        MembershipStatus::Rejected
    );
    assert_eq!(
        projection.statuses[&wrong_workspace_id],
        MembershipStatus::Rejected
    );
}

fn member_id(body: &MembershipOperationBody) -> String {
    let MembershipOperationBody::AddMember {
        public_identity, ..
    } = body;
    resonance_runtime::identity::PublicIdentity::from_bytes(*public_identity).to_string()
}
