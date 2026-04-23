#![cfg(test)]
#![allow(deprecated)]

use super::*;
use soroban_sdk::{testutils::Address as _, BytesN, Env, String, Vec};

fn build_services(env: &Env, amount: i128) -> Vec<ServiceLine> {
    let mut services = Vec::new(env);
    services.push_back(ServiceLine {
        procedure_code: String::from_str(env, "99213"),
        modifier: None,
        quantity: 1,
        charge_amount: amount,
        diagnosis_pointers: Vec::new(env),
    });
    services
}

#[test]
fn test_full_claim_lifecycle_reconciles_cleanly() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);
    let insurance_admin = Address::generate(&env);

    let claim_id = client.submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 15000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[0; 32]),
        &15000,
    );

    let mut approved_lines = Vec::new(&env);
    approved_lines.push_back(1);
    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &approved_lines,
        &Vec::new(&env),
        &10000,
        &2000,
    );

    client.process_payment(
        &claim_id,
        &insurance_admin,
        &10000,
        &1690100000,
        &String::from_str(&env, "REF_123"),
    );
    client.apply_patient_payment(&claim_id, &patient_id, &2000, &1690200000);

    let claim = client.get_claim(&claim_id);
    assert_eq!(claim.status, ClaimStatus::Closed);
    assert_eq!(claim.reconciliation_status, ReconciliationStatus::Reconciled);
    assert_eq!(claim.insurer_paid_amount, 10000);
    assert_eq!(claim.patient_paid_amount, 2000);
    assert_eq!(client.get_insurer_payments(&claim_id).len(), 1);
    assert_eq!(client.get_patient_payments(&claim_id).len(), 1);
}

#[test]
fn test_partial_reconciliation_is_tracked() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);
    let insurance_admin = Address::generate(&env);

    let claim_id = client.submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 25000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[1; 32]),
        &25000,
    );

    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &Vec::new(&env),
        &20000,
        &5000,
    );
    client.process_payment(
        &claim_id,
        &insurance_admin,
        &12000,
        &1690100000,
        &String::from_str(&env, "REF_PARTIAL"),
    );
    client.apply_patient_payment(&claim_id, &patient_id, &2000, &1690200000);

    let claim = client.get_claim(&claim_id);
    assert_eq!(claim.status, ClaimStatus::Adjudicated);
    assert_eq!(
        claim.reconciliation_status,
        ReconciliationStatus::PartiallyReconciled
    );
    assert_eq!(claim.insurer_paid_amount, 12000);
    assert_eq!(claim.patient_paid_amount, 2000);
}

#[test]
fn test_submit_claim_rejects_inconsistent_total() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);

    let result = client.try_submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 10000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[2; 32]),
        &9000,
    );
    assert!(matches!(result, Err(Ok(Error::InvalidAmount))));
}

#[test]
fn test_adjudication_rejects_over_allocated_totals() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);
    let insurance_admin = Address::generate(&env);

    let claim_id = client.submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 15000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[3; 32]),
        &15000,
    );

    let result = client.try_adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &Vec::new(&env),
        &12000,
        &4000,
    );
    assert!(matches!(result, Err(Ok(Error::InvalidAmount))));
}

#[test]
fn test_payment_application_is_bounded() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);
    let insurance_admin = Address::generate(&env);

    let claim_id = client.submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 15000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[4; 32]),
        &15000,
    );

    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &Vec::new(&env),
        &10000,
        &2000,
    );

    let insurer_overpay = client.try_process_payment(
        &claim_id,
        &insurance_admin,
        &11000,
        &1690100000,
        &String::from_str(&env, "REF_OVER"),
    );
    assert!(matches!(insurer_overpay, Err(Ok(Error::InvalidAmount))));

    let patient_overpay = client.try_apply_patient_payment(&claim_id, &patient_id, &3000, &1690200000);
    assert!(matches!(patient_overpay, Err(Ok(Error::InvalidAmount))));
}

#[test]
fn test_appeal_workflow_still_functions() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, MedicalClaimsSystem);
    let client = MedicalClaimsSystemClient::new(&env, &contract_id);

    let provider_id = Address::generate(&env);
    let patient_id = Address::generate(&env);
    let insurance_admin = Address::generate(&env);

    let claim_id = client.submit_claim(
        &provider_id,
        &patient_id,
        &12345,
        &1690000000,
        &build_services(&env, 25000),
        &Vec::new(&env),
        &BytesN::from_array(&env, &[5; 32]),
        &25000,
    );

    let mut denials = Vec::new(&env);
    denials.push_back(DenialInfo {
        line_number: 1,
        denial_code: String::from_str(&env, "CO-50"),
        denial_reason: String::from_str(&env, "Not deemed medically necessary"),
        is_appealable: true,
    });

    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &denials,
        &0,
        &0,
    );
    client.appeal_denial(
        &claim_id,
        &provider_id,
        &1,
        &BytesN::from_array(&env, &[6; 32]),
    );

    let res1 = client.try_appeal_denial(
        &claim_id,
        &provider_id,
        &1,
        &BytesN::from_array(&env, &[7; 32]),
    );
    assert!(matches!(res1, Err(Ok(Error::InvalidStateTransition))));

    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &denials,
        &0,
        &0,
    );
    client.appeal_denial(
        &claim_id,
        &provider_id,
        &2,
        &BytesN::from_array(&env, &[8; 32]),
    );

    client.adjudicate_claim(
        &claim_id,
        &insurance_admin,
        &Vec::new(&env),
        &denials,
        &0,
        &0,
    );
    client.appeal_denial(
        &claim_id,
        &provider_id,
        &3,
        &BytesN::from_array(&env, &[9; 32]),
    );

    let claim = client.get_claim(&claim_id);
    assert_eq!(claim.appeal_level, 3);
    assert_eq!(claim.status, ClaimStatus::Appealed);
}
