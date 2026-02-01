use anchor_lang::prelude::*;
use mpl_core::{
    accounts::BaseAssetV1,
    fetch_plugin,
    instructions::AddPluginV1CpiBuilder,
    types::{FreezeDelegate, Plugin, PluginAuthority, PluginType},
    ID as CORE_PROGRAM_ID,
};

use crate::{
    errors::StakeError,
    state::{StakeAccount, StakeConfig, UserAccount},
};

#[derive(Accounts)]
pub struct Stake<'info> {
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
        init,
        payer = user,
        seeds = [b"stake_account".as_ref(), asset.key().as_ref()],
        bump,
        space = StakeAccount::DISCRIMINATOR.len() + StakeAccount::INIT_SPACE,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(address = CORE_PROGRAM_ID)]
    /// CHECK: Verified by address constraint
    pub core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> Stake<'info> {
    pub fn stake(&mut self, bumps: &StakeBumps) -> Result<()> {
        // Check that user hasn't exceeded max stake
        require!(
            self.user_account.amount_staked < self.config.max_stake,
            StakeError::MaxStakeReached
        );

        // Verify the asset belongs to the user and the collection
        {
            let asset_data = Box::new(BaseAssetV1::try_from(&self.asset)?);
            require!(
                asset_data.owner == self.user.key(),
                StakeError::NotOwner
            );

            // Check that asset belongs to the collection
            match asset_data.update_authority {
                mpl_core::types::UpdateAuthority::Collection(collection_key) => {
                    require!(
                        collection_key == self.collection.key(),
                        StakeError::InvalidCollection
                    );
                }
                _ => return Err(StakeError::InvalidCollection.into()),
            }
        }

        // Check if the asset already has a freeze delegate plugin
        let freeze_delegate_result = fetch_plugin::<BaseAssetV1, FreezeDelegate>(&self.asset, PluginType::FreezeDelegate);

        // If no freeze delegate exists, add one
        if freeze_delegate_result.is_err() {
            let signer_seeds: &[&[&[u8]]] = &[&[
                b"stake_account",
                &self.asset.key().to_bytes(),
                &[bumps.stake_account],
            ]];

            AddPluginV1CpiBuilder::new(&self.core_program.to_account_info())
                .asset(&self.asset)
                .collection(Some(&self.collection))
                .payer(&self.user.to_account_info())
                .authority(Some(&self.user.to_account_info()))
                .system_program(&self.system_program.to_account_info())
                .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
                .init_authority(PluginAuthority::UpdateAuthority)
                .invoke_signed(signer_seeds)?;
        } else {
            // If freeze delegate exists but asset is not frozen, we need to freeze it
            // This would require UpdatePluginV1CpiBuilder, but for now we'll error
            return Err(StakeError::InvalidAsset.into());
        }

        // Initialize stake account
        self.stake_account.set_inner(StakeAccount {
            owner: self.user.key(),
            mint: self.asset.key(),
            staked_at: Clock::get()?.unix_timestamp,
            bump: bumps.stake_account,
        });

        // Update user account
        self.user_account.amount_staked += 1;

        Ok(())
    }
}
