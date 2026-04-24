#![no_std]
#![allow(deprecated)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, vec,
    Address, BytesN, Env, String, Vec,
};

mod test;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    RateLimitExceeded = 1,
    InvalidScore = 2,
    AlreadyRated = 3,
    Unauthorized = 4,
    NotFound = 5,
    AlreadyInitialized = 6,
    InvalidCredential = 7,
    CredentialExpired = 8,
    CredentialRevoked = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    pub max_records: u32,
    pub window_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRateWindow {
    pub count: u32,
    pub window_start: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CredentialAnchor {
    pub credential_hash: BytesN<32>,
    pub issuer: Address,
    pub attestation_hash: BytesN<32>,
    pub expires_at: u64,
    pub revocation_reference: BytesN<32>,
    pub revoked_at: Option<u64>,
    pub revoked_by: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderProfile {
    pub credential: CredentialAnchor,
    pub active: bool,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub data: String,
    pub created_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderReputation {
    pub total_ratings: u64,
    pub total_score: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Provider(Address),
    Record(String),
    ProviderRecords(Address),
    ProviderRecordCount(Address),
    RateLimitConfig,
    ProviderRate(Address),
    ProviderReputation(Address),
    ProviderRatingByPatient(Address, Address),
}

#[contract]
pub struct ProviderRegistry;

#[contractimpl]
impl ProviderRegistry {
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn set_rate_limit(env: Env, admin: Address, max_records: u32, window_seconds: u64) {
        Self::assert_admin(&env, &admin);
        env.storage().instance().set(
            &DataKey::RateLimitConfig,
            &RateLimitConfig {
                max_records,
                window_seconds,
            },
        );
    }

    pub fn register_provider(
        env: Env,
        admin: Address,
        provider: Address,
        issuer: Address,
        credential_hash: BytesN<32>,
        attestation_hash: BytesN<32>,
        expires_at: u64,
        revocation_reference: BytesN<32>,
    ) -> Result<(), ContractError> {
        Self::assert_admin(&env, &admin);
        provider.require_auth();
        issuer.require_auth();
        Self::assert_future_expiry(&env, expires_at)?;

        let profile = ProviderProfile {
            credential: CredentialAnchor {
                credential_hash,
                issuer,
                attestation_hash,
                expires_at,
                revocation_reference,
                revoked_at: None,
                revoked_by: None,
            },
            active: true,
            registered_at: env.ledger().timestamp(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Provider(provider.clone()), &profile);
        env.events()
            .publish((symbol_short!("reg_prov"), provider), symbol_short!("ok"));
        Ok(())
    }

    pub fn revoke_provider(env: Env, admin: Address, provider: Address) -> Result<(), ContractError> {
        Self::assert_admin(&env, &admin);

        let key = DataKey::Provider(provider.clone());
        let mut profile: ProviderProfile = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NotFound)?;

        profile.active = false;
        profile.credential.revoked_at = Some(env.ledger().timestamp());
        profile.credential.revoked_by = Some(admin.clone());
        env.storage().persistent().set(&key, &profile);

        env.events()
            .publish((symbol_short!("rev_prov"), provider), symbol_short!("ok"));
        Ok(())
    }

    pub fn is_provider(env: Env, provider: Address) -> bool {
        Self::provider_is_active(&env, &provider)
    }

    pub fn get_provider_profile(
        env: Env,
        provider: Address,
    ) -> Result<ProviderProfile, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Provider(provider))
            .ok_or(ContractError::NotFound)
    }

    pub fn add_record(
        env: Env,
        provider: Address,
        record_id: String,
        data: String,
    ) -> Result<(), ContractError> {
        provider.require_auth();
        Self::load_active_provider(&env, &provider)?;
        Self::consume_provider_rate_slot(&env, &provider)?;

        let record = Record {
            data,
            created_by: provider.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::Record(record_id.clone()), &record);

        let list_key = DataKey::ProviderRecords(provider.clone());
        let mut ids: Vec<String> = env
            .storage()
            .persistent()
            .get(&list_key)
            .unwrap_or(vec![&env]);
        ids.push_back(record_id.clone());
        env.storage().persistent().set(&list_key, &ids);

        let count_key = DataKey::ProviderRecordCount(provider.clone());
        let count: u64 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage().persistent().set(&count_key, &(count + 1));

        env.events().publish(
            (symbol_short!("add_rec"), provider, record_id),
            symbol_short!("ok"),
        );
        Ok(())
    }

    pub fn get_record(env: Env, record_id: String) -> Result<Record, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Record(record_id))
            .ok_or(ContractError::NotFound)
    }

    pub fn get_provider_record_count(env: Env, provider: Address) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::ProviderRecordCount(provider))
            .unwrap_or(0)
    }

    pub fn rate_provider(
        env: Env,
        patient: Address,
        provider: Address,
        score: u32,
    ) -> Result<(), ContractError> {
        patient.require_auth();

        if !(1..=5).contains(&score) {
            return Err(ContractError::InvalidScore);
        }
        Self::load_active_provider(&env, &provider)?;

        let patient_rating_key = DataKey::ProviderRatingByPatient(provider.clone(), patient);
        if env.storage().persistent().has(&patient_rating_key) {
            return Err(ContractError::AlreadyRated);
        }

        let reputation_key = DataKey::ProviderReputation(provider.clone());
        let mut reputation: ProviderReputation = env
            .storage()
            .persistent()
            .get(&reputation_key)
            .unwrap_or(ProviderReputation {
                total_ratings: 0,
                total_score: 0,
            });

        reputation.total_ratings += 1;
        reputation.total_score += score as u64;

        env.storage().persistent().set(&patient_rating_key, &true);
        env.storage().persistent().set(&reputation_key, &reputation);
        env.events().publish(
            (symbol_short!("rate"), provider),
            (reputation.total_ratings, score),
        );
        Ok(())
    }

    pub fn get_provider_reputation(env: Env, provider: Address) -> (u64, u64) {
        let reputation_key = DataKey::ProviderReputation(provider);
        let reputation: ProviderReputation = env
            .storage()
            .persistent()
            .get(&reputation_key)
            .unwrap_or(ProviderReputation {
                total_ratings: 0,
                total_score: 0,
            });

        if reputation.total_ratings == 0 {
            return (0, 0);
        }
        let average_scaled = (reputation.total_score * 100) / reputation.total_ratings;
        (reputation.total_ratings, average_scaled)
    }

    pub fn deactivate_provider(
        env: Env,
        admin: Address,
        provider: Address,
        successor: Address,
    ) -> Result<(), ContractError> {
        Self::assert_admin(&env, &admin);
        Self::load_active_provider(&env, &successor)?;

        let list_key = DataKey::ProviderRecords(provider.clone());
        let ids: Vec<String> = env
            .storage()
            .persistent()
            .get(&list_key)
            .unwrap_or(vec![&env]);

        let count = ids.len();
        for id in ids.iter() {
            let rec_key = DataKey::Record(id.clone());
            if let Some(mut rec) = env.storage().persistent().get::<DataKey, Record>(&rec_key) {
                rec.created_by = successor.clone();
                env.storage().persistent().set(&rec_key, &rec);
            }
        }

        if count > 0 {
            let succ_key = DataKey::ProviderRecords(successor.clone());
            let mut succ_ids: Vec<String> = env
                .storage()
                .persistent()
                .get(&succ_key)
                .unwrap_or(vec![&env]);
            for id in ids.iter() {
                succ_ids.push_back(id.clone());
            }
            env.storage().persistent().set(&succ_key, &succ_ids);
        }
        env.storage().persistent().remove(&list_key);

        let key = DataKey::Provider(provider.clone());
        let mut profile: ProviderProfile = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::NotFound)?;
        profile.active = false;
        profile.credential.revoked_at = Some(env.ledger().timestamp());
        profile.credential.revoked_by = Some(admin.clone());
        env.storage().persistent().set(&key, &profile);

        env.events().publish(
            (symbol_short!("prov_deac"), provider.clone()),
            symbol_short!("ok"),
        );
        env.events()
            .publish((symbol_short!("rec_xfer"), provider, successor), count);
        Ok(())
    }

    fn assert_admin(env: &Env, caller: &Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, ContractError::NotFound));
        if *caller != admin {
            panic_with_error!(env, ContractError::Unauthorized);
        }
    }

    fn assert_future_expiry(env: &Env, expires_at: u64) -> Result<(), ContractError> {
        if expires_at <= env.ledger().timestamp() {
            return Err(ContractError::CredentialExpired);
        }
        Ok(())
    }

    fn provider_is_active(env: &Env, provider: &Address) -> bool {
        let Some(profile) = env
            .storage()
            .persistent()
            .get::<DataKey, ProviderProfile>(&DataKey::Provider(provider.clone()))
        else {
            return false;
        };

        profile.active && Self::credential_is_active(env, &profile.credential).is_ok()
    }

    fn load_active_provider(env: &Env, provider: &Address) -> Result<ProviderProfile, ContractError> {
        let profile: ProviderProfile = env
            .storage()
            .persistent()
            .get(&DataKey::Provider(provider.clone()))
            .ok_or(ContractError::NotFound)?;

        if !profile.active {
            return Err(ContractError::Unauthorized);
        }
        Self::credential_is_active(env, &profile.credential)?;
        Ok(profile)
    }

    fn credential_is_active(env: &Env, credential: &CredentialAnchor) -> Result<(), ContractError> {
        if credential.revoked_at.is_some() {
            return Err(ContractError::CredentialRevoked);
        }
        if credential.expires_at <= env.ledger().timestamp() {
            return Err(ContractError::CredentialExpired);
        }
        Ok(())
    }

    fn consume_provider_rate_slot(env: &Env, provider: &Address) -> Result<(), ContractError> {
        let config_opt: Option<RateLimitConfig> =
            env.storage().instance().get(&DataKey::RateLimitConfig);
        let Some(config) = config_opt else {
            return Ok(());
        };
        if config.max_records == 0 || config.window_seconds == 0 {
            return Ok(());
        }

        let now = env.ledger().timestamp();
        let key = DataKey::ProviderRate(provider.clone());
        let mut state: ProviderRateWindow = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(ProviderRateWindow {
                count: 0,
                window_start: 0,
            });

        let window_end = state.window_start.saturating_add(config.window_seconds);
        if state.window_start == 0 || now >= window_end {
            state.count = 0;
            state.window_start = now;
        }

        if state.count >= config.max_records {
            return Err(ContractError::RateLimitExceeded);
        }

        state.count += 1;
        env.storage().persistent().set(&key, &state);
        Ok(())
    }
}
