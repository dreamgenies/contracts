#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, String};

/// ------------------------------------------------
/// PATIENT TESTS
/// ------------------------------------------------

#[test]
fn test_register_and_get_patient() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let patient_wallet = Address::generate(&env);
    let name = String::from_str(&env, "John Doe");
    let dob = 631152000;
    let metadata = String::from_str(&env, "ipfs://some-medical-history");

    env.mock_all_auths();

    client.register_patient(&patient_wallet, &name, &dob, &metadata);

    let patient_data = client.get_patient(&patient_wallet);
    assert_eq!(patient_data.name, name);
    assert_eq!(patient_data.dob, dob);
    assert_eq!(patient_data.metadata, metadata);
}

#[test]
fn test_update_patient() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let patient_wallet = Address::generate(&env);
    let name = String::from_str(&env, "John Doe");
    let dob = 631152000;
    let initial_metadata = String::from_str(&env, "ipfs://initial");

    env.mock_all_auths();

    client.register_patient(&patient_wallet, &name, &dob, &initial_metadata);

    let new_metadata = String::from_str(&env, "ipfs://updated-history");
    client.update_patient(&patient_wallet, &patient_wallet, &new_metadata);

    let patient_data = client.get_patient(&patient_wallet);
    assert_eq!(patient_data.metadata, new_metadata);
}

#[test]
fn test_is_patient_registered() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let patient_wallet = Address::generate(&env);
    let unregistered_wallet = Address::generate(&env);

    env.mock_all_auths();

    assert_eq!(client.is_patient_registered(&patient_wallet), false);
    assert_eq!(client.is_patient_registered(&unregistered_wallet), false);

    client.register_patient(
        &patient_wallet,
        &String::from_str(&env, "Jane Doe"),
        &631152000,
        &String::from_str(&env, "ipfs://data"),
    );

    assert_eq!(client.is_patient_registered(&patient_wallet), true);
    assert_eq!(client.is_patient_registered(&unregistered_wallet), false);
}

/// ------------------------------------------------
/// DOCTOR + INSTITUTION TESTS
/// ------------------------------------------------

#[test]
fn test_register_and_get_doctor() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let doctor_wallet = Address::generate(&env);
    let name = String::from_str(&env, "Dr. Alice");
    let specialization = String::from_str(&env, "Cardiology");
    let cert_hash = Bytes::from_array(&env, &[1, 2, 3, 4]);

    env.mock_all_auths();

    client.register_doctor(&doctor_wallet, &name, &specialization, &cert_hash);

    let doctor = client.get_doctor(&doctor_wallet);
    assert_eq!(doctor.name, name);
    assert_eq!(doctor.specialization, specialization);
    assert_eq!(doctor.certificate_hash, cert_hash);
    assert_eq!(doctor.verified, false);
}

#[test]
fn test_register_institution_and_verify_doctor() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let doctor_wallet = Address::generate(&env);
    let institution_wallet = Address::generate(&env);

    let name = String::from_str(&env, "Dr. Bob");
    let specialization = String::from_str(&env, "Neurology");
    let cert_hash = Bytes::from_array(&env, &[9, 9, 9]);

    env.mock_all_auths();

    // Register doctor
    client.register_doctor(&doctor_wallet, &name, &specialization, &cert_hash);

    // Register institution
    client.register_institution(&institution_wallet);

    // Verify doctor
    client.verify_doctor(&doctor_wallet, &institution_wallet);

    let doctor = client.get_doctor(&doctor_wallet);
    assert_eq!(doctor.verified, true);
}

#[test]
#[should_panic(expected = "Unauthorized institution")]
fn test_verify_doctor_by_unregistered_institution_should_fail() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let doctor_wallet = Address::generate(&env);
    let fake_institution = Address::generate(&env);

    let name = String::from_str(&env, "Dr. Eve");
    let specialization = String::from_str(&env, "Oncology");
    let cert_hash = Bytes::from_array(&env, &[7, 7, 7]);

    env.mock_all_auths();

    client.register_doctor(&doctor_wallet, &name, &specialization, &cert_hash);

    // This should panic
    client.verify_doctor(&doctor_wallet, &fake_institution);
}

#[test]
fn test_grant_access_and_add_medical_record() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);

    let hash = Bytes::from_array(&env, &[1, 2, 3]);
    let desc = String::from_str(&env, "Blood test results");
    let v1 = BytesN::from_array(&env, &[1u8; 32]);

    env.mock_all_auths();

    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);
    client.grant_access(&patient, &patient, &doctor);
    client.add_medical_record(&patient, &doctor, &hash, &desc);

    let records = client.get_medical_records(&patient);
    assert_eq!(records.len(), 1);

    let record = records.get(0).unwrap();
    assert_eq!(record.record_hash, hash);
    assert_eq!(record.description, desc);
}

#[test]
#[should_panic(expected = "Patient has not acknowledged current consent version")]
fn test_unauthorized_doctor_cannot_add_record() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);

    let hash = Bytes::from_array(&env, &[9, 9, 9]);
    let desc = String::from_str(&env, "X-ray scan");

    env.mock_all_auths();

    client.add_medical_record(&patient, &doctor, &hash, &desc);
}

#[test]
fn test_revoke_access() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);

    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);

    env.mock_all_auths();

    client.grant_access(&patient, &patient, &doctor);
    client.revoke_access(&patient, &patient, &doctor);

    let doctors = client.get_authorized_doctors(&patient);
    assert_eq!(doctors.len(), 0);
}

/// ------------------------------------------------
/// CONSENT TESTS
/// ------------------------------------------------

fn make_version(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn test_consent_status_never_signed() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let patient = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&Address::generate(&env));

    assert_eq!(client.get_consent_status(&patient), ConsentStatus::NeverSigned);
}

#[test]
fn test_consent_status_never_signed_no_ack() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&make_version(&env, 1));

    // Version published but patient never acknowledged
    assert_eq!(client.get_consent_status(&patient), ConsentStatus::NeverSigned);
}

#[test]
fn test_consent_status_acknowledged() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let v1 = make_version(&env, 1);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);

    assert_eq!(client.get_consent_status(&patient), ConsentStatus::Acknowledged);
}

#[test]
fn test_consent_status_pending_after_new_version() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let v1 = make_version(&env, 1);
    let v2 = make_version(&env, 2);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);

    // Admin publishes new version — patient is now Pending
    client.publish_consent_version(&v2);
    assert_eq!(client.get_consent_status(&patient), ConsentStatus::Pending);
}

#[test]
fn test_consent_re_acknowledge_restores_acknowledged() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let v1 = make_version(&env, 1);
    let v2 = make_version(&env, 2);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);
    client.publish_consent_version(&v2);
    client.acknowledge_consent(&patient, &patient, &v2);

    assert_eq!(client.get_consent_status(&patient), ConsentStatus::Acknowledged);
}

#[test]
#[should_panic(expected = "Version mismatch")]
fn test_acknowledge_wrong_version_panics() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&make_version(&env, 1));
    client.acknowledge_consent(&patient, &patient, &make_version(&env, 99));
}

#[test]
#[should_panic(expected = "Patient has not acknowledged current consent version")]
fn test_add_record_blocked_without_consent() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&make_version(&env, 1));
    // Patient never acknowledges
    client.grant_access(&patient, &patient, &doctor);
    client.add_medical_record(
        &patient,
        &doctor,
        &Bytes::from_array(&env, &[1, 2, 3]),
        &String::from_str(&env, "test"),
    );
}

#[test]
fn test_add_record_allowed_after_consent() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);
    let v1 = make_version(&env, 1);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);
    client.grant_access(&patient, &patient, &doctor);
    client.add_medical_record(
        &patient,
        &doctor,
        &Bytes::from_array(&env, &[1, 2, 3]),
        &String::from_str(&env, "Blood test"),
    );

    assert_eq!(client.get_medical_records(&patient).len(), 1);
}

#[test]
#[should_panic(expected = "Patient has not acknowledged current consent version")]
fn test_add_record_blocked_after_new_version() {
    let env = Env::default();
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);
    let v1 = make_version(&env, 1);
    let v2 = make_version(&env, 2);

    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&v1);
    client.acknowledge_consent(&patient, &patient, &v1);
    client.grant_access(&patient, &patient, &doctor);

    // Admin bumps version — patient must re-acknowledge
    client.publish_consent_version(&v2);
    client.add_medical_record(
        &patient,
        &doctor,
        &Bytes::from_array(&env, &[1, 2, 3]),
        &String::from_str(&env, "Post-update record"),
    );
}

/// ------------------------------------------------
/// GUARDIAN TESTS
/// ------------------------------------------------

fn setup_with_consent(env: &Env) -> (MedicalRegistryClient, Address) {
    let contract_id = env.register(MedicalRegistry, ());
    let client = MedicalRegistryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.initialize(&admin);
    client.publish_consent_version(&make_version(env, 1));
    (client, admin)
}

#[test]
fn test_assign_and_get_guardian() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    assert_eq!(client.get_guardian(&patient), Some(guardian));
}

#[test]
fn test_revoke_guardian() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    client.revoke_guardian(&patient);
    assert_eq!(client.get_guardian(&patient), None);
}

#[test]
fn test_guardian_can_acknowledge_consent() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    client.acknowledge_consent(&patient, &guardian, &v1);

    assert_eq!(client.get_consent_status(&patient), ConsentStatus::Acknowledged);
}

#[test]
fn test_guardian_can_grant_and_revoke_access() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);
    let doctor = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    client.acknowledge_consent(&patient, &guardian, &v1);
    client.grant_access(&patient, &guardian, &doctor);

    assert_eq!(client.get_authorized_doctors(&patient).len(), 1);

    client.revoke_access(&patient, &guardian, &doctor);
    assert_eq!(client.get_authorized_doctors(&patient).len(), 0);
}

#[test]
fn test_guardian_can_update_patient() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.register_patient(
        &patient,
        &String::from_str(&env, "Minor Patient"),
        &631152000,
        &String::from_str(&env, "ipfs://original"),
    );
    client.assign_guardian(&patient, &guardian);
    client.update_patient(&patient, &guardian, &String::from_str(&env, "ipfs://updated"));

    assert_eq!(
        client.get_patient(&patient).metadata,
        String::from_str(&env, "ipfs://updated")
    );
}

#[test]
fn test_guardian_enables_record_write() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);
    let doctor = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    client.acknowledge_consent(&patient, &guardian, &v1);
    client.grant_access(&patient, &guardian, &doctor);
    client.add_medical_record(
        &patient,
        &doctor,
        &Bytes::from_array(&env, &[5, 6, 7]),
        &String::from_str(&env, "Guardian-approved record"),
    );

    assert_eq!(client.get_medical_records(&patient).len(), 1);
}

#[test]
#[should_panic(expected = "Caller is not patient or assigned guardian")]
fn test_unauthorized_caller_rejected() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.acknowledge_consent(&patient, &stranger, &v1);
}

#[test]
#[should_panic(expected = "Caller is not patient or assigned guardian")]
fn test_revoked_guardian_rejected() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient = Address::generate(&env);
    let guardian = Address::generate(&env);

    client.assign_guardian(&patient, &guardian);
    client.revoke_guardian(&patient);
    // Guardian no longer valid
    client.acknowledge_consent(&patient, &guardian, &v1);
}

#[test]
#[should_panic(expected = "Caller is not patient or assigned guardian")]
fn test_guardian_cannot_act_for_different_patient() {
    let env = Env::default();
    let (client, _admin) = setup_with_consent(&env);
    let v1 = make_version(&env, 1);
    let patient_a = Address::generate(&env);
    let patient_b = Address::generate(&env);
    let guardian = Address::generate(&env);

    // Guardian assigned only to patient_a
    client.assign_guardian(&patient_a, &guardian);
    // Attempt to act on behalf of patient_b
    client.acknowledge_consent(&patient_b, &guardian, &v1);
}
