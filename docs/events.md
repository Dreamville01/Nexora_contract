# Contract Event Schemas

All events are emitted via `env.events().publish(topics, data)` and are filterable through Stellar Horizon's event streaming API.

---

## `campaign_initialized`

Emitted once when the campaign contract is successfully initialized.

**Topics:** `["campaign", "initialized"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |
| `goal_amount` | `i128` | Total funding target in base units |
| `end_time` | `u64` | UNIX timestamp after which donations are rejected |
| `asset_count` | `u32` | Number of accepted assets |
| `milestone_count` | `u32` | Number of milestones registered |

---

## `donation_received`

Emitted after every successful donation, once storage has been updated.

**Topics:** `["donation_received", contract_address]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `donor` | `Address` | Donor's Stellar address |
| `amount` | `i128` | Donated amount in base units |
| `asset_code` | `String` | Asset code (e.g. `"XLM"`, `"USDC"`) |
| `raised_total` | `i128` | Cumulative raised amount after this donation |
| `timestamp` | `u64` | Ledger timestamp of the donation |

---

## `milestone_unlocked`

Emitted once per milestone when its target is first reached. Not re-emitted if the milestone is already unlocked.

**Topics:** `["milestone_unlocked", contract_address]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `target_amount` | `i128` | Funding threshold that triggered the unlock |
| `raised_total` | `i128` | Cumulative raised amount at time of unlock |

---

## `milestone_released`

Emitted when a milestone's funds are released to the beneficiary.

**Topics:** `["campaign", "milestone_released"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `milestone_index` | `u32` | Zero-based milestone index |
| `scheduled_release` | `i128` | Amount scheduled for release |
| `total_released` | `i128` | Cumulative released amount for this milestone |
| `assets_released` | `u32` | Number of asset types released |
| `recipient` | `Address` | Address that received the funds |

---

## `campaign_ended`

Emitted when the campaign transitions to the `Ended` state (deadline passed or concluded normally).

**Topics:** `["campaign", "campaign_ended"]`

**Data:** `()` (no additional data)

---

## `campaign_cancelled`

Emitted when the campaign creator cancels the campaign.

**Topics:** `["campaign", "campaign_cancelled"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |

---

## `refund_issued`

Emitted when a donor successfully claims a refund.

**Topics:** `["campaign", "refund_issued"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `donor` | `Address` | Donor's Stellar address |
| `amount` | `i128` | Refunded amount in base units |
| `asset` | `AssetInfo` | Asset used for the refund |

---

## `deadline_extended`

Emitted when the campaign creator extends the campaign deadline.

**Topics:** `["campaign", "deadline_extended"]`

**Data:**

| Field | Type | Description |
|---|---|---|
| `creator` | `Address` | Campaign creator's Stellar address |
| `old_deadline` | `u64` | Previous deadline UNIX timestamp |
| `new_deadline` | `u64` | New deadline UNIX timestamp |

---

## Naming Convention

- Event names use `snake_case`.
- Topics are a tuple of `(event_name, contract_address)` for domain events, or `("campaign", event_name)` for lifecycle events.
- All amounts are in base units (stroops for XLM, smallest unit for other assets).
- All timestamps are UNIX seconds from the Soroban ledger (`env.ledger().timestamp()`).
