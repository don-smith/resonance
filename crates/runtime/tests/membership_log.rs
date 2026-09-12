use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    membership_log::{
        MembershipLog, MembershipOperationBody, MembershipOperationId, MembershipStatus,
        SignedMembershipOperation, MEMBERSHIP_PROTOCOL_VERSION,
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
    assert!(projection.contains(&creator.public_identity()));
    assert!(projection.contains(&contributor.public_identity()));
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
        projection.statuses[&operation_id(&tampered_id)],
        MembershipStatus::Rejected
    );
    assert_eq!(
        projection.statuses[&operation_id(&pending_id)],
        MembershipStatus::Pending
    );
    assert_eq!(
        projection.statuses[&operation_id(&unknown_id)],
        MembershipStatus::Rejected
    );
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
        .contains_text(&member_id(&loser.operation.body)));

    let winner_id = winner.operation_id().expect("winner ID");
    let loser_id = loser.operation_id().expect("loser ID");
    log.insert(winner.clone()).expect("winner records late");
    let projection = log.projection(&workspace_id);

    assert!(projection.contains_text(&member_id(&winner.operation.body)));
    assert!(!projection.contains_text(&member_id(&loser.operation.body)));
    assert_eq!(
        projection.statuses[&operation_id(&winner_id)],
        MembershipStatus::Canonical
    );
    assert_eq!(
        projection.statuses[&operation_id(&loser_id)],
        MembershipStatus::Rejected
    );
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
        projection.statuses[&operation_id(&wrong_version_id)],
        MembershipStatus::Rejected
    );
    assert_eq!(
        projection.statuses[&operation_id(&wrong_workspace_id)],
        MembershipStatus::Rejected
    );
}

#[test]
fn legacy_addition_and_new_removal_fixtures_are_exact_and_stable() {
    let creator =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![41; 32]))
            .expect("creator loads");
    let member =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![42; 32]))
            .expect("member loads");
    let workspace_id = "ab".repeat(32);
    let genesis = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    let addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace_id,
        genesis.operation_id().expect("genesis ID"),
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("addition signs");
    let request = resonance_runtime::membership_log::SignedSelfRemovalRequestV1::create_with_nonce(
        &member,
        &workspace_id,
        addition.operation_id().expect("addition ID"),
        *creator.public_identity().as_bytes(),
        [43; 32],
        3,
    )
    .expect("request signs");
    let removal = SignedMembershipOperation::remove_member_by_request(
        &creator,
        &workspace_id,
        addition.operation_id().expect("addition ID"),
        2,
        request.clone(),
        4,
    )
    .expect("removal signs");
    for (name, bytes, id) in [
        (
            "genesis",
            genesis.encode().expect("genesis encodes"),
            genesis.operation_id().expect("genesis ID"),
        ),
        (
            "add-member",
            addition.encode().expect("addition encodes"),
            addition.operation_id().expect("addition ID"),
        ),
        (
            "self-removal-request",
            request.encode().expect("request encodes"),
            request.request_id().expect("request ID").to_string(),
        ),
        (
            "remove-member",
            removal.encode().expect("removal encodes"),
            removal.operation_id().expect("removal ID"),
        ),
    ] {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/membership/v1");
        assert_eq!(
            std::fs::read(root.join(format!("{name}.bin"))).expect("fixture bytes"),
            bytes
        );
        assert_eq!(
            std::fs::read_to_string(root.join(format!("{name}.id"))).expect("fixture ID"),
            format!("{id}\n")
        );
    }
}

#[test]
fn creator_is_immutable_and_roles_never_grant_removal_authority() {
    let creator = identity();
    let member = identity();
    let outsider = identity();
    let workspace = "6".repeat(64);
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let addition = SignedMembershipOperation::add_member_with_role(
        &creator,
        &workspace,
        &genesis_id,
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        "workspace-admin",
        2,
    )
    .expect("addition signs");
    let addition_id = addition.operation_id().expect("addition ID");
    let outsider_addition = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &addition_id,
        2,
        *outsider.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("second addition signs");
    let outsider_interval = outsider_addition.operation_id().expect("outsider interval");
    let creator_expulsion = SignedMembershipOperation::expel_member(
        &member,
        &workspace,
        &outsider_interval,
        1,
        *creator.public_identity().as_bytes(),
        &genesis_id,
        3,
    )
    .expect("attempt signs");
    let non_creator_expulsion = SignedMembershipOperation::expel_member(
        &member,
        &workspace,
        &outsider_interval,
        2,
        *outsider.public_identity().as_bytes(),
        &outsider_interval,
        3,
    )
    .expect("attempt signs");
    let mut log = MembershipLog::new();
    for operation in [
        genesis,
        addition,
        outsider_addition,
        creator_expulsion,
        non_creator_expulsion,
    ] {
        log.insert(operation).expect("operation inserts");
    }
    let projection = log.projection(&workspace);
    assert_eq!(projection.creator, Some(creator.public_identity()));
    assert!(projection.contains(&creator.public_identity()));
    assert!(projection.contains(&member.public_identity()));
    assert!(projection.contains(&outsider.public_identity()));
}

#[test]
fn requested_departure_is_idempotent_and_old_requests_cannot_remove_a_fresh_interval() {
    let creator = identity();
    let member = identity();
    let other = identity();
    let workspace = "7".repeat(64);
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let first_add = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &genesis_id,
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("member adds");
    let first_interval = first_add.operation_id().expect("interval ID");
    let other_add = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &first_interval,
        2,
        *other.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("other adds");
    let request = resonance_runtime::membership_log::SignedSelfRemovalRequestV1::create_with_nonce(
        &member,
        &workspace,
        &first_interval,
        *creator.public_identity().as_bytes(),
        [9; 32],
        4,
    )
    .expect("request signs");
    let removal = SignedMembershipOperation::remove_member_by_request(
        &creator,
        &workspace,
        other_add.operation_id().expect("head"),
        3,
        request.clone(),
        5,
    )
    .expect("removal signs");
    let removal_id = removal.operation_id().expect("removal ID");
    let readdition = SignedMembershipOperation::add_member(
        &other,
        &workspace,
        &removal_id,
        1,
        *member.public_identity().as_bytes(),
        "Lin returned",
        6,
    )
    .expect("any current member re-adds after requested departure");
    let new_interval = readdition.operation_id().expect("new interval");
    let stale_removal = SignedMembershipOperation::remove_member_by_request(
        &creator,
        &workspace,
        &new_interval,
        4,
        request,
        7,
    )
    .expect("stale removal signs but must not project");
    let mut log = MembershipLog::new();
    for operation in [
        genesis,
        first_add,
        other_add,
        removal.clone(),
        removal,
        readdition,
        stale_removal,
    ] {
        log.insert(operation)
            .expect("operation inserts idempotently");
    }
    let projection = log.projection(&workspace);
    assert!(projection.contains(&member.public_identity()));
    assert_eq!(
        projection
            .interval_id(&member.public_identity())
            .expect("fresh interval")
            .as_str(),
        new_interval
    );
}

#[test]
fn still_current_request_reprepares_against_a_replacement_head() {
    let creator = identity();
    let requester = identity();
    let first_branch_member = identity();
    let second_branch_member = identity();
    let workspace = "9".repeat(64);
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let requester_add = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &genesis_id,
        1,
        *requester.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("requester adds");
    let requester_interval = requester_add.operation_id().expect("requester interval");
    let first = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &requester_interval,
        2,
        *first_branch_member.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("first branch");
    let second = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &requester_interval,
        3,
        *second_branch_member.public_identity().as_bytes(),
        "Jo",
        3,
    )
    .expect("second branch");
    let (winner, loser) =
        if first.operation_id().expect("first ID") < second.operation_id().expect("second ID") {
            (first, second)
        } else {
            (second, first)
        };
    let request = resonance_runtime::membership_log::SignedSelfRemovalRequestV1::create_with_nonce(
        &requester,
        &workspace,
        &requester_interval,
        *creator.public_identity().as_bytes(),
        [10; 32],
        4,
    )
    .expect("request");
    let mut log = MembershipLog::new();
    for operation in [genesis, requester_add, loser] {
        log.insert(operation).expect("insert");
    }
    let losing_head = log
        .projection(&workspace)
        .canonical_head
        .expect("losing head");
    log.insert(winner).expect("winner arrives late");
    let replacement = log.projection(&workspace);
    assert_ne!(replacement.canonical_head.as_ref(), Some(&losing_head));
    assert_eq!(
        replacement
            .interval_id(&requester.public_identity())
            .expect("interval remains")
            .as_str(),
        requester_interval
    );
    let removal = SignedMembershipOperation::remove_member_by_request(
        &creator,
        &workspace,
        replacement
            .canonical_head
            .expect("replacement head")
            .to_string(),
        4,
        request,
        5,
    )
    .expect("request reprepares");
    let prepared = log
        .prepare(&workspace, removal)
        .expect("replacement-head removal validates");
    assert_eq!(prepared.removals.len(), 1);
    assert_eq!(
        prepared.removals[0].member.public_identity,
        requester.public_identity()
    );
}

#[test]
fn creator_expulsion_requires_creator_re_admission() {
    let creator = identity();
    let member = identity();
    let other = identity();
    let workspace = "8".repeat(64);
    let genesis =
        SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1).expect("genesis");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let member_add = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &genesis_id,
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        2,
    )
    .expect("member add");
    let member_interval = member_add.operation_id().expect("member interval");
    let other_add = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &member_interval,
        2,
        *other.public_identity().as_bytes(),
        "Mia",
        3,
    )
    .expect("other add");
    let expulsion = SignedMembershipOperation::expel_member(
        &creator,
        &workspace,
        other_add.operation_id().expect("head"),
        3,
        *member.public_identity().as_bytes(),
        &member_interval,
        4,
    )
    .expect("expulsion");
    let expulsion_id = expulsion.operation_id().expect("expulsion ID");
    let unauthorized = SignedMembershipOperation::add_member(
        &other,
        &workspace,
        &expulsion_id,
        1,
        *member.public_identity().as_bytes(),
        "Lin",
        5,
    )
    .expect("unauthorized re-add signs");
    let authorized = SignedMembershipOperation::add_member(
        &creator,
        &workspace,
        &expulsion_id,
        4,
        *member.public_identity().as_bytes(),
        "Lin",
        5,
    )
    .expect("creator re-add signs");
    let mut first = MembershipLog::new();
    for operation in [
        genesis.clone(),
        member_add.clone(),
        other_add.clone(),
        expulsion.clone(),
        unauthorized,
    ] {
        first.insert(operation).expect("insert");
    }
    assert!(!first
        .projection(&workspace)
        .contains(&member.public_identity()));
    let mut second = MembershipLog::new();
    for operation in [genesis, member_add, other_add, expulsion, authorized] {
        second.insert(operation).expect("insert");
    }
    assert!(second
        .projection(&workspace)
        .contains(&member.public_identity()));
}

fn operation_id(value: &str) -> MembershipOperationId {
    MembershipOperationId::parse(value).expect("operation ID is valid")
}

fn member_id(body: &MembershipOperationBody) -> String {
    match body {
        MembershipOperationBody::AddMember {
            public_identity, ..
        }
        | MembershipOperationBody::RemoveMember {
            public_identity, ..
        } => resonance_runtime::identity::PublicIdentity::from_bytes(*public_identity).to_string(),
    }
}
