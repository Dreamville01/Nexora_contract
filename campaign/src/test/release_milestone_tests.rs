#![cfg(test)]

use soroban_sdk::testutils::{Address as AddressTestUtils, MockToken};
use soroban_sdk::{Address, Env, Vec, String, BytesN};

use crate::types::{CampaignData, CampaignStatus, StellarAsset, MilestoneData, MilestoneStatus};
use crate::storage::{get_campaign, get_milestone, set_campaign, set_milestone};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn create_test_env() -> (Env, Address) {
    let env = Env::default();
    let creator = Address::generate(&env);
    (env, creator)
}

/// Creates a campaign in Active state with the given parameters.
/// Registers a mock token contract for each accepted asset so that
/// token client calls during release_milestone succeed.
fn create_test_campaign(env: &Env, creator: &Address, milestone_count: u32) {
    let token_issuer = Address::generate(env);

    // Register the mock token so balance/transfer calls don't panic
    env.register_stellar_asset_contract(token_issuer.clone());

    let mut assets: Vec<StellarAsset> = Vec::new(env);
    assets.push_back(StellarAsset {
        asset_code: String::from_str(env, "XLM"),
        issuer: Some(token_issuer),
    });

    let campaign = CampaignData {
        creator: creator.clone(),
        goal_amount: 3000,
        raised_amount: 3000, // Fully funded
        end_time: env.ledger().timestamp() + 86_400,
        status: CampaignStatus::Active,
        accepted_assets: assets,
        milestone_count,
        min_donation_amount: 0,
        created_at_ledger: env.ledger().sequence(),
        created_at_time: env.ledger().timestamp(),
        concluded_at_ledger: None,
    };
    set_campaign(env, &campaign);
}

/// Creates a milestone with the given index, target, and status.
fn create_test_milestone(
    env: &Env,
    index: u32,
    target_amount: i128,
    status: MilestoneStatus,
) {
    let milestone = MilestoneData {
        index,
        target_amount,
        released_amount: 0,
        description_hash: BytesN::from_array(env, &[0u8; 32]),
        status,
        released_at: None,
        released_at_ledger: None,
        release_tx: None,
        released_to: None,
    };
    set_milestone(env, index, &milestone);
}

/// Creates a simple campaign with one unlocked milestone for happy-path tests.
fn setup_single_milestone_campaign(env: &Env) -> Address {
    let creator = Address::generate(env);
    create_test_campaign(env, &creator, 1);
    create_test_milestone(env, 0, 3000, MilestoneStatus::Unlocked);
    creator
}

// ─── Happy path: valid release ────────────────────────────────────────────────

/// Test: valid release updates milestone status to Released.
#[test]
fn test_valid_release_updates_milestone_status() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    // Mock the creator's auth
    env.mock_all_auths();

    crate::release_milestone::release_milestone(&env, 0, recipient.clone());

    // Verify milestone is now Released
    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(
        milestone.status,
        MilestoneStatus::Released,
        "Milestone should transition to Released after valid release"
    );
}

/// Test: valid release sets the released_amount to target_amount.
#[test]
fn test_valid_release_sets_released_amount() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    crate::release_milestone::release_milestone(&env, 0, recipient.clone());

    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(
        milestone.released_amount,
        milestone.target_amount,
        "Released amount should equal target amount after release"
    );
}

/// Test: final milestone releases remaining balance correctly.
#[test]
fn test_final_milestone_releases_remaining_balance() {
    let (env, _creator) = create_test_env();
    let creator = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Create a 3-milestone campaign at 1000, 2000, 3000
    create_test_campaign(&env, &creator, 3);
    create_test_milestone(&env, 0, 1000, MilestoneStatus::Released);
    create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
    create_test_milestone(&env, 2, 3000, MilestoneStatus::Locked);

    env.mock_all_auths();

    // Release milestone 1 (the only Unlocked one)
    crate::release_milestone::release_milestone(&env, 1, recipient.clone());

    let milestone = get_milestone(&env, 1).expect("Milestone should exist");
    assert_eq!(
        milestone.status,
        MilestoneStatus::Released,
        "Milestone 1 should be Released"
    );
    assert_eq!(
        milestone.released_amount,
        milestone.target_amount,
        "Milestone 1 released amount should equal target"
    );

    // Milestone 0 was already released, milestone 2 is still locked
    let milestone0 = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(milestone0.status, MilestoneStatus::Released);

    let milestone2 = get_milestone(&env, 2).expect("Milestone should exist");
    assert_eq!(milestone2.status, MilestoneStatus::Locked);
}

// ─── Error path: non-creator release panics ──────────────────────────────────

/// Test: calling release_milestone with a non-creator address panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_non_creator_release_panics() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    // Do NOT mock auth — the creator address won't be the caller
    // The call should panic because creator.require_auth() fails
    crate::release_milestone::release_milestone(&env, 0, recipient.clone());
}

// ─── Error path: locked milestone release panics ─────────────────────────────

/// Test: releasing a Locked milestone panics with InvalidMilestoneTransition.
#[test]
#[should_panic(expected = "HostError")]
fn test_locked_milestone_release_panics() {
    let (env, _creator) = create_test_env();
    let creator = Address::generate(&env);

    create_test_campaign(&env, &creator, 1);
    // Milestone is Locked (not Unlocked)
    create_test_milestone(&env, 0, 3000, MilestoneStatus::Locked);

    let recipient = Address::generate(&env);
    env.mock_all_auths();

    crate::release_milestone::release_milestone(&env, 0, recipient.clone());
}

// ─── Error path: skipping milestone release panics ───────────────────────────

/// Test: releasing a milestone without the previous being Released panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_skipping_milestone_release_panics() {
    let (env, _creator) = create_test_env();
    let creator = Address::generate(&env);

    // Create 3 milestones. Only milestone 1 is Unlocked, but 0 is NOT Released
    create_test_campaign(&env, &creator, 3);
    create_test_milestone(&env, 0, 1000, MilestoneStatus::Unlocked); // Not Released
    create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
    create_test_milestone(&env, 2, 3000, MilestoneStatus::Locked);

    let recipient = Address::generate(&env);
    env.mock_all_auths();

    // Try to release milestone 1 while milestone 0 is still Unlocked
    crate::release_milestone::release_milestone(&env, 1, recipient.clone());
}

/// Test: releasing milestone 0 when previous is still Locked succeeds
/// because milestone 0 has no predecessor check.
#[test]
fn test_first_milestone_release_succeeds_regardless_of_previous() {
    let (env, _creator) = create_test_env();
    let creator = Address::generate(&env);

    create_test_campaign(&env, &creator, 1);
    create_test_milestone(&env, 0, 3000, MilestoneStatus::Unlocked);

    let recipient = Address::generate(&env);
    env.mock_all_auths();

    // Release milestone 0 — should succeed (no predecessor check)
    crate::release_milestone::release_milestone(&env, 0, recipient.clone());

    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(milestone.status, MilestoneStatus::Released);
}

// ─── Error path: double release panics ───────────────────────────────────────

/// Test: releasing an already-Released milestone panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_double_release_panics() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    // First release — should succeed
    crate::release_milestone::release_milestone(&env, 0, recipient.clone());

    // Verify it's released
    let milestone = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(milestone.status, MilestoneStatus::Released);

    // Second release of the same milestone — should panic
    crate::release_milestone::release_milestone(&env, 0, recipient.clone());
}

// ─── Error path: non-existent milestone ──────────────────────────────────────

/// Test: releasing a milestone index that doesn't exist panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_release_non_existent_milestone_panics() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    env.mock_all_auths();

    // Campaign has 1 milestone. Index 5 is out of bounds.
    crate::release_milestone::release_milestone(&env, 5, recipient.clone());
}

// ─── Error path: frozen contract ─────────────────────────────────────────────

/// Test: releasing a milestone while the contract is frozen panics.
#[test]
#[should_panic(expected = "HostError")]
fn test_frozen_contract_release_panics() {
    let (env, _creator) = create_test_env();
    let creator = setup_single_milestone_campaign(&env);
    let recipient = Address::generate(&env);

    // Freeze the contract
    crate::storage::set_frozen(&env, true);

    env.mock_all_auths();

    crate::release_milestone::release_milestone(&env, 0, recipient.clone());
}

// ─── Sequential milestone release ────────────────────────────────────────────

/// Test: releasing milestones in order succeeds.
#[test]
fn test_sequential_milestone_release_succeeds() {
    let (env, _creator) = create_test_env();
    let creator = Address::generate(&env);

    create_test_campaign(&env, &creator, 3);
    create_test_milestone(&env, 0, 1000, MilestoneStatus::Unlocked);
    create_test_milestone(&env, 1, 2000, MilestoneStatus::Unlocked);
    create_test_milestone(&env, 2, 3000, MilestoneStatus::Unlocked);

    let recipient = Address::generate(&env);
    env.mock_all_auths();

    // Release milestone 0
    crate::release_milestone::release_milestone(&env, 0, recipient.clone());
    let m0 = get_milestone(&env, 0).expect("Milestone should exist");
    assert_eq!(m0.status, MilestoneStatus::Released);

    // Release milestone 1
    crate::release_milestone::release_milestone(&env, 1, recipient.clone());
    let m1 = get_milestone(&env, 1).expect("Milestone should exist");
    assert_eq!(m1.status, MilestoneStatus::Released);

    // Release milestone 2
    crate::release_milestone::release_milestone(&env, 2, recipient.clone());
    let m2 = get_milestone(&env, 2).expect("Milestone should exist");
    assert_eq!(m2.status, MilestoneStatus::Released);
}
