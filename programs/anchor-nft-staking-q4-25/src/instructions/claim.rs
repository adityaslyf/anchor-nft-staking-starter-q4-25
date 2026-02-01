use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

use crate::state::{StakeConfig, UserAccount};

#[derive(Accounts)]
pub struct Claim<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user".as_ref(), user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        seeds = [b"config".as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        seeds = [b"rewards".as_ref(), config.key().as_ref()],
        bump = config.rewards_bump,
    )]
    pub reward_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = reward_mint,
        associated_token::authority = user,
    )]
    pub user_reward_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> Claim<'info> {
    pub fn claim(&mut self) -> Result<()> {
        // Get the user's accumulated points
        let points = self.user_account.points;

        // If no points, nothing to claim
        if points == 0 {
            return Ok(());
        }

        // Calculate reward amount (points * 10^decimals for proper token amount)
        // Since reward_mint has 6 decimals, we multiply by 1_000_000
        let reward_amount = (points as u64).checked_mul(1_000_000).unwrap();

        // Create signer seeds for the config PDA to sign the mint
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"config",
            &[self.config.bump],
        ]];

        // Mint reward tokens to the user's ATA
        let cpi_accounts = MintTo {
            mint: self.reward_mint.to_account_info(),
            to: self.user_reward_ata.to_account_info(),
            authority: self.config.to_account_info(),
        };

        let cpi_ctx = CpiContext::new_with_signer(
            self.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );

        mint_to(cpi_ctx, reward_amount)?;

        // Reset user points to 0 after claiming
        self.user_account.points = 0;

        Ok(())
    }
}
