use soroban_sdk::Env;

use crate::storage::get_milestone;
use crate::types::{Error, MilestoneData};
use crate::views::{find_next_pending_index, get_campaign_or_panic, MilestoneView};

// ─── get_milestone_view ───────────────────────────────────────────────────────

/// Issue #199 — Returns the raw `MilestoneData` for the milestone at `index`.
///
/// Prefer [`get_milestone_by_index`] when you need computed fields
/// (`pending_release`, `is_fully_released`, `is_next_pending`).
/// Use this only when you need the bare stored record without the overhead
/// of computing enriched fields.
///
/// No authentication required (read-only view).
///
/// Panics:
///   `Error::NotInitialized`    — contract not yet initialised.
///   `Error::MilestoneNotFound` — `index` ≥ `milestone_count` or missing
///                                from storage (indicates corrupted state).
pub fn get_milestone_view(env: &Env, index: u32) -> MilestoneData {
    let campaign = get_campaign_or_panic(env);

    if index >= campaign.milestone_count {
        soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound);
    }

    get_milestone(env, index)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, Error::MilestoneNotFound))
}

// ─── get_milestone_view_enriched ─────────────────────────────────────────────

/// Returns the enriched `MilestoneView` for `index` — a convenience re-export
/// of [`crate::views::get_milestone_by_index`] kept here so Issue #199
/// callers have a single import point for both raw and enriched variants.
///
/// Prefer this over `get_milestone_view` unless you specifically need the
/// bare `MilestoneData` record.
///
/// Panics:
///   `Error::NotInitialized`    — contract not yet initialised.
///   `Error::MilestoneNotFound` — `index` ≥ `milestone_count` or missing
///                                from storage.
pub fn get_milestone_view_enriched(env: &Env, index: u32) -> MilestoneView {
    crate::views::get_milestone_by_index(env, index)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    use crate::types::{CampaignData, CampaignStatus, DataKey, MilestoneStatus};

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn make_env() -> Env {
        Env::default()
    }

    fn seed_campaign(env: &Env, milestone_count: u32) {
        let creator = Address::generate(env);
        let campaign = CampaignData {
            creator,
            goal_amount: 10_000,
            raised_amount: 0,
            end_time: 9_999_999,
            status: CampaignStatus::Active,
            milestone_count,
            accepted_assets: soroban_sdk::Vec::new(env),
        };
        env.storage()
            .persistent()
            .set(&DataKey::CampaignData, &campaign);
    }

    fn seed_milestone(env: &Env, index: u32, status: MilestoneStatus) -> MilestoneData {
        let m = MilestoneData {
            index,
            target_amount: (index as i128 + 1) * 1_000,
            released_amount: if status == MilestoneStatus::Released {
                (index as i128 + 1) * 1_000
            } else {
                0
            },
            description_hash: soroban_sdk::BytesN::from_array(env, &[0u8; 32]),
            status,
            released_at: None,
            released_at_ledger: None,
            release_tx: None,
            released_to: None,
        };
        env.storage()
            .persistent()
            .set(&DataKey::MilestoneData(index), &m);
        m
    }

    // ── get_milestone_view ───────────────────────────────────────────────────

    #[test]
    fn returns_raw_milestone_data_for_valid_index() {
        let env = make_env();
        seed_campaign(&env, 2);
        let stored = seed_milestone(&env, 0, MilestoneStatus::Locked);

        let result = get_milestone_view(&env, 0);
        assert_eq!(result, stored);
    }

    #[test]
    fn returns_correct_milestone_for_non_zero_index() {
        let env = make_env();
        seed_campaign(&env, 3);
        seed_milestone(&env, 0, MilestoneStatus::Released);
        seed_milestone(&env, 1, MilestoneStatus::Unlocked);
        let stored = seed_milestone(&env, 2, MilestoneStatus::Locked);

        let result = get_milestone_view(&env, 2);
        assert_eq!(result.index, stored.index);
        assert_eq!(result.target_amount, stored.target_amount);
        assert_eq!(result.status, MilestoneStatus::Locked);
    }

    #[test]
    #[should_panic]
    fn panics_when_index_equals_milestone_count() {
        let env = make_env();
        seed_campaign(&env, 1);
        seed_milestone(&env, 0, MilestoneStatus::Locked);

        // index == milestone_count (1) → out of bounds
        get_milestone_view(&env, 1);
    }

    #[test]
    #[should_panic]
    fn panics_when_index_exceeds_milestone_count() {
        let env = make_env();
        seed_campaign(&env, 1);
        seed_milestone(&env, 0, MilestoneStatus::Locked);

        get_milestone_view(&env, 99);
    }

    #[test]
    #[should_panic]
    fn panics_when_contract_not_initialised() {
        // No campaign seeded → get_campaign_or_panic should fire NotInitialized
        let env = make_env();
        get_milestone_view(&env, 0);
    }

    // ── get_milestone_view_enriched ──────────────────────────────────────────

    #[test]
    fn enriched_view_includes_pending_release_and_flags() {
        let env = make_env();
        seed_campaign(&env, 2);
        seed_milestone(&env, 0, MilestoneStatus::Released);
        let stored = seed_milestone(&env, 1, MilestoneStatus::Unlocked);

        let view = get_milestone_view_enriched(&env, 1);

        assert_eq!(view.data, stored);
        assert_eq!(view.pending_release, stored.target_amount); // nothing released yet
        assert!(!view.is_fully_released);
        assert!(view.is_next_pending, "index 1 should be next pending");
    }

    #[test]
    fn enriched_view_is_fully_released_for_released_milestone() {
        let env = make_env();
        seed_campaign(&env, 1);
        let stored = seed_milestone(&env, 0, MilestoneStatus::Released);

        let view = get_milestone_view_enriched(&env, 0);

        assert!(view.is_fully_released);
        assert_eq!(view.pending_release, 0);
        assert!(!view.is_next_pending);
    }

    #[test]
    fn enriched_view_is_not_next_pending_for_locked_milestone() {
        let env = make_env();
        seed_campaign(&env, 2);
        seed_milestone(&env, 0, MilestoneStatus::Unlocked);
        seed_milestone(&env, 1, MilestoneStatus::Locked);

        // index 0 is Unlocked → it is next pending; index 1 is NOT
        let view = get_milestone_view_enriched(&env, 1);
        assert!(!view.is_next_pending);
    }
}