#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _, testutils::Ledger as _, Address, BytesN, Env, String,
};

fn setup() -> (Env, Address, ProviderRegistryClient<'static>) {
    let env = Env::default();
    let contract_id = env.register_contract(None, ProviderRegistry);
    let client = ProviderRegistryClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    (env, admin, client)
}

fn dummy_hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn register_provider_with_anchor(
    env: &Env,
    client: &ProviderRegistryClient<'_>,
    admin: &Address,
    provider: &Address,
) {
    let issuer = Address::generate(env);
    client.register_provider(
        admin,
        provider,
        &issuer,
        &dummy_hash(env, 1),
        &dummy_hash(env, 2),
        &4_100_000_000_u64,
        &dummy_hash(env, 3),
    );
}

#[test]
fn test_register_and_is_provider() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    assert!(!client.is_provider(&provider));
    register_provider_with_anchor(&env, &client, &admin, &provider);
    assert!(client.is_provider(&provider));
}

#[test]
fn test_register_provider_exposes_profile() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);

    let profile = client.get_provider_profile(&provider);
    assert_eq!(profile.credential.credential_hash, dummy_hash(&env, 1));
    assert_eq!(profile.credential.attestation_hash, dummy_hash(&env, 2));
    assert_eq!(profile.credential.revocation_reference, dummy_hash(&env, 3));
    assert!(profile.active);
}

#[test]
fn test_revoke_provider_preserves_profile_but_disables_membership() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    assert!(client.is_provider(&provider));

    client.revoke_provider(&admin, &provider);
    assert!(!client.is_provider(&provider));

    let profile = client.get_provider_profile(&provider);
    assert!(!profile.active);
    assert!(profile.credential.revoked_at.is_some());
    assert_eq!(profile.credential.revoked_by, Some(admin));
}

#[test]
fn test_add_record_by_whitelisted_provider() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.add_record(
        &provider,
        &String::from_str(&env, "REC001"),
        &String::from_str(&env, "Patient data"),
    );

    let rec = client.get_record(&String::from_str(&env, "REC001"));
    assert_eq!(rec.data, String::from_str(&env, "Patient data"));
    assert_eq!(rec.created_by, provider);
    assert_eq!(client.get_provider_record_count(&provider), 1);
}

#[test]
fn test_provider_record_count_starts_at_zero() {
    let (env, _admin, client) = setup();
    let provider = Address::generate(&env);

    assert_eq!(client.get_provider_record_count(&provider), 0);
}

#[test]
fn test_provider_record_count_persists_across_multiple_records() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);

    client.add_record(
        &provider,
        &String::from_str(&env, "REC100"),
        &String::from_str(&env, "Patient data 1"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "REC101"),
        &String::from_str(&env, "Patient data 2"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "REC102"),
        &String::from_str(&env, "Patient data 3"),
    );

    assert_eq!(client.get_provider_record_count(&provider), 3);
}

#[test]
fn test_add_record_rejected_for_non_provider() {
    let (env, _admin, client) = setup();
    let stranger = Address::generate(&env);

    let result = client.try_add_record(
        &stranger,
        &String::from_str(&env, "REC002"),
        &String::from_str(&env, "Malicious data"),
    );
    assert!(matches!(result, Err(Ok(ContractError::NotFound))));
}

#[test]
fn test_add_record_rejected_after_revocation() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.revoke_provider(&admin, &provider);

    let result = client.try_add_record(
        &provider,
        &String::from_str(&env, "REC003"),
        &String::from_str(&env, "Should fail"),
    );
    assert!(matches!(result, Err(Ok(ContractError::Unauthorized))));
}

#[test]
fn test_register_provider_non_admin_rejected() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    let provider = Address::generate(&env);
    let issuer = Address::generate(&env);

    let result = client.try_register_provider(
        &non_admin,
        &provider,
        &issuer,
        &dummy_hash(&env, 1),
        &dummy_hash(&env, 2),
        &4_100_000_000_u64,
        &dummy_hash(&env, 3),
    );
    assert!(matches!(result, Err(Ok(ContractError::Unauthorized))));
}

#[test]
fn test_revoke_provider_non_admin_rejected() {
    let (env, admin, client) = setup();
    let non_admin = Address::generate(&env);
    let provider = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);

    let result = client.try_revoke_provider(&non_admin, &provider);
    assert!(matches!(result, Err(Ok(ContractError::Unauthorized))));
}

#[test]
fn test_double_initialize() {
    let (_env, admin, client) = setup();
    let result = client.try_initialize(&admin);
    assert!(matches!(result, Err(Ok(ContractError::AlreadyInitialized))));
}

#[test]
fn test_register_provider_rejects_expired_anchor() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let issuer = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 100);
    let result = client.try_register_provider(
        &admin,
        &provider,
        &issuer,
        &dummy_hash(&env, 1),
        &dummy_hash(&env, 2),
        &100_u64,
        &dummy_hash(&env, 3),
    );
    assert!(matches!(result, Err(Ok(ContractError::CredentialExpired))));
}

#[test]
fn test_expired_provider_loses_membership() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let issuer = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 10);
    client.register_provider(
        &admin,
        &provider,
        &issuer,
        &dummy_hash(&env, 1),
        &dummy_hash(&env, 2),
        &20_u64,
        &dummy_hash(&env, 3),
    );
    assert!(client.is_provider(&provider));

    env.ledger().with_mut(|li| li.timestamp = 21);
    assert!(!client.is_provider(&provider));

    let result = client.try_add_record(
        &provider,
        &String::from_str(&env, "REC-EXP"),
        &String::from_str(&env, "expired"),
    );
    assert!(matches!(result, Err(Ok(ContractError::CredentialExpired))));
}

#[test]
fn test_rate_limit_within_limit() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.set_rate_limit(&admin, &50, &3600);

    for id in ["REC-W-0", "REC-W-1", "REC-W-2"] {
        client.add_record(
            &provider,
            &String::from_str(&env, id),
            &String::from_str(&env, "data"),
        );
    }
}

#[test]
fn test_rate_limit_at_limit() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 2_000_000);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    const MAX: u32 = 5;
    client.set_rate_limit(&admin, &MAX, &3600);

    let ids = ["REC-A-0", "REC-A-1", "REC-A-2", "REC-A-3", "REC-A-4"];
    for id in ids {
        client.add_record(
            &provider,
            &String::from_str(&env, id),
            &String::from_str(&env, "data"),
        );
    }

    let over = client.try_add_record(
        &provider,
        &String::from_str(&env, "REC-A-OVER"),
        &String::from_str(&env, "data"),
    );
    assert!(matches!(over, Err(Ok(ContractError::RateLimitExceeded))));
}

#[test]
fn test_rate_limit_over_limit_rejected() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 3_000_000);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.set_rate_limit(&admin, &2, &100);

    client.add_record(
        &provider,
        &String::from_str(&env, "R1"),
        &String::from_str(&env, "d"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "R2"),
        &String::from_str(&env, "d"),
    );

    let third = client.try_add_record(
        &provider,
        &String::from_str(&env, "R3"),
        &String::from_str(&env, "d"),
    );
    assert!(matches!(third, Err(Ok(ContractError::RateLimitExceeded))));
}

#[test]
fn test_rate_limit_window_reset_allows_again() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    let t0 = 10_000u64;
    env.ledger().with_mut(|li| li.timestamp = t0);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.set_rate_limit(&admin, &3, &3600);

    for id in ["REC-R-0", "REC-R-1", "REC-R-2"] {
        client.add_record(
            &provider,
            &String::from_str(&env, id),
            &String::from_str(&env, "data"),
        );
    }

    let blocked = client.try_add_record(
        &provider,
        &String::from_str(&env, "REC-BLOCKED"),
        &String::from_str(&env, "data"),
    );
    assert!(matches!(blocked, Err(Ok(ContractError::RateLimitExceeded))));

    env.ledger().with_mut(|li| li.timestamp = t0 + 3600);

    client.add_record(
        &provider,
        &String::from_str(&env, "REC-AFTER-RESET"),
        &String::from_str(&env, "data"),
    );
    assert_eq!(
        client
            .get_record(&String::from_str(&env, "REC-AFTER-RESET"))
            .data,
        String::from_str(&env, "data")
    );
}

#[test]
fn test_deactivate_provider_transfers_records_and_removes_whitelist() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let successor = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    register_provider_with_anchor(&env, &client, &admin, &successor);

    client.add_record(
        &provider,
        &String::from_str(&env, "R1"),
        &String::from_str(&env, "data1"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "R2"),
        &String::from_str(&env, "data2"),
    );

    client.deactivate_provider(&admin, &provider, &successor);

    assert!(!client.is_provider(&provider));
    assert_eq!(
        client.get_record(&String::from_str(&env, "R1")).created_by,
        successor
    );
    assert_eq!(
        client.get_record(&String::from_str(&env, "R2")).created_by,
        successor
    );
}

#[test]
fn test_deactivate_provider_no_records() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let successor = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    register_provider_with_anchor(&env, &client, &admin, &successor);
    client.deactivate_provider(&admin, &provider, &successor);
    assert!(!client.is_provider(&provider));
}

#[test]
fn test_deactivate_provider_non_admin_rejected() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let successor = Address::generate(&env);
    let non_admin = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    register_provider_with_anchor(&env, &client, &admin, &successor);

    let result = client.try_deactivate_provider(&non_admin, &provider, &successor);
    assert!(matches!(result, Err(Ok(ContractError::Unauthorized))));
}

#[test]
fn test_deactivate_provider_successor_accumulates_records() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let successor = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    register_provider_with_anchor(&env, &client, &admin, &successor);

    client.add_record(
        &successor,
        &String::from_str(&env, "S1"),
        &String::from_str(&env, "succ data"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "P1"),
        &String::from_str(&env, "prov data 1"),
    );
    client.add_record(
        &provider,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "prov data 2"),
    );

    client.deactivate_provider(&admin, &provider, &successor);

    assert_eq!(
        client.get_record(&String::from_str(&env, "S1")).created_by,
        successor
    );
    assert_eq!(
        client.get_record(&String::from_str(&env, "P1")).created_by,
        successor
    );
    assert_eq!(
        client.get_record(&String::from_str(&env, "P2")).created_by,
        successor
    );
}

#[test]
fn test_rate_limit_disabled_with_zero_max() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 4_000_000);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.set_rate_limit(&admin, &0, &3600);

    let ids = [
        "REC-D-0", "REC-D-1", "REC-D-2", "REC-D-3", "REC-D-4", "REC-D-5", "REC-D-6",
        "REC-D-7", "REC-D-8", "REC-D-9",
    ];
    for id in ids {
        client.add_record(
            &provider,
            &String::from_str(&env, id),
            &String::from_str(&env, "data"),
        );
    }
}

#[test]
fn test_rate_provider_success() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let patient = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.rate_provider(&patient, &provider, &5);

    let (total_ratings, average_score) = client.get_provider_reputation(&provider);
    assert_eq!(total_ratings, 1);
    assert_eq!(average_score, 500);
}

#[test]
fn test_rate_provider_prevents_double_rating() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let patient = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.rate_provider(&patient, &provider, &4);

    let second = client.try_rate_provider(&patient, &provider, &5);
    assert!(matches!(second, Err(Ok(ContractError::AlreadyRated))));
}

#[test]
fn test_rate_provider_invalid_score() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let patient = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    let result = client.try_rate_provider(&patient, &provider, &0);
    assert!(matches!(result, Err(Ok(ContractError::InvalidScore))));
}

#[test]
fn test_get_provider_reputation_average_scaled() {
    let (env, admin, client) = setup();
    let provider = Address::generate(&env);
    let patient_a = Address::generate(&env);
    let patient_b = Address::generate(&env);
    let patient_c = Address::generate(&env);

    register_provider_with_anchor(&env, &client, &admin, &provider);
    client.rate_provider(&patient_a, &provider, &5);
    client.rate_provider(&patient_b, &provider, &4);
    client.rate_provider(&patient_c, &provider, &3);

    let (total_ratings, average_score) = client.get_provider_reputation(&provider);
    assert_eq!(total_ratings, 3);
    assert_eq!(average_score, 400);
}
