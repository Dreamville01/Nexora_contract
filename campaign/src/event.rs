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
