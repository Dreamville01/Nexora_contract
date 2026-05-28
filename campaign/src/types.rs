use soroban_sdk::{contracttype, Address, BytesN, Vec};

// ── Error enum ──────────────────────────────────────────────────────────────

/// All error types for validation and state transitions
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    // ── Initialization validation errors ──
    InvalidGoalAmount,        // goal_amount must be > 0
    InvalidEndTime,           // end_time must be > current ledger timestamp
    InvalidAssets,            // accepted_assets must be non-empty
    InvalidMilestones,        // milestones must be sorted ascending and last must equal goal
    MilestoneMismatch,        // last milestone.target_amount != goal_amount
    
    // ── State transition errors ──
    InvalidCampaignTransition, // campaign status transition not allowed
    InvalidMilestoneTransition,// milestone status transition not allowed
    CampaignNotActive,        // campaign must be Active to accept donations
    CampaignEnded,            // campaign end_time has passed
    GoalNotReached,           // cannot transition to GoalReached before reaching goal
}

// ── Supporting enums ─────────────────────────────────────────────────────────

/// Issue #167 – campaign lifecycle status
/// State transitions:
///   Active -> GoalReached (goal reached)
///   Active -> Ended (deadline passed)
///   GoalReached -> Ended (deadline passed)
///   Active/GoalReached/Ended -> Cancelled (by creator)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    Active,      // Campaign accepting donations
    GoalReached, // Goal amount reached, still accepting donations until deadline
    Ended,       // Deadline passed or campaign concluded
    Cancelled,   // Campaign cancelled by creator
}

/// Issue #168 – milestone release status
/// State transitions:
///   Locked -> Unlocked (when target_amount reached)
///   Unlocked -> Released (when explicitly released by admin)
///   Locked/Unlocked -> Released (milestone marked as released)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Locked,   // Milestone condition not yet met
    Unlocked, // Target amount reached, awaiting release
    Released, // Funds released to beneficiary
}

/// Accepted asset descriptor (native XLM or a Stellar asset)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetInfo {
    Native,
    Stellar(Address),
}

// ── Issue #166 – storage key enum ────────────────────────────────────────────

/// All persistent storage keys used by the campaign contract.
/// Implements `contracttype` so Soroban can serialise it via XDR / `IntoVal`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    CampaignData,
    MilestoneData(u32),
    DonorData(Address),
    TotalRaised,
    ContractStatus,
}

// ── Issue #167 – CampaignData struct ─────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignData {
    pub creator: Address,
    pub goal_amount: i128,
    pub raised_amount: i128,
    pub end_time: u64,
    pub status: CampaignStatus,
    pub accepted_assets: Vec<AssetInfo>,
    pub milestone_count: u32,
}

// ── Issue #168 – MilestoneData struct ────────────────────────────────────────

/// Max 5 milestones enforced at the contract call site.
pub const MAX_MILESTONES: u32 = 5;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneData {
    pub index: u32,
    pub target_amount: i128,
    pub description_hash: BytesN<32>,
    pub status: MilestoneStatus,
    pub released_at: Option<u64>,
    pub release_tx: Option<BytesN<32>>,
}

// ── Issue #169 – DonorRecord struct ──────────────────────────────────────────

/// Stored under `DataKey::DonorData(donor_address)`.
/// Aggregated per-donor across multiple donations.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DonorRecord {
    pub donor: Address,
    pub total_donated: i128,
    pub asset: AssetInfo,
    pub last_donation_time: u64,
}
