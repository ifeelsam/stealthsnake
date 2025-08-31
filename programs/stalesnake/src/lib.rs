use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use arcium_anchor::prelude::*;

const COMP_DEF_OFFSET_BATTLE: u32 = comp_def_offset("execute_battle");

declare_id!("DUrqKd64caWsHCL132QaHMUsK3MPHrnQwZtWJo4UFLSm");

#[arcium_program]
pub mod stale_snake {
    use arcium_client::idl::arcium::types::CallbackAccount;

    use super::*;

    pub fn init_battle_comp_def(ctx: Context<InitBattleCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, true, 0, None, None)?;
        Ok(())
    }

    /// Step 1: Player stakes assets and creates/joins a duel
    pub fn stake_and_join(
        ctx: Context<StakeAndJoin>,
        duel_id: u64,
        stake_amount: u64,
        encrypted_stats: EncryptedStats,
    ) -> Result<()> {
        let duel = &mut ctx.accounts.duel_order;

        if duel.player1 == Pubkey::default() {
            // First player creates the duel
            duel.duel_id = duel_id;
            duel.player1 = ctx.accounts.player.key();
            duel.player1_stats = encrypted_stats;
            duel.player1_mint = ctx.accounts.token_mint.key();
            duel.stake_amount = stake_amount;
            duel.status = DuelStatus::WaitingForOpponent;

            msg!("Player 1 joined. Waiting for opponent...");
        } else {
            // Second player joins
            require!(
                duel.status == DuelStatus::WaitingForOpponent,
                ErrorCode::DuelNotOpen
            );

            duel.player2 = ctx.accounts.player.key();
            duel.player2_stats = encrypted_stats;
            duel.player2_mint = ctx.accounts.token_mint.key();
            duel.status = DuelStatus::ReadyToBattle;

            msg!("Player 2 joined. Battle ready!");
        }

        // Transfer stake to vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, stake_amount)?;

        emit!(PlayerJoinedEvent {
            duel_id,
            player: ctx.accounts.player.key(),
            stake: stake_amount,
        });

        Ok(())
    }

    /// Step 2: Execute the battle through MPC
    pub fn start_battle(
        ctx: Context<StartBattle>,
        computation_offset: u64,
        player1_pubkey: [u8; 32],
        player1_nonce: u128,
        player2_pubkey: [u8; 32],
        player2_nonce: u128,
    ) -> Result<()> {
        let duel = &ctx.accounts.duel_order;

        require!(
            duel.status == DuelStatus::ReadyToBattle,
            ErrorCode::BattleNotReady
        );

        // Prepare encrypted data for MPC
        let args = vec![
            // Player 1 encrypted stats
            Argument::ArcisPubkey(player1_pubkey),
            Argument::PlaintextU128(player1_nonce),
            Argument::EncryptedU16(duel.player1_stats.attack),
            Argument::EncryptedU16(duel.player1_stats.defense),
            Argument::EncryptedU16(duel.player1_stats.speed),
            // Player 2 encrypted stats
            Argument::ArcisPubkey(player2_pubkey),
            Argument::PlaintextU128(player2_nonce),
            Argument::EncryptedU16(duel.player2_stats.attack),
            Argument::EncryptedU16(duel.player2_stats.defense),
            Argument::EncryptedU16(duel.player2_stats.speed),
        ];

        queue_computation(
            ctx.accounts,
            computation_offset,
            args,
            vec![CallbackAccount {
                pubkey: ctx.accounts.duel_order.key(),
                is_writable: true,
            }],
            None,
        )?;

        let duel = &mut ctx.accounts.duel_order;
        duel.status = DuelStatus::BattleInProgress;

        Ok(())
    }

    /// Step 3: Handle battle result from MPC
    #[arcium_callback(encrypted_ix = "execute_battle")]
    pub fn execute_battle_callback(
        ctx: Context<BattleResultCallback>,
        output: ComputationOutputs<ExecuteBattleOutput>,
    ) -> Result<()> {
        let result = match output {
            ComputationOutputs::Success(ExecuteBattleOutput { field_0 }) => field_0,
            _ => return Err(ErrorCode::BattleFailed.into()),
        };

        let duel = &mut ctx.accounts.duel_order;

        match result {
            1 => {
                duel.winner = duel.player1;
                msg!("Player 1 wins!");
            }
            2 => {
                duel.winner = duel.player2;
                msg!("Player 2 wins!");
            }
            _ => {
                duel.winner = Pubkey::default();
                msg!("Draw!");
            }
        }

        duel.status = DuelStatus::Completed;

        emit!(BattleCompletedEvent {
            duel_id: duel.duel_id,
            winner: duel.winner,
        });

        Ok(())
    }

    /// Step 4: Release assets from vault
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        let duel = &ctx.accounts.duel_order;

        require!(
            duel.status == DuelStatus::Completed,
            ErrorCode::BattleNotCompleted
        );

        let is_player1 = ctx.accounts.winner.key() == duel.player1;
        let is_player2 = ctx.accounts.winner.key() == duel.player2;

        require!(is_player1 || is_player2, ErrorCode::NotAParticipant);

        let amount = if duel.winner == ctx.accounts.winner.key() {
            // Winner gets both stakes
            duel.stake_amount * 2
        } else if duel.winner == Pubkey::default() {
            // Draw - return original stake
            duel.stake_amount
        } else {
            // Loser gets nothing
            0
        };

        if amount > 0 {
            let duel_order_key = ctx.accounts.duel_order.key();
            let player1_mint = ctx.accounts.duel_order.player1_mint;
            let player2_mint = ctx.accounts.duel_order.player2_mint;

            let player1_vault_seeds = &[
                b"vault",
                duel_order_key.as_ref(),
                player1_mint.as_ref(),
                &[ctx.bumps.player1_vault],
            ];
            let player1_vault_signer = &[&player1_vault_seeds[..]];

            let cpi_accounts_p1 = Transfer {
                from: ctx.accounts.player1_vault.to_account_info(),
                to: ctx.accounts.winner_token_account.to_account_info(),
                authority: ctx.accounts.player1_vault.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx =
                CpiContext::new_with_signer(cpi_program, cpi_accounts_p1, player1_vault_signer);
            token::transfer(cpi_ctx, ctx.accounts.player1_vault.amount)?;

            let player2_vault_seeds = &[
                b"vault",
                duel_order_key.as_ref(),
                player2_mint.as_ref(),
                &[ctx.bumps.player2_vault],
            ];
            let player2_vault_signer = &[&player2_vault_seeds[..]];

            let cpi_accounts_p2 = Transfer {
                from: ctx.accounts.player2_vault.to_account_info(),
                to: ctx.accounts.winner_token_account2.to_account_info(),
                authority: ctx.accounts.player2_vault.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx =
                CpiContext::new_with_signer(cpi_program, cpi_accounts_p2, player2_vault_signer);
            token::transfer(cpi_ctx, ctx.accounts.player2_vault.amount)?;
        }

        emit!(RewardClaimedEvent {
            player: ctx.accounts.winner.key(),
            amount,
        });

        Ok(())
    }
}

// Account Structures
#[account]
#[derive(InitSpace)]
pub struct DuelOrder {
    pub duel_id: u64,
    pub player1: Pubkey,
    pub player2: Pubkey,
    pub player1_mint: Pubkey,
    pub player2_mint: Pubkey,
    pub player1_stats: EncryptedStats,
    pub player2_stats: EncryptedStats,
    pub stake_amount: u64,
    pub winner: Pubkey,
    pub status: DuelStatus,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct EncryptedStats {
    pub attack: [u8; 32],
    pub defense: [u8; 32],
    pub speed: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, InitSpace)]
pub enum DuelStatus {
    WaitingForOpponent,
    ReadyToBattle,
    BattleInProgress,
    Completed,
}

// Account Contexts
#[derive(Accounts)]
#[instruction(duel_id: u64)]
pub struct StakeAndJoin<'info> {
    #[account(mut)]
    pub player: Signer<'info>,

    #[account(
        init_if_needed,
        payer = player,
        space = 8 + DuelOrder::INIT_SPACE,
        seeds = [b"duel", duel_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub duel_order: Account<'info, DuelOrder>,

    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = player,
        token::mint = token_mint,
        token::authority = vault,
        seeds = [b"vault", duel_order.key().as_ref(), token_mint.key().as_ref()],
        bump,
    )]
    pub vault: Account<'info, TokenAccount>,

    /// CHECK: Token mint
    pub token_mint: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[queue_computation_accounts("execute_battle", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct StartBattle<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(mut)]
    pub duel_order: Account<'info, DuelOrder>,

    #[account(address = derive_mxe_pda!())]
    pub mxe_account: Account<'info, MXEAccount>,

    #[account(mut, address = derive_mempool_pda!())]
    /// CHECK: Arcium mempool
    pub mempool_account: UncheckedAccount<'info>,

    #[account(mut, address = derive_execpool_pda!())]
    /// CHECK: Arcium execution pool
    pub executing_pool: UncheckedAccount<'info>,

    #[account(mut, address = derive_comp_pda!(computation_offset))]
    /// CHECK: Computation account
    pub computation_account: UncheckedAccount<'info>,

    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_BATTLE))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,

    #[account(mut, address = derive_cluster_pda!(mxe_account))]
    pub cluster_account: Account<'info, Cluster>,

    #[account(mut, address = ARCIUM_FEE_POOL_ACCOUNT_ADDRESS)]
    pub pool_account: Account<'info, FeePool>,

    #[account(address = ARCIUM_CLOCK_ACCOUNT_ADDRESS)]
    pub clock_account: Account<'info, ClockAccount>,

    pub system_program: Program<'info, System>,
    pub arcium_program: Program<'info, Arcium>,
}

#[callback_accounts("execute_battle", payer)]
#[derive(Accounts)]
pub struct BattleResultCallback<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub arcium_program: Program<'info, Arcium>,

    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_BATTLE))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,

    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: Instructions sysvar
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub duel_order: Account<'info, DuelOrder>,
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    pub winner: Signer<'info>,

    /// the nfts and staked amount will be transfered to this ata
    #[account(mut)]
    pub winner_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub winner_token_account2: Account<'info, TokenAccount>,

    #[account(mut)]
    pub duel_order: Account<'info, DuelOrder>,

    // now we need to get both the vaults
    #[account(
        mut,
        token::mint = duel_order.player2_mint,
        token::authority = player1_vault,
        seeds = [b"vault", duel_order.key().as_ref(), player1_vault.key().as_ref()],
        bump
    )]
    pub player1_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = duel_order.player2_mint,
        token::authority = player2_vault,
        seeds = [b"vault", duel_order.key().as_ref(), player2_vault.key().as_ref()],
        bump
    )]
    pub player2_vault: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[init_computation_definition_accounts("execute_battle", payer)]
#[derive(Accounts)]
pub struct InitBattleCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(mut, address = derive_mxe_pda!())]
    pub mxe_account: Box<Account<'info, MXEAccount>>,

    #[account(mut)]
    /// CHECK: Comp def account
    pub comp_def_account: UncheckedAccount<'info>,

    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

// Events
#[event]
pub struct PlayerJoinedEvent {
    pub duel_id: u64,
    pub player: Pubkey,
    pub stake: u64,
}

#[event]
pub struct BattleCompletedEvent {
    pub duel_id: u64,
    pub winner: Pubkey,
}

#[event]
pub struct RewardClaimedEvent {
    pub player: Pubkey,
    pub amount: u64,
}

// Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Duel is not open for joining")]
    DuelNotOpen,
    #[msg("Battle is not ready to start")]
    BattleNotReady,
    #[msg("Battle computation failed")]
    BattleFailed,
    #[msg("Battle not completed yet")]
    BattleNotCompleted,
    #[msg("You are not a participant in this duel")]
    NotAParticipant,
    #[msg("Cluster Not Set")]
    ClusterNotSet,
}
