# Rust Staking Contract

A Solana staking program built using **Rust** and the **Anchor framework**. Users can stake SOL into a Program Derived Address (PDA), earn time-based reward points, claim accumulated points, and unstake their SOL.

> **Project status:** Development / Learning project. Reward points are currently recorded on-chain as accounting units. This implementation does not mint or distribute SPL reward tokens.

## Features

- **PDA-based staking accounts:** Each user gets an individual staking account.
- **SOL staking:** Transfer SOL from the user's wallet to their staking PDA.
- **SOL unstaking:** Withdraw staked SOL while maintaining the PDA's rent-exempt reserve.
- **Time-based rewards:** Earn points based on staked SOL and elapsed time.
- **Reward claiming:** Claim accumulated whole points.
- **Fractional reward accounting:** Preserve fractional micro-points and calculation remainders.
- **Access control:** Only the staking account owner can manage their stake.
- **Checked arithmetic:** Protect important calculations against overflow and underflow.
- **LiteSVM testing:** Test staking operations in a lightweight Solana virtual environment.

## Tech Stack

- Rust
- Solana
- Anchor Framework
- LiteSVM
- Cargo

## Architecture

Each user has an individual staking PDA derived using:

```rust
seeds = [b"client1", user.key().as_ref()]
```

The PDA stores the user's staking information, including:

- Owner public key
- Staked amount
- Total unclaimed points
- Lifetime claimed points
- Last reward update timestamp
- PDA bump
- Fractional reward remainder

## Reward Calculation

The current program uses the following reward rate:

**1 SOL = 1,000,000 points per day**

Reward calculation:

```text
earned_micro_points =
    staked_lamports * elapsed_seconds * points_per_SOL_per_day
    -------------------------------------------------------------
                  lamports_per_SOL * seconds_per_day
```

The program uses:

- 1 SOL = 1,000,000,000 lamports
- 1 day = 86,400 seconds
- 1 point = 1,000,000 micro-points

Intermediate calculations use `u128` to reduce overflow risks.

Fractional rewards are preserved instead of being discarded. Only whole points are moved into the lifetime claimed-points counter.

**Note:** Points are accounting units, not SOL, SPL tokens, or guaranteed financial returns.

## Program Instructions

| Instruction | Description |
|---|---|
| `create_pda_account()` | Creates and initializes the user's staking PDA. |
| `stake(amount)` | Transfers SOL into the PDA and updates the stake. |
| `unstake(amount)` | Withdraws SOL from the PDA. |
| `claim_points()` | Updates rewards and claims whole points. |
| `get_points()` | Calculates and displays current unclaimed rewards. |

## Project Structure

```text
rust-staking-contract/
├── Anchor.toml
├── Cargo.toml
├── Cargo.lock
├── package.json
├── rust-toolchain.toml
└── programs/
    └── rust-staking-contract/
        ├── Cargo.toml
        ├── src/
        │   ├── lib.rs
        │   ├── state.rs
        │   ├── constants.rs
        │   ├── error.rs
        │   └── instructions.rs
        └── tests/
            └── test_staking.rs
```

The active staking implementation and `StakeAccount` definition are currently in `src/lib.rs`.

## Prerequisites

Install the following tools:

- Rust
- Cargo
- Solana CLI
- Anchor CLI
- Node.js
- npm

Verify your installations:

```bash
rustc --version
cargo --version
solana --version
anchor --version
node --version
npm --version
```

The workspace specifies Rust 1.89.0 and the program uses Anchor Lang 1.1.2.

## Installation

Clone the repository:

```bash
git clone https://github.com/Rav1Chauhan/rust-staking-contract.git

cd rust-staking-contract
```

## Build

Build the Solana program:

```bash
anchor build
```

The compiled program is expected at:

```text
target/deploy/rust_staking_contract.so
```

## Run Tests

The project uses LiteSVM for Rust integration testing.

Build the program first:

```bash
anchor build
```

Run the tests:

```bash
cargo test -p rust-staking-contract
```

## Test Coverage

The current test suite covers:

1. Successful staking PDA initialization.
2. Rejection of zero stake amounts.
3. Rejection of unstaking more than the available stake.
4. Rejection of zero unstaking amounts.
5. Prevention of unauthorized access to another user's PDA.
6. Successful stake, reward accrual, claim, and unstake flow.
7. Prevention of duplicate reward claims.
8. Fractional reward accumulation across multiple periods.
9. Preservation of fractional reward remainders across unequal time intervals.

## Program ID

```text
BkfyfukArSRccdXmmgN3tnngt2BZqe5EB8MM2wdzECDh
```

## Security Considerations

- Every staking account is associated with its owner's public key.
- Only the authorized wallet can stake, unstake, or claim points.
- The PDA retains its rent-exempt reserve during withdrawals.
- Checked arithmetic is used for important reward and balance calculations.
- Reward points are not transferable tokens.
- The program has not been independently audited.

**Important:** This is a development project. Do not deposit funds you cannot afford to lose. Conduct thorough testing and obtain an independent security audit before production deployment.

## Future Improvements

- Build a TypeScript client SDK.
- Add a frontend for staking and unstaking.
- Implement SPL reward-token distribution.
- Add Devnet deployment instructions.
- Expand multi-user and edge-case testing.
- Add an independent security audit.

## Author

**Ravi Chauhan**

GitHub: https://github.com/Rav1Chauhan

---

If you find this project useful, consider giving the repository a ⭐.
