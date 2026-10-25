use anchor_lang::prelude::*;
use anchor_lang::system_program;

declare_id!("BkfyfukArSRccdXmmgN3tnngt2BZqe5EB8MM2wdzECDh");

const POINTS_PER_SOL_PER_DAY: u64 = 1_000_000;
const MICRO_POINTS_PER_POINT: u64 = 1_000_000;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

#[program]
pub mod sol_staking {
    use super::*;

    pub fn create_pda_account(ctx: Context<CreatePdaAccount>) -> Result<()> {
        let clock = Clock::get()?;
        let pda_account = &mut ctx.accounts.pda_account;

        pda_account.owner = ctx.accounts.payer.key();
        pda_account.staked_amount = 0;
        pda_account.total_points = 0;
        pda_account.claimed_points = 0;
        pda_account.last_update_time = clock.unix_timestamp;
        pda_account.bump = ctx.bumps.pda_account;

        msg!("Staking account initialized");
        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        require!(amount > 0, StakeError::InvalidAmount);

        let clock = Clock::get()?;

        // Get account infos before borrowing the account mutably.
        let user_info = ctx.accounts.user.to_account_info();
        let pda_info = ctx.accounts.pda_account.to_account_info();
        let system_program_id = ctx.accounts.system_program.key();

        let pda_account = &mut ctx.accounts.pda_account;

        // Accrue points using the previous stake.
        update_points(pda_account, clock.unix_timestamp)?;

        let new_staked_amount = pda_account
            .staked_amount
            .checked_add(amount)
            .ok_or(StakeError::Overflow)?;

        // Transfer SOL from the user to the PDA exactly once.
        let cpi_context = CpiContext::new(
            system_program_id,
            system_program::Transfer {
                from: user_info,
                to: pda_info,
            },
        );

        system_program::transfer(cpi_context, amount)?;

        pda_account.staked_amount = new_staked_amount;

        msg!("Staked {} lamports", amount);
        msg!("Total staked: {} lamports", new_staked_amount);

        Ok(())
    }

    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        require!(amount > 0, StakeError::InvalidAmount);

        let clock = Clock::get()?;
        let user_key = ctx.accounts.user.key();

        let pda_account = &mut ctx.accounts.pda_account;

        require!(
            pda_account.staked_amount >= amount,
            StakeError::InsufficientStake
        );

        // Accrue points using the old stake before reducing it.
        update_points(pda_account, clock.unix_timestamp)?;

        let new_staked_amount = pda_account
            .staked_amount
            .checked_sub(amount)
            .ok_or(StakeError::Underflow)?;

        let pda_info = pda_account.to_account_info();
        let user_info = ctx.accounts.user.to_account_info();

        // Keep the account rent-exempt after withdrawal.
        let rent_minimum = Rent::get()?.minimum_balance(pda_info.data_len());

        let pda_balance = pda_info.lamports();
        let remaining_balance = pda_balance
            .checked_sub(amount)
            .ok_or(StakeError::InsufficientPdaBalance)?;

        require!(
            remaining_balance >= rent_minimum,
            StakeError::InsufficientPdaBalance
        );

        let new_user_balance = user_info
            .lamports()
            .checked_add(amount)
            .ok_or(StakeError::Overflow)?;

        // The PDA is owned by this program. Debit its lamports directly;
        // a System Program transfer cannot sign for this program-owned PDA.
        **pda_info.try_borrow_mut_lamports()? = remaining_balance;
        **user_info.try_borrow_mut_lamports()? = new_user_balance;

        pda_account.staked_amount = new_staked_amount;

        msg!("Unstaked {} lamports for {}", amount, user_key);
        msg!("Remaining stake: {} lamports", new_staked_amount);

        Ok(())
    }

    pub fn claim_points(ctx: Context<ClaimPoints>) -> Result<()> {
        let clock = Clock::get()?;
        let pda_account = &mut ctx.accounts.pda_account;

        update_points(pda_account, clock.unix_timestamp)?;

        // Only whole points are claimable. Preserve fractional micro-points.
        let claimable_points = pda_account.total_points / MICRO_POINTS_PER_POINT;

        let claimed_micro_points = claimable_points
            .checked_mul(MICRO_POINTS_PER_POINT)
            .ok_or(StakeError::Overflow)?;

        pda_account.total_points = pda_account
            .total_points
            .checked_sub(claimed_micro_points)
            .ok_or(StakeError::Underflow)?;

        pda_account.claimed_points = pda_account
            .claimed_points
            .checked_add(claimed_micro_points)
            .ok_or(StakeError::Overflow)?;

        msg!("Claimed points: {}", claimable_points);
        msg!(
            "Remaining unclaimed micro-points: {}",
            pda_account.total_points
        );
        msg!(
            "Lifetime claimed whole points: {}",
            pda_account.claimed_points / MICRO_POINTS_PER_POINT
        );

        Ok(())
    }

    pub fn get_points(ctx: Context<GetPoints>) -> Result<()> {
        let pda_account = &ctx.accounts.pda_account;
        let clock = Clock::get()?;

        let elapsed = elapsed_seconds(pda_account.last_update_time, clock.unix_timestamp)?;

        let newly_earned = calculate_points_earned(pda_account.staked_amount, elapsed)?;

        let current_total = pda_account
            .total_points
            .checked_add(newly_earned)
            .ok_or(StakeError::Overflow)?;

        msg!(
            "Unclaimed whole points: {}",
            current_total / MICRO_POINTS_PER_POINT
        );
        msg!("Unclaimed micro-points: {}", current_total);
        msg!("Staked amount: {} lamports", pda_account.staked_amount);
        msg!(
            "Lifetime claimed points: {}",
            pda_account.claimed_points / MICRO_POINTS_PER_POINT
        );

        Ok(())
    }
}

fn elapsed_seconds(last_update: i64, current_time: i64) -> Result<u64> {
    let elapsed = current_time
        .checked_sub(last_update)
        .ok_or(StakeError::InvalidTimestamp)?;

    u64::try_from(elapsed).map_err(|_| error!(StakeError::InvalidTimestamp))
}

fn update_points(pda_account: &mut StakeAccount, current_time: i64) -> Result<()> {
    let elapsed = elapsed_seconds(pda_account.last_update_time, current_time)?;

    if elapsed > 0 && pda_account.staked_amount > 0 {
        let earned = calculate_points_earned(pda_account.staked_amount, elapsed)?;

        pda_account.total_points = pda_account
            .total_points
            .checked_add(earned)
            .ok_or(StakeError::Overflow)?;
    }

    pda_account.last_update_time = current_time;
    Ok(())
}

fn calculate_points_earned(staked_amount: u64, elapsed_seconds: u64) -> Result<u64> {
    let numerator = (staked_amount as u128)
        .checked_mul(elapsed_seconds as u128)
        .and_then(|v| v.checked_mul(POINTS_PER_SOL_PER_DAY as u128))
        .ok_or(StakeError::Overflow)?;

    let denominator = (LAMPORTS_PER_SOL as u128)
        .checked_mul(SECONDS_PER_DAY as u128)
        .ok_or(StakeError::Overflow)?;

    let points = numerator
        .checked_div(denominator)
        .ok_or(StakeError::Overflow)?;

    u64::try_from(points).map_err(|_| error!(StakeError::Overflow))
}

#[derive(Accounts)]
pub struct CreatePdaAccount<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + 32 + 8 + 8 + 8 + 8 + 1,
        seeds = [b"client1", payer.key().as_ref()],
        bump
    )]
    pub pda_account: Account<'info, StakeAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"client1", user.key().as_ref()],
        bump = pda_account.bump,
        constraint = pda_account.owner == user.key()
            @ StakeError::Unauthorized
    )]
    pub pda_account: Account<'info, StakeAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"client1", user.key().as_ref()],
        bump = pda_account.bump,
        constraint = pda_account.owner == user.key()
            @ StakeError::Unauthorized
    )]
    pub pda_account: Account<'info, StakeAccount>,
}

#[derive(Accounts)]
pub struct ClaimPoints<'info> {
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"client1", user.key().as_ref()],
        bump = pda_account.bump,
        constraint = pda_account.owner == user.key()
            @ StakeError::Unauthorized
    )]
    pub pda_account: Account<'info, StakeAccount>,
}

#[derive(Accounts)]
pub struct GetPoints<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [b"client1", user.key().as_ref()],
        bump = pda_account.bump,
        constraint = pda_account.owner == user.key()
            @ StakeError::Unauthorized
    )]
    pub pda_account: Account<'info, StakeAccount>,
}

#[account]
pub struct StakeAccount {
    pub owner: Pubkey,
    pub staked_amount: u64,
    pub total_points: u64,
    pub claimed_points: u64,
    pub last_update_time: i64,
    pub bump: u8,
}

#[error_code]
pub enum StakeError {
    #[msg("Amount must be greater than zero")]
    InvalidAmount,

    #[msg("Insufficient staked amount")]
    InsufficientStake,

    #[msg("Unauthorized access")]
    Unauthorized,

    #[msg("Arithmetic overflow")]
    Overflow,

    #[msg("Arithmetic underflow")]
    Underflow,

    #[msg("Invalid or negative elapsed timestamp")]
    InvalidTimestamp,

    #[msg("Insufficient PDA balance or rent reserve")]
    InsufficientPdaBalance,
}
