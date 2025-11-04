use anchor_lang::prelude::*;
use anchor_lang::prelude::InterfaceAccount;
use anchor_spl::{
    associated_token::AssociatedToken,
    // Нужен только ID классической программы для проверки владельца WSOL
    token,
    token_2022,
    token_interface::{self, Mint, TokenAccount, TokenInterface},
};

declare_id!("G84pgVuVT3hTeAvxAfKPJkgPnSk4Es7w8bdoqKAMYD1Z");

// Цена: 1 TOKEN = 0.5 WSOL -> NUM/DEN = 1/2
const PRICE_NUM_WS_PER_TOKEN: u64 = 1;
const PRICE_DEN_WS_PER_TOKEN: u64 = 2;

#[program]
pub mod dex_fixed_mixed {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        amount_token: u64,
        amount_wsol: u64,
    ) -> Result<()> {
        // Жёстко требуем: ваш токен — под Token-2022, а WSOL — под классическим SPL
        require_keys_eq!(
            *ctx.accounts.mint_token.to_account_info().owner,
            token_2022::ID,
            DexError::WrongProgramToken
        );
        require_keys_eq!(
            *ctx.accounts.mint_wsol.to_account_info().owner,
            token::ID,
            DexError::WrongProgramWsol
        );

        // Для простоты требуем равные decimals
        require!(
            ctx.accounts.mint_token.decimals == ctx.accounts.mint_wsol.decimals,
            DexError::DecimalsMismatch
        );

        // Списываем стартовую ликвидность из пользовательских ATA в хранилища пула
        xfer_checked(
            &ctx.accounts.token_program_token.to_account_info(),
            &ctx.accounts.user_token.to_account_info(),
            &ctx.accounts.vault_token.to_account_info(),
            &ctx.accounts.mint_token.to_account_info(),
            &ctx.accounts.signer.to_account_info(),
            None,
            amount_token,
            ctx.accounts.mint_token.decimals,
        )?;

        xfer_checked(
            &ctx.accounts.token_program_wsol.to_account_info(),
            &ctx.accounts.user_wsol.to_account_info(),
            &ctx.accounts.vault_wsol.to_account_info(),
            &ctx.accounts.mint_wsol.to_account_info(),
            &ctx.accounts.signer.to_account_info(),
            None,
            amount_wsol,
            ctx.accounts.mint_wsol.decimals,
        )?;

        let pool = &mut ctx.accounts.pool;
        pool.admin = ctx.accounts.signer.key();
        pool.mint_token = ctx.accounts.mint_token.key();
        pool.mint_wsol = ctx.accounts.mint_wsol.key();
        pool.vault_token = ctx.accounts.vault_token.key();
        pool.vault_wsol = ctx.accounts.vault_wsol.key();
        pool.vault_bump = ctx.bumps.vault_authority;
        pool.decimals = ctx.accounts.mint_token.decimals;
        pool.reserve_token = amount_token;
        pool.reserve_wsol = amount_wsol;

        Ok(())
    }

    // Пользователь шлёт WSOL (classic) — получает ваш токен (Token-2022)
    pub fn buy(ctx: Context<Buy>, amount_in_wsol: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        // user WSOL -> vault WSOL (classic)
        xfer_checked(
            &ctx.accounts.token_program_wsol.to_account_info(),
            &ctx.accounts.user_wsol.to_account_info(),
            &ctx.accounts.vault_wsol.to_account_info(),
            &ctx.accounts.mint_wsol.to_account_info(),
            &ctx.accounts.user.to_account_info(),
            None,
            amount_in_wsol,
            pool.decimals,
        )?;

        // tokens_out = wsol_in * DEN / NUM
        let tokens_out = mul_div_u64(amount_in_wsol, PRICE_DEN_WS_PER_TOKEN, PRICE_NUM_WS_PER_TOKEN)?;
        require!(tokens_out > 0, DexError::ZeroOutput);
        require!(pool.reserve_token >= tokens_out, DexError::InsufficientLiquidity);

        // vault TOKEN-2022 -> user TOKEN-2022 (подписывает PDA)
        let pool_key = pool.key();
        let seeds = [b"vault", pool_key.as_ref(), &[pool.vault_bump]];
        let signer_seeds: &[&[&[u8]]] = &[&seeds];

        xfer_checked(
            &ctx.accounts.token_program_token.to_account_info(),
            &ctx.accounts.vault_token.to_account_info(),
            &ctx.accounts.user_token.to_account_info(),
            &ctx.accounts.mint_token.to_account_info(),
            &ctx.accounts.vault_authority.to_account_info(),
            Some(signer_seeds),
            tokens_out,
            pool.decimals,
        )?;

        pool.reserve_wsol = pool.reserve_wsol.checked_add(amount_in_wsol).ok_or(DexError::MathOverflow)?;
        pool.reserve_token = pool.reserve_token.checked_sub(tokens_out).ok_or(DexError::MathOverflow)?;
        Ok(())
    }

    // Пользователь шлёт ваш токен (Token-2022) — получает WSOL (classic)
    pub fn sell(ctx: Context<Sell>, amount_in_token: u64) -> Result<()> {
        let pool = &mut ctx.accounts.pool;

        // user TOKEN-2022 -> vault TOKEN-2022
        xfer_checked(
            &ctx.accounts.token_program_token.to_account_info(),
            &ctx.accounts.user_token.to_account_info(),
            &ctx.accounts.vault_token.to_account_info(),
            &ctx.accounts.mint_token.to_account_info(),
            &ctx.accounts.user.to_account_info(),
            None,
            amount_in_token,
            pool.decimals,
        )?;

        // wsol_out = token_in * NUM / DEN
        let wsol_out = mul_div_u64(amount_in_token, PRICE_NUM_WS_PER_TOKEN, PRICE_DEN_WS_PER_TOKEN)?;
        require!(wsol_out > 0, DexError::ZeroOutput);
        require!(pool.reserve_wsol >= wsol_out, DexError::InsufficientLiquidity);

        // vault WSOL (classic) -> user WSOL (подписывает PDA)
        let pool_key = pool.key();
        let seeds = [b"vault", pool_key.as_ref(), &[pool.vault_bump]];
        let signer_seeds: &[&[&[u8]]] = &[&seeds];

        xfer_checked(
            &ctx.accounts.token_program_wsol.to_account_info(),
            &ctx.accounts.vault_wsol.to_account_info(),
            &ctx.accounts.user_wsol.to_account_info(),
            &ctx.accounts.mint_wsol.to_account_info(),
            &ctx.accounts.vault_authority.to_account_info(),
            Some(signer_seeds),
            wsol_out,
            pool.decimals,
        )?;

        pool.reserve_token = pool.reserve_token.checked_add(amount_in_token).ok_or(DexError::MathOverflow)?;
        pool.reserve_wsol = pool.reserve_wsol.checked_sub(wsol_out).ok_or(DexError::MathOverflow)?;
        Ok(())
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn mul_div_u64(a: u64, num: u64, den: u64) -> Result<u64> {
    require!(den != 0, DexError::DivisionByZero);
    let v = (a as u128)
        .checked_mul(num as u128)
        .ok_or(DexError::MathOverflow)?
        .checked_div(den as u128)
        .ok_or(DexError::DivisionByZero)?;
    u64::try_from(v).map_err(|_| DexError::MathOverflow.into())
}

// Универсальный transfer_checked через token_interface – работает и с Token-2022, и с классикой
fn xfer_checked<'info>(
    token_program: &AccountInfo<'info>,
    from: &AccountInfo<'info>,
    to: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    signer_seeds: Option<&[&[&[u8]]]>,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let accs = token_interface::TransferChecked {
        from: from.clone(),
        to: to.clone(),
        mint: mint.clone(),
        authority: authority.clone(),
    };
    let cpi = match signer_seeds {
        Some(seeds) => CpiContext::new_with_signer(token_program.clone(), accs, seeds),
        None => CpiContext::new(token_program.clone(), accs),
    };
    token_interface::transfer_checked(cpi, amount, decimals)
}

// ── state ─────────────────────────────────────────────────────────────────────

#[account]
pub struct Pool {
    pub admin: Pubkey,
    pub mint_token: Pubkey,    // ваш токен под Token-2022
    pub mint_wsol: Pubkey,     // WSOL под классическим SPL
    pub vault_token: Pubkey,
    pub vault_wsol: Pubkey,
    pub vault_bump: u8,
    pub decimals: u8,
    pub reserve_token: u64,
    pub reserve_wsol: u64,
}
impl Pool {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 32 + 1 + 1 + 8 + 8;
}

// ── accounts ──────────────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(init, payer = signer, space = 8 + Pool::LEN)]
    pub pool: Account<'info, Pool>,

    /// CHECK: PDA – владелец хранилищ пула
    #[account(seeds = [b"vault", pool.key().as_ref()], bump)]
    pub vault_authority: UncheckedAccount<'info>,

    // Хранилище токена (Token-2022) под PDA
    #[account(
        init,
        payer = signer,
        associated_token::mint = mint_token,
        associated_token::authority = vault_authority,
        associated_token::token_program = token_program_token
    )]
    pub vault_token: InterfaceAccount<'info, TokenAccount>,

    // Хранилище WSOL (classic) под PDA
    #[account(
        init,
        payer = signer,
        associated_token::mint = mint_wsol,
        associated_token::authority = vault_authority,
        associated_token::token_program = token_program_wsol
    )]
    pub vault_wsol: InterfaceAccount<'info, TokenAccount>,

    // Пользовательские ATA
    #[account(mut)]
    pub user_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub user_wsol: InterfaceAccount<'info, TokenAccount>,

    // Минты
    pub mint_token: InterfaceAccount<'info, Mint>,
    pub mint_wsol: InterfaceAccount<'info, Mint>,

    // Две программы токенов
    pub token_program_token: Interface<'info, TokenInterface>, // должен указывать на Token-2022
    pub token_program_wsol:  Interface<'info, TokenInterface>, // должен указывать на классический SPL

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, has_one = mint_token, has_one = mint_wsol)]
    pub pool: Account<'info, Pool>,

    /// CHECK: PDA подписант
    #[account(seeds = [b"vault", pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub vault_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub vault_wsol: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub user_wsol: InterfaceAccount<'info, TokenAccount>,

    pub mint_token: InterfaceAccount<'info, Mint>,
    pub mint_wsol: InterfaceAccount<'info, Mint>,

    pub token_program_token: Interface<'info, TokenInterface>,
    pub token_program_wsol:  Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct Sell<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut, has_one = mint_token, has_one = mint_wsol)]
    pub pool: Account<'info, Pool>,

    /// CHECK: PDA подписант
    #[account(seeds = [b"vault", pool.key().as_ref()], bump = pool.vault_bump)]
    pub vault_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub vault_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub vault_wsol: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub user_token: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub user_wsol: InterfaceAccount<'info, TokenAccount>,

    pub mint_token: InterfaceAccount<'info, Mint>,
    pub mint_wsol: InterfaceAccount<'info, Mint>,

    pub token_program_token: Interface<'info, TokenInterface>,
    pub token_program_wsol:  Interface<'info, TokenInterface>,
}

// ── errors ────────────────────────────────────────────────────────────────────

#[error_code]
pub enum DexError {
    #[msg("Division by zero")]
    DivisionByZero,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Zero output")]
    ZeroOutput,
    #[msg("Pool has not enough liquidity")]
    InsufficientLiquidity,
    #[msg("Decimals of both mints must match")]
    DecimalsMismatch,
    #[msg("Your TOKEN mint must be under Token-2022")]
    WrongProgramToken,
    #[msg("WSOL mint must be under classic SPL Token")]
    WrongProgramWsol,
}
