use yubihsm::{Capability, ObjectType, OpaqueAlg};

use {clear_test_key_slot, TEST_DOMAINS, TEST_KEY_ID, TEST_KEY_LABEL, TEST_MESSAGE};

/// Put an opaque object and read it back
#[test]
fn opaque_object_test() {
    let mut client = ::get_hsm_client();

    clear_test_key_slot(&mut client, ObjectType::Opaque);

    let object_id = client
        .put_opaque(
            TEST_KEY_ID,
            TEST_KEY_LABEL.into(),
            TEST_DOMAINS,
            Capability::default(),
            OpaqueAlg::DATA,
            TEST_MESSAGE,
        ).unwrap_or_else(|err| panic!("error putting opaque object: {}", err));

    assert_eq!(object_id, TEST_KEY_ID);

    let opaque_data = client
        .get_opaque(TEST_KEY_ID)
        .unwrap_or_else(|err| panic!("error getting opaque object: {}", err));

    assert_eq!(opaque_data, TEST_MESSAGE);
}
