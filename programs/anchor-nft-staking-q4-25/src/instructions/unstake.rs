use anchor_lang::prelude::*;
use mpl_core::{
    accounts::BaseAssetV1,
    instructions::{RemovePluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{FreezeDelegate, Plugin, PluginType},
    ID as CORE_PROGRAM_ID,
};

use crate::{
    errors::StakeError,
    state::{StakeAccount, StakeConfig, UserAccount},
};

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Verified through CPI to mpl-core
    #[account(mut)]
    pub asset: AccountInfo<'info>,

    /// CHECK: Verified by constraint checking owner is CORE_PROGRAM_ID
    #[account(
        constraint = collection.owner == &CORE_PROGRAM_ID @ StakeError::InvalidCollection,
    )]
    pub collection: AccountInfo<'info>,

    #[account(
        seeds = [b"config".as_ref()],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        seeds = [b"user".as_ref(), user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        mut,
        close = user,
        seeds = [b"stake_account".as_ref(), asset.key().as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == user.key() @ StakeError::NotOwner,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(address = CORE_PROGRAM_ID)]
    /// CHECK: Verified by address constraint
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Unstake<'info> {
    pub fn unstake(&mut self) -> Result<()> {
        // Verify the asset belongs to the user
        {
            let asset_data = Box::new(BaseAssetV1::try_from(&self.asset)?);
            require!(
                asset_data.owner == self.user.key(),
                StakeError::NotOwner
            );
        }

        // Check that freeze period has passed
        let current_time = Clock::get()?.unix_timestamp;
        let staked_at = self.stake_account.staked_at;
        let freeze_period = self.config.freeze_period as i64;

        require!(
            current_time >= staked_at + freeze_period,
            StakeError::FreezePeriodNotPassed
        );

        // Calculate points earned
        let time_staked = current_time - staked_at;
        let points_earned = (time_staked as u32)
            .checked_mul(self.config.points_per_stake as u32)
            .ok_or(StakeError::InvalidAsset)?;

        // Update user points
        self.user_account.points = self.user_account.points
            .checked_add(points_earned)
            .ok_or(StakeError::InvalidAsset)?;

        // Decrement staked amount
        self.user_account.amount_staked = self.user_account.amount_staked
            .checked_sub(1)
            .ok_or(StakeError::InvalidAsset)?;

        // Unfreeze the asset first
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"stake_account",
            &self.asset.key().to_bytes(),
            &[self.stake_account.bump],
        ]];

        UpdatePluginV1CpiBuilder::new(&self.core_program.to_account_info())
            .asset(&self.asset)
            .collection(Some(&self.collection))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.collection))
            .system_program(&self.system_program.to_account_info())
            .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
            .invoke_signed(signer_seeds)?;

        // Remove the freeze delegate plugin
        RemovePluginV1CpiBuilder::new(&self.core_program.to_account_info())
            .asset(&self.asset)
            .collection(Some(&self.collection))
            .payer(&self.user.to_account_info())
            .authority(Some(&self.collection))
            .system_program(&self.system_program.to_account_info())
            .plugin_type(PluginType::FreezeDelegate)
            .invoke_signed(signer_seeds)?;

        // stake_account is automatically closed via the `close = user` constraint

        Ok(())
    }
}
