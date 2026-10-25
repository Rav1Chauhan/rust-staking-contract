use anchor_lang::{AccountDeserialize, InstructionData};

use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;
use solana_system_interface::program as system_program;

use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

// Replace this with the actual compiled program ID if it differs.
const PROGRAM_ID: &str = "BkfyfukArSRccdXmmgN3tnngt2BZqe5EB8MM2wdzECDh";

fn program_id() -> Pubkey {
    PROGRAM_ID.parse().unwrap()
}

fn derive_stake_pda(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"client1", user.as_ref()], &program_id())
}

fn send_instruction(
    svm: &mut LiteSVM,
    payer: &Keypair,
    instruction: Instruction,
) -> Result<(), String> {
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
    let mut data = match name {
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
        _ => panic!("Unknown instruction"),
    };

    // Anchor-generated instruction structs already include the discriminator.
    // Keep the vector unchanged.
    data.shrink_to_fit();
    data
}

fn setup() -> (LiteSVM, Keypair, Pubkey) {
    let mut svm = LiteSVM::new();
    let user = Keypair::new();
    let (pda, _) = derive_stake_pda(&user.pubkey());

    svm.airdrop(&user.pubkey(), 5_000_000_000).unwrap();

    svm.add_program_from_file(
        program_id(),
        "/home/ravichauhan/solana/rust-staking-contract/target/deploy/rust_staking_contract.so",
    )
    .unwrap();

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

fn read_stake_amount(svm: &LiteSVM, pda: &Pubkey) -> u64 {
    let account = svm.get_account(pda).unwrap();
    let mut data: &[u8] = &account.data;

    let stake = rust_staking_contract::StakeAccount::try_deserialize(&mut data).unwrap();

    stake.staked_amount
}

#[test]
fn initializes_staking_account() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    assert_eq!(read_stake_amount(&svm, &pda), 0);
}

#[test]
fn rejects_zero_stake() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let result = send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "stake", Some(0)),
    );

    assert!(result.is_err());
}

#[test]
fn rejects_unstaking_more_than_staked() {
    let (mut svm, user, pda) = setup();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let result = send_instruction(
        &mut svm,
        &user,
        user_instruction(&user, pda, "unstake", Some(1_000_000_000)),
    );

    assert!(result.is_err());
}

#[test]
fn rejects_another_users_pda() {
    let (mut svm, user, pda) = setup();
    let attacker = Keypair::new();

    svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();

    send_instruction(&mut svm, &user, initialize_instruction(&user, pda)).unwrap();

    let (attacker_pda, _) = derive_stake_pda(&attacker.pubkey());

    let result = send_instruction(
        &mut svm,
        &attacker,
        user_instruction(&attacker, pda, "unstake", Some(1)),
    );

    assert!(result.is_err());
    assert_ne!(attacker_pda, pda);
}
