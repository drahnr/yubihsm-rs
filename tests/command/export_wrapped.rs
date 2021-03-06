use yubihsm::{AsymmetricAlg, Capability, ObjectOrigin, ObjectType, WrapAlg};

use test_vectors::AESCCM_TEST_VECTORS;
use {
    clear_test_key_slot, TEST_DOMAINS, TEST_EXPORTED_KEY_ID, TEST_EXPORTED_KEY_LABEL, TEST_KEY_ID,
    TEST_KEY_LABEL,
};

/// Test wrap key workflow using randomly generated keys
// TODO: test against RFC 3610 vectors
#[test]
fn wrap_key_test() {
    let mut client = ::get_hsm_client();
    let algorithm = WrapAlg::AES128_CCM;
    let capabilities = Capability::EXPORT_WRAPPED | Capability::IMPORT_WRAPPED;
    let delegated_capabilities = Capability::all();

    clear_test_key_slot(&mut client, ObjectType::WrapKey);

    let key_id = client
        .put_wrap_key(
            TEST_KEY_ID,
            TEST_KEY_LABEL.into(),
            TEST_DOMAINS,
            capabilities,
            delegated_capabilities,
            algorithm,
            AESCCM_TEST_VECTORS[0].key,
        ).unwrap_or_else(|err| panic!("error generating wrap key: {}", err));

    assert_eq!(key_id, TEST_KEY_ID);

    // Create a key to export
    let exported_key_type = ObjectType::AsymmetricKey;
    let exported_key_capabilities =
        Capability::ASYMMETRIC_SIGN_EDDSA | Capability::EXPORT_UNDER_WRAP;
    let exported_key_algorithm = AsymmetricAlg::Ed25519;

    let _ = client.delete_object(TEST_EXPORTED_KEY_ID, exported_key_type);

    client
        .generate_asymmetric_key(
            TEST_EXPORTED_KEY_ID,
            TEST_EXPORTED_KEY_LABEL.into(),
            TEST_DOMAINS,
            exported_key_capabilities,
            exported_key_algorithm,
        ).unwrap_or_else(|err| panic!("error generating asymmetric key: {}", err));

    let wrap_data = client
        .export_wrapped(TEST_KEY_ID, exported_key_type, TEST_EXPORTED_KEY_ID)
        .unwrap_or_else(|err| panic!("error exporting key: {}", err));

    // Delete the object from the HSM prior to re-importing it
    assert!(
        client
            .delete_object(TEST_EXPORTED_KEY_ID, exported_key_type)
            .is_ok()
    );

    // Re-import the wrapped key back into the HSM
    let import_response = client
        .import_wrapped(TEST_KEY_ID, wrap_data)
        .unwrap_or_else(|err| panic!("error importing key: {}", err));

    assert_eq!(import_response.object_type, exported_key_type);
    assert_eq!(import_response.object_id, TEST_EXPORTED_KEY_ID);

    let imported_key_info = client
        .get_object_info(TEST_EXPORTED_KEY_ID, exported_key_type)
        .unwrap_or_else(|err| panic!("error getting object info: {}", err));

    assert_eq!(imported_key_info.capabilities, exported_key_capabilities);
    assert_eq!(imported_key_info.object_id, TEST_EXPORTED_KEY_ID);
    assert_eq!(imported_key_info.domains, TEST_DOMAINS);
    assert_eq!(imported_key_info.object_type, exported_key_type);
    assert_eq!(imported_key_info.algorithm, exported_key_algorithm.into());
    assert_eq!(imported_key_info.origin, ObjectOrigin::WrappedGenerated);
    assert_eq!(
        &imported_key_info.label.to_string().unwrap(),
        TEST_EXPORTED_KEY_LABEL
    );
}
