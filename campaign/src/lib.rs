#![no_std]

pub mod storage;
pub mod types;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
use types::{CampaignData, CampaignInitializedEvent, CampaignStatus, DonorRecord, Error, MilestoneData, MilestoneStatus, StellarAsset, AssetInfo};
use storage::{get_campaign, set_campaign, get_milestone, set_milestone, get_donor, set_donor, get_total_raised as storage_get_total_raised, set_total_raised};

pub const VERSION: u32 = 1;

#[contract]
pub struct CampaignContract;

#[contractimpl]
impl CampaignContract {
    /// Initialize a new campaign with strict validation on all inputs.
    ///
    /// Requires: Creator authorization via `creator.require_auth()`
    /// Can only be called once per contract instance
    ///
    /// # Panics
    /// - `Error::UnauthorizedCreator`   if caller is not the creator
    /// - `Error::AlreadyInitialized`    if campaign already exists
    /// - `Error::InvalidGoalAmount`     if goal_amount <= 0
    /// - `Error::InvalidEndTime`        if end_time <= current ledger timestamp
    /// - `Error::InvalidAssets`         if accepted_assets is empty
    /// - `Error::InvalidAssetCode`      if any asset_code is empty
    /// - `Error::InvalidMilestoneCount` if milestone count is not 1-5
    /// - `Error::InvalidMilestones`     if milestones are not sorted ascending
    /// - `Error::MilestoneMismatch`     if last milestone.target_amount != goal_amount
    pub fn initialize(
        env: Env,
        creator: soroban_sdk::Address,
        goal_amount: i128,
        end_time: u64,
        accepted_assets: Vec<StellarAsset>,
        milestones: Vec<MilestoneData>,
        min_donation_amount: i128,
    ) -> Result<(), Error> {
        creator.require_auth();

        if get_campaign(&env).is_some() {
            panic_with_error(&env, Error::AlreadyInitialized);
        }

        if goal_amount <= 0 {
            panic_with_error(&env, Error::InvalidGoalAmount);
        }

        let current_timestamp = env.ledger().timestamp();
        if end_time <= current_timestamp {
            panic_with_error(&env, Error::InvalidEndTime);
        }

        if accepted_assets.is_empty() {
            panic_with_error(&env, Error::InvalidAssets);
        }

        validate_assets(&env, &accepted_assets)?;

        let milestone_count = milestones.len() as u32;
        if milestone_count == 0 || milestone_count > types::MAX_MILESTONES {
            panic_with_error(&env, Error::InvalidMilestoneCount);
        }

        validate_milestones(&env, &milestones, goal_amount)?;

        let campaign = CampaignData {
            creator: creator.clone(),
            goal_amount,
            raised_amount: 0,
            end_time,
            status: CampaignStatus::Active,
            accepted_assets: accepted_assets.clone(),
            milestone_count,
            min_donation_amount,
        };

        set_campaign(&env, &campaign);

        for (index, milestone) in milestones.iter().enumerate() {
            set_milestone(&env, index as u32, &milestone);
        }

        env.events().publish(
            ("campaign", "initialized"),
            CampaignInitializedEvent {
                creator,
                goal_amount,
                end_time,
                asset_count: accepted_assets.len() as u32,
                milestone_count,
            },
        );

        Ok(())
    }

    /// Issue #194 – Donate to the campaign, enforcing campaign status.
    ///
    /// Panics with `Error::CampaignNotActive` unless status is `Active` or `GoalReached`.
    /// The status check is atomic with the state update to prevent race conditions.
    ///
    /// Issue #195 – After updating raised_amount, loops over milestones and unlocks
    ///              any whose target_amount <= raised_amount and status == Locked.
    /// Issue #198 – After donation, transitions to GoalReached if raised_amount >= goal_amount.
    pub fn donate(env: Env, donor: Address, amount: i128, asset: AssetInfo) {
        donor.require_auth();

        let mut campaign: CampaignData = get_campaign(&env)
            .unwrap_or_else(|| panic_with_error(&env, Error::NotInitialized));

        // Issue #194 – status check: only Active or GoalReached campaigns accept donations
        match campaign.status {
            CampaignStatus::Active | CampaignStatus::GoalReached => {}
            _ => panic_with_error(&env, Error::CampaignNotActive),
        }

        // Issue #195 – update raised_amount atomically
        campaign.raised_amount += amount;

        // Issue #198 – goal reached status transition
        if campaign.raised_amount >= campaign.goal_amount
            && campaign.status == CampaignStatus::Active
        {
            campaign.status = CampaignStatus::GoalReached;
            env.events().publish(
                ("campaign", "campaign_goal_reached"),
                campaign.raised_amount,
            );
        }

        set_campaign(&env, &campaign);

        // Issue #195 – update TotalRaised storage
        let new_total = storage_get_total_raised(&env) + amount;
        set_total_raised(&env, new_total);

        // Issue #195 – update donor record
        let mut donor_record = get_donor(&env, &donor).unwrap_or(DonorRecord {
            donor: donor.clone(),
            total_donated: 0,
            asset: asset.clone(),
            last_donation_time: 0,
        });
        donor_record.total_donated += amount;
        donor_record.asset = asset;
        donor_record.last_donation_time = env.ledger().timestamp();
        set_donor(&env, &donor, &donor_record);

        // Issue #195 – milestone unlock check
        for i in 0..campaign.milestone_count {
            if let Some(mut milestone) = get_milestone(&env, i) {
                if milestone.status == MilestoneStatus::Locked
                    && campaign.raised_amount >= milestone.target_amount
                {
                    milestone.status = MilestoneStatus::Unlocked;
                    set_milestone(&env, i, &milestone);
                    env.events().publish(
                        ("campaign", "milestone_unlocked"),
                        (i, milestone.target_amount),
                    );
                }
            }
        }

        env.events().publish(("campaign", "donation_received"), (donor, amount));
    }

    /// Issue #197 – Returns the total amount raised by the campaign.
    /// No auth required. Returns 0 if no donations yet.
    pub fn get_total_raised(env: Env) -> i128 {
        storage_get_total_raised(&env)
    }

    /// Issue #196 – Returns the donor record for the given address.
    /// No auth required. Returns None if the address has never donated.
    pub fn get_donor_record(env: Env, donor: Address) -> Option<DonorRecord> {
        get_donor(&env, &donor)
    }

    pub fn hello(env: Env) -> soroban_sdk::Symbol {
        soroban_sdk::Symbol::new(&env, "campaign")
    }

    pub fn version() -> u32 {
        VERSION
    }
}

/// Issue #175 – assert the current invoker is the campaign creator.
///
/// Reads the creator address from campaign storage and calls `require_auth()`.
/// Panics with `Error::UnauthorizedCreator` if the campaign is not initialized;
/// Soroban's auth framework panics if the invoker is not the creator.
fn require_creator(env: &Env) {
    let campaign =
        get_campaign(env).unwrap_or_else(|| panic_with_error(env, Error::UnauthorizedCreator));
    campaign.creator.require_auth();
}

/// Validates that `asset` is in the campaign's accepted list and returns the
/// token contract address needed to construct a `token::Client`.
///
/// - `AssetInfo::Stellar(addr)` → `addr` must match an accepted asset's issuer.
/// - `AssetInfo::Native` (XLM) → finds the XLM entry by asset_code and uses its issuer.
fn get_token_address_for_asset(
    env: &Env,
    asset: &AssetInfo,
    campaign: &CampaignData,
) -> Address {
    match asset {
        AssetInfo::Stellar(addr) => {
            let accepted = campaign
                .accepted_assets
                .iter()
                .any(|a| a.issuer == Some(addr.clone()));
            if !accepted {
                panic_with_error(env, Error::AssetNotAccepted);
            }
            addr.clone()
        }
        AssetInfo::Native => {
            // Find the XLM entry in accepted_assets by asset_code == "XLM".
            // Its issuer must hold the wrapped native token contract address.
            let xlm_code = soroban_sdk::String::from_str(env, "XLM");
            campaign
                .accepted_assets
                .iter()
                .find(|a| a.asset_code == xlm_code)
                .and_then(|a| a.issuer.clone())
                .unwrap_or_else(|| panic_with_error(env, Error::AssetNotAccepted))
        }
    }
}

fn validate_assets(env: &Env, assets: &Vec<StellarAsset>) -> Result<(), Error> {
    for asset in assets.iter() {
        if asset.asset_code.len() == 0 {
            panic_with_error(env, Error::InvalidAssetCode);
        }
    }
    Ok(())
}

fn validate_milestones(
    env: &Env,
    milestones: &Vec<MilestoneData>,
    goal_amount: i128,
) -> Result<(), Error> {
    for i in 1..milestones.len() {
        let prev = &milestones.get(i - 1).unwrap();
        let current = &milestones.get(i).unwrap();

        if prev.target_amount >= current.target_amount {
            panic_with_error(env, Error::InvalidMilestones);
        }
    }

    if let Some(last_milestone) = milestones.last() {
        if last_milestone.target_amount != goal_amount {
            panic_with_error(env, Error::MilestoneMismatch);
        }
    } else {
        panic_with_error(env, Error::InvalidMilestones);
    }

    Ok(())
}

/// Panics the contract execution with the given error code.
/// With `contracterror`, `Error` implements `Into<soroban_sdk::Error>` directly.
fn panic_with_error(env: &Env, error: Error) -> ! {
    env.panic_with_error(error)
}

/// Validates campaign status transitions; panics if invalid.
///
/// Valid transitions:
///   Active -> GoalReached (goal reached)
///   Active -> Ended (deadline passes)
///   GoalReached -> Ended (deadline passes)
///   Active/GoalReached/Ended -> Cancelled (by creator)
pub fn validate_campaign_transition(
    env: &Env,
    current_status: &CampaignStatus,
    next_status: &CampaignStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (CampaignStatus::Active, CampaignStatus::GoalReached) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::Active, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::GoalReached, CampaignStatus::Ended) => Ok(()),
        (CampaignStatus::GoalReached, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::Ended, CampaignStatus::Cancelled) => Ok(()),
        (CampaignStatus::Cancelled, _) => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
        _ => {
            panic_with_error(env, Error::InvalidCampaignTransition);
        }
    }
}

/// Validates milestone status transitions; panics if invalid.
///
/// Valid transitions:
///   Locked -> Unlocked (target_amount reached)
///   Unlocked -> Released (explicitly released)
///   Locked -> Released (direct release)
pub fn validate_milestone_transition(
    env: &Env,
    current_status: &MilestoneStatus,
    next_status: &MilestoneStatus,
) -> Result<(), Error> {
    match (current_status, next_status) {
        (MilestoneStatus::Locked, MilestoneStatus::Unlocked) => Ok(()),
        (MilestoneStatus::Locked, MilestoneStatus::Released) => Ok(()),
        (MilestoneStatus::Unlocked, MilestoneStatus::Released) => Ok(()),
        (MilestoneStatus::Released, _) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        (MilestoneStatus::Unlocked, MilestoneStatus::Locked) => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
        _ => {
            panic_with_error(env, Error::InvalidMilestoneTransition);
        }
    }
}
