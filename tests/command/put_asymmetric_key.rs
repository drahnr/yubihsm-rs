use yubihsm::{AsymmetricAlg, Capability, ObjectOrigin, ObjectType};

use test_vectors::ED25519_TEST_VECTORS;
use {put_asymmetric_key, TEST_DOMAINS, TEST_KEY_ID, TEST_KEY_LABEL};

/// Put an Ed25519 key
#[test]
fn ed25519_key_test() {
    let mut client = ::get_hsm_client();
    let algorithm = AsymmetricAlg::Ed25519;
    let capabilities = Capability::ASYMMETRIC_SIGN_EDDSA;
    let example_private_key = ED25519_TEST_VECTORS[0].sk;

    put_asymmetric_key(&mut client, algorithm, capabilities, example_private_key);

    let object_info = client
        .get_object_info(TEST_KEY_ID, ObjectType::AsymmetricKey)
        .unwrap_or_else(|err| panic!("error getting object info: {}", err));

    assert_eq!(object_info.capabilities, capabilities);
    assert_eq!(object_info.object_id, TEST_KEY_ID);
    assert_eq!(object_info.domains, TEST_DOMAINS);
    assert_eq!(object_info.object_type, ObjectType::AsymmetricKey);
    assert_eq!(object_info.algorithm, algorithm.into());
    assert_eq!(object_info.origin, ObjectOrigin::Imported);
    assert_eq!(&object_info.label.to_string().unwrap(), TEST_KEY_LABEL);
}
