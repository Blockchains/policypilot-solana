use anchor_lang::prelude::*;

declare_id!("EkmWFbZoAdPgFfcDYcpxsd5f31SyRDFbErNz9H39sqqN");

pub const MAX_RECIPIENTS: usize = 8;

/// PolicyPilot (Solana): bounded lamport payouts from a program-owned config PDA.
/// Replay prevention: each request_id creates a Request PDA via `init`, which fails if it exists.
#[program]
pub mod pp {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, executor: Pubkey, per_request_limit: u64, budget: u64) -> Result<()> {
        let c = &mut ctx.accounts.config;
        c.admin = ctx.accounts.admin.key();
        c.executor = executor;
        c.per_request_limit = per_request_limit;
        c.budget = budget;
        c.total_spent = 0;
        c.paused = false;
        c.recipients = vec![];
        c.bump = ctx.bumps.config;
        Ok(())
    }

    pub fn set_recipient(ctx: Context<Admin>, recipient: Pubkey, allowed: bool) -> Result<()> {
        let c = &mut ctx.accounts.config;
        let pos = c.recipients.iter().position(|r| *r == recipient);
        match (allowed, pos) {
            (true, None) => { require!(c.recipients.len() < MAX_RECIPIENTS, PpError::TooManyRecipients); c.recipients.push(recipient); }
            (false, Some(i)) => { c.recipients.swap_remove(i); }
            _ => {}
        }
        Ok(())
    }

    pub fn set_paused(ctx: Context<Admin>, paused: bool) -> Result<()> { ctx.accounts.config.paused = paused; Ok(()) }
    pub fn set_limits(ctx: Context<Admin>, per_request_limit: u64, budget: u64) -> Result<()> {
        let c = &mut ctx.accounts.config; c.per_request_limit = per_request_limit; c.budget = budget; Ok(())
    }

    pub fn execute(ctx: Context<Execute>, request_id: [u8; 32], amount: u64) -> Result<()> {
        let c = &mut ctx.accounts.config;
        require_keys_eq!(ctx.accounts.executor.key(), c.executor, PpError::NotExecutor);
        require!(!c.paused, PpError::Paused);
        require!(amount > 0, PpError::ZeroAmount);
        let to = ctx.accounts.recipient.key();
        require!(c.recipients.contains(&to), PpError::RecipientNotAllowed);
        require!(amount <= c.per_request_limit, PpError::PerRequestLimitExceeded);
        let remaining = c.budget.saturating_sub(c.total_spent);
        require!(amount <= remaining, PpError::BudgetExceeded);
        // effects
        c.total_spent = c.total_spent.checked_add(amount).ok_or(PpError::Overflow)?;
        let r = &mut ctx.accounts.request;
        r.request_id = request_id; r.recipient = to; r.amount = amount; r.slot = Clock::get()?.slot;
        // interaction: debit program-owned PDA (keeps rent-exempt minimum)
        let cfg_info = c.to_account_info();
        let min = Rent::get()?.minimum_balance(cfg_info.data_len());
        require!(cfg_info.lamports().saturating_sub(min) >= amount, PpError::InsufficientFunds);
        **cfg_info.try_borrow_mut_lamports()? -= amount;
        **ctx.accounts.recipient.to_account_info().try_borrow_mut_lamports()? += amount;
        emit!(Executed { request_id, recipient: to, amount, total_spent: c.total_spent });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey, pub executor: Pubkey,
    pub per_request_limit: u64, pub budget: u64, pub total_spent: u64,
    pub paused: bool, pub bump: u8,
    #[max_len(8)] pub recipients: Vec<Pubkey>,
}

#[account]
#[derive(InitSpace)]
pub struct Request { pub request_id: [u8; 32], pub recipient: Pubkey, pub amount: u64, pub slot: u64 }

#[event]
pub struct Executed { pub request_id: [u8; 32], pub recipient: Pubkey, pub amount: u64, pub total_spent: u64 }

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + Config::INIT_SPACE, seeds = [b"config", admin.key().as_ref()], bump)]
    pub config: Account<'info, Config>,
    #[account(mut)] pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Admin<'info> {
    #[account(mut, has_one = admin @ PpError::NotAdmin)] pub config: Account<'info, Config>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(request_id: [u8; 32])]
pub struct Execute<'info> {
    #[account(mut)] pub config: Account<'info, Config>,
    #[account(init, payer = executor, space = 8 + Request::INIT_SPACE, seeds = [b"request", config.key().as_ref(), request_id.as_ref()], bump)]
    pub request: Account<'info, Request>,
    #[account(mut)] pub executor: Signer<'info>,
    /// CHECK: checked against config.recipients allowlist
    #[account(mut)] pub recipient: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[error_code]
pub enum PpError {
    NotAdmin, NotExecutor, Paused, ZeroAmount, RecipientNotAllowed,
    PerRequestLimitExceeded, BudgetExceeded, TooManyRecipients, Overflow, InsufficientFunds,
}
