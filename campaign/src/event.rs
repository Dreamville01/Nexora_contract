use soroban_sdk::{Address, Env, String, Symbol};

pub fn donation_received(
    env: &Env,
    donor: &Address,
    amount: i128,
    asset_code: String,
    raised_total: i128,
    timestamp: u64,
) {
    let topics = (Symbol::new(env, "donation_received"), env.current_contract_address());
    env.events().publish(topics, (donor, amount, asset_code, raised_total, timestamp));
}

pub fn milestone_unlocked(
    env: &Env,
    milestone_index: u32,
    target_amount: i128,
    raised_total: i128,
) {
    let topics = (Symbol::new(env, "milestone_unlocked"), env.current_contract_address());
    env.events().publish(topics, (milestone_index, target_amount, raised_total));
}

pub fn deadline_extended(
    env: &Env,
    creator: &Address,
    old_deadline: u64,
    new_deadline: u64,
) {
    env.events().publish(
        ("campaign", "deadline_extended"),
        (creator, old_deadline, new_deadline),
    );
}

pub fn campaign_cancelled(env: &Env, creator: &Address) {
    env.events().publish(("campaign", "campaign_cancelled"), creator);
}

pub fn campaign_ended(env: &Env) {
    env.events().publish(("campaign", "campaign_ended"), ());
}

pub fn milestone_released(
    env: &Env,
    milestone_index: u32,
    amount: i128,
    asset_code: String,
    recipient: &Address,
    timestamp: u64,
) {
    let topics = (Symbol::new(env, "milestone_released"), env.current_contract_address());
    env.events().publish(topics, (milestone_index, amount, asset_code, recipient, timestamp));
}

/// Issue #246 – Emitted when the contract is upgraded by the admin.
pub fn contract_upgraded(env: &Env, admin: &Address, new_wasm_hash: soroban_sdk::BytesN<32>, timestamp: u64) {
    env.events().publish(
        ("campaign", "contract_upgraded"),
        (admin, new_wasm_hash, timestamp),
    );
}

/// Issue #246 – Emitted when the contract is frozen by the admin.
pub fn contract_frozen(env: &Env, admin: &Address, timestamp: u64) {
    env.events().publish(
        ("campaign", "contract_frozen"),
        (admin, timestamp),
    );
}

/// Issue #246 – Emitted when the contract is unfrozen by the admin.
pub fn contract_unfrozen(env: &Env, admin: &Address, timestamp: u64) {
    env.events().publish(
        ("campaign", "contract_unfrozen"),
        (admin, timestamp),
    );
}
