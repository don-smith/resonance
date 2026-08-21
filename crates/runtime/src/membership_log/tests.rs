use super::*;
use crate::identity::{InMemoryKeyCustody, InstallationIdentity};

fn identity() -> InstallationIdentity {
    InstallationIdentity::load_or_create(&InMemoryKeyCustody::default()).expect("identity creates")
}

#[test]
fn preserves_an_unknown_role_without_interpreting_it_as_a_privileged_role() {
    let creator = identity();
    let unknown_role_member = identity();
    let workspace_id = "1".repeat(64);
    let genesis = SignedMembershipOperation::genesis(&creator, &workspace_id, "Ada", 1)
        .expect("genesis signs");
    let genesis_id = genesis.operation_id().expect("genesis ID");
    let operation = MembershipOperation {
        version: MEMBERSHIP_PROTOCOL_VERSION,
        workspace_id: workspace_id.clone(),
        parent_operation_id: Some(genesis_id),
        author: *creator.public_identity().as_bytes(),
        author_counter: 1,
        body: MembershipOperationBody::AddMember {
            public_identity: *unknown_role_member.public_identity().as_bytes(),
            display_name: "Lin".to_owned(),
            role: "operator".to_owned(),
            added_at: 2,
        },
    };
    let unknown_role =
        SignedMembershipOperation::sign(&creator, operation).expect("operation signs");

    let mut log = MembershipLog::new();
    log.insert(genesis).expect("genesis records");
    log.insert(unknown_role).expect("operation records");
    let projection = log.projection(&workspace_id);

    assert_eq!(projection.members.len(), 2);
    assert_eq!(
        projection
            .members
            .iter()
            .find(|member| member.public_identity
                == unknown_role_member.public_identity().to_string())
            .expect("unknown-role member projects")
            .role,
        "operator"
    );
}
