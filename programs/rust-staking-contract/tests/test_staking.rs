use anchor_lang::{AccountDeserialize, InstructionData};
use litesvm::LiteSVM;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_system_interface::program as system_program;
use solana_transaction::Transaction;
use std::path::PathBuf;

const PROGRAM_ID: &str = "BkfyfukArSRccdXmmgN3tnngt2BZqe5EB8MM2wdzECDh";

const ONE_SOL: u64 = 1_000_000_000;
const MICRO_POINTS_PER_POINT: u64 = 1_000_000;
const SECONDS_PER_DAY: i64 = 86_400;

fn program_id() -> Pubkey {
    PROGRAM_ID.parse().unwrap()
}

fn derive_stake_pda(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"client1", user.as_ref()], &program_id())
}

// Advance the slot before each transaction to avoid reusing
// the same recent blockhash for identical transactions.
fn send_instruction(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
) -> Result<(), String> {
    // Force LiteSVM to provide a fresh blockhash.
    svm.expire_blockhash();

    let blockhash = svm.latest_blockhash();

    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[payer],
        blockhash,
    );

    svm.send_transaction(transaction)
        .map(|_| ())
        .map_err(|err| format!("{err:?}"))
}

fn instruction_data(name: &str, amount: Option<u64>) -> Vec<u8> {
    match name {
        "create_pda_account" => rust_staking_contract::instruction::CreatePdaAccount {}.data(),
        "stake" => rust_staking_contract::instruction::Stake {
            amount: amount.unwrap(),
        }
        .data(),
        "unstake" => rust_staking_contract::instruction::Unstake {
            amount: amount.unwrap(),
        }
        .data(),
        "claim_points" => rust_staking_contract::instruction::ClaimPoints {}.data(),
        "get_points" => rust_staking_contract::instruction::GetPoints {}.data(),
        _ => panic!("Unknown instruction: {name}"),
    }
}

fn setup() -> (LiteSVM, Keypair, Pubkey) {
    let mut svm = LiteSVM::new();
    let user = Keypair::new();
    let (pda, _) = derive_stake_pda(&user.pubkey());

    svm.airdrop(&user.pubkey(), 5 * ONE_SOL).unwrap();

    let program_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/rust_staking_contract.so");

    svm.add_program_from_file(program_id(), program_path)
        .expect("Build the program first with `anchor build`");

    (svm, user, pda)
}

fn initialize_instruction(user: &Keypair, pda: Pubkey) -> Instruction {
    Instruction {
        program_id: program_id(),
        accounts: vec![
            AccountMeta::new(user.pubkey(), true),
            AccountMeta::new(pda, false),
            AccountMeta::new_readonly(system_program::id(), false),
        ],
        data: instruction_data("create_pda_account", None),
    }
}

fn user_instruction(user: &Keypair, pda: Pubkey, name: &str, amount: Option<u64>) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new(user.pubkey(), true),
        AccountMeta::new(pda, false),
    ];

    if name == "stake" {
        accounts.push(AccountMeta::new_readonly(system_program::id(), false));
    }

    Instruction {
        program_id: program_id(),
        accounts,
        data: instruction_data(name, amount),
    }
}

fn read_stake_account(svm: &LiteSVM, pda: &Pubkey) -> rust_staking_contract::StakeAccount {
    let account = svm.get_account(pda).expect("Stake PDA must exist");
    let mut data: &[u8] = &account.data;

    rust_staking_contract::StakeAccount::try_deserialize(&mut data)
        .expect("Stake account data should deserialize")
}

fn read_stake_amount(svm: &LiteSVM, pda: &Pubkey) -> u64 {
    read_stake_account(svm, pda).staked_amount
}

// Test 1: PDA initializes successfully.
#[test]
fn initializes_staking_account() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    assert_eq!(read_stake_amount(&svm, &pda), 0);
}

// Test 2: Zero stake must fail.
#[test]
fn rejects_zero_stake() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let result = send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(0)),
    );

    assert!(result.is_err(), "Zero stake should be rejected");
}

// Test 3: Unstaking more than the staked amount must fail.
#[test]
fn rejects_unstaking_more_than_staked() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let result = send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "unstake", Some(ONE_SOL)),
    );

    assert!(
        result.is_err(),
        "Cannot unstake more than the staked amount"
    );
}

// Test 4: Zero unstake must fail.
#[test]
fn rejects_zero_unstake() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let result = send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "unstake", Some(0)),
    );

    assert!(result.is_err(), "Zero unstake should be rejected");
}

// Test 5: Another user must not access the original user's PDA.
#[test]
fn rejects_another_users_pda() {
    let (mut svm, user, pda) = setup();
    let attacker = Keypair::new();

    svm.airdrop(&attacker.pubkey(), ONE_SOL).unwrap();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let (attacker_pda, _) = derive_stake_pda(&attacker.pubkey());

    let result = send_instruction(
        &mut svm,
        &attacker,
        user_instruction(&attacker, pda, "unstake", Some(1)),
    );

    assert!(result.is_err(), "Unauthorized PDA access must fail");
    assert_ne!(attacker_pda, pda);
}

// Test 6: Complete stake -> accrue -> claim -> unstake flow.
#[test]
fn successful_stake_accrue_claim_and_unstake() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let balance_before = svm.get_account(&pda).unwrap().lamports;

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(ONE_SOL)),
    )
    .unwrap();

    assert_eq!(read_stake_amount(&svm, &pda), ONE_SOL);

    let balance_after_stake = svm.get_account(&pda).unwrap().lamports;
    assert_eq!(balance_after_stake - balance_before, ONE_SOL);

    // Simulate two days passing.
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += 2 * SECONDS_PER_DAY;
    clock.slot += 300_000;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_claim = read_stake_account(&svm, &pda);

    assert_eq!(
        after_claim.claimed_points,
        2 * MICRO_POINTS_PER_POINT,
        "Two days at 1 SOL should earn two points in micro-point units"
    );
    assert_eq!(after_claim.total_points, 0);
    assert_eq!(after_claim.staked_amount, ONE_SOL);

    let pda_balance_before_unstake = svm.get_account(&pda).unwrap().lamports;

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "unstake", Some(ONE_SOL)),
    )
    .unwrap();

    let after_unstake = read_stake_account(&svm, &pda);
    let pda_balance_after_unstake = svm.get_account(&pda).unwrap().lamports;

    assert_eq!(after_unstake.staked_amount, 0);
    assert_eq!(
        pda_balance_before_unstake - pda_balance_after_unstake,
        ONE_SOL
    );
    assert_eq!(after_unstake.claimed_points, after_claim.claimed_points);
}

// Test 7: Claiming twice without additional elapsed time must not
// duplicate rewards.
#[test]
fn repeated_claim_does_not_duplicate_rewards() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(ONE_SOL)),
    )
    .unwrap();

    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += 2 * SECONDS_PER_DAY;
    clock.slot += 300_000;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_first_claim = read_stake_account(&svm, &pda);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_second_claim = read_stake_account(&svm, &pda);

    assert_eq!(
        after_second_claim.claimed_points,
        after_first_claim.claimed_points
    );
    assert_eq!(
        after_second_claim.total_points,
        after_first_claim.total_points
    );
}

// Test 8: Fractional rewards must be preserved and accumulate
// across two 12-hour periods.
#[test]
fn fractional_rewards_accumulate_across_claims() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(ONE_SOL)),
    )
    .unwrap();

    // First 12 hours: expect 500,000 micro-points.
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += SECONDS_PER_DAY / 2;
    clock.slot += 300_000;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_first_claim = read_stake_account(&svm, &pda);

    assert_eq!(after_first_claim.claimed_points, 0);
    assert_eq!(after_first_claim.total_points, MICRO_POINTS_PER_POINT / 2);

    // Claim again without advancing time: rewards must remain unchanged.
    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_repeat_claim = read_stake_account(&svm, &pda);

    assert_eq!(
        after_repeat_claim.claimed_points,
        after_first_claim.claimed_points
    );
    assert_eq!(
        after_repeat_claim.total_points,
        after_first_claim.total_points
    );

    // Another 12 hours: total rewards should become one whole point.
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += SECONDS_PER_DAY / 2;
    clock.slot += 300_000;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_second_period = read_stake_account(&svm, &pda);

    assert_eq!(after_second_period.claimed_points, MICRO_POINTS_PER_POINT);
    assert_eq!(after_second_period.total_points, 0);
}

#[test]
fn fractional_remainder_is_preserved_across_unequal_intervals() {
    let (mut svm, user, pda) = setup();

    // Initialize the user's staking account.
    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    // Stake 1 SOL.
    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(ONE_SOL)),
    )
    .unwrap();

    // Advance time by exactly 1 second.
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += 1;
    clock.slot += 1;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_one_second = read_stake_account(&svm, &pda);

    // The first interval earns 11 whole micro-points,
    // with its fractional numerator preserved in reward_remainder.
    assert_eq!(after_one_second.claimed_points, 0);
    assert_eq!(after_one_second.total_points, 11);
    assert!(
        after_one_second.reward_remainder > 0,
        "Fractional reward remainder should be preserved"
    );

    // Advance another 86,399 seconds: total elapsed time is one day.
    let mut clock = svm.get_sysvar::<anchor_lang::prelude::Clock>();
    clock.unix_timestamp += SECONDS_PER_DAY - 1;
    clock.slot += 1;
    svm.set_sysvar(&clock);

    send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "claim_points", None),
    )
    .unwrap();

    let after_one_day = read_stake_account(&svm, &pda);

    assert_eq!(
        after_one_day.claimed_points, MICRO_POINTS_PER_POINT,
        "Exactly one day at 1 SOL should earn one point"
    );
    assert_eq!(after_one_day.total_points, 0);
    assert_eq!(
        after_one_day.reward_remainder, 0,
        "The fractional remainder should resolve exactly after one day"
    );
}
