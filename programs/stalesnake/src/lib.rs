use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

const COMP_DEF_OFFSET_EXECUTE_BATTLE: u32 = comp_def_offset("execute_battle");
const PLATFORM_FEE_BPS: u16 = 500; // 5% platform fee

declare_id!("EZLXMbgSwoXWBDB1xLd43bKjtVjAxDYPRqNcZir2idyr");

#[arcium_program]
pub mod arena_stake {
    use super::*;

    pub fn init_battle_comp_def(ctx: Context<InitBattleCompDef>) -> Result<()> {
        init_comp_def(ctx.accounts, true, 0, None, None)?;
        Ok(())
    }

    /// Creates a new duel request with encrypted fighter stats and strategy
    pub fn create_duel(
        ctx: Context<CreateDuel>,
        duel_id: u64,
        nft_mint: Pubkey,
        stake_amount: u64,
        encrypted_fighter_stats: EncryptedFighterStats,
        encrypted_strategy: EncryptedStrategy,
    ) -> Result<()> {
        let duel = &mut ctx.accounts.duel_account;
        
        duel.id = duel_id;
        duel.creator = ctx.accounts.player.key();
        duel.nft_mint = nft_mint;
        duel.stake_amount = stake_amount;
        duel.encrypted_fighter_stats = encrypted_fighter_stats;
        duel.encrypted_strategy = encrypted_strategy;
        duel.opponent = Pubkey::default();
        duel.status = DuelStatus::Open;
        duel.created_at = Clock::get()?.unix_timestamp;

        // Transfer stake to escrow
        let cpi_accounts = Transfer {
            from: ctx.accounts.player_token_account.to_account_info(),
            to: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.player.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, stake_amount)?;

        emit!(DuelCreatedEvent {
            duel_id,
            creator: ctx.accounts.player.key(),
            stake_amount,
        });

        Ok(())
    }

    /// Join an existing duel with your fighter
    pub fn join_duel(
        ctx: Context<JoinDuel>,
        opponent_nft_mint: Pubkey,
        encrypted_fighter_stats: EncryptedFighterStats,
        encrypted_strategy: EncryptedStrategy,
    ) -> Result<()> {
        let duel = &mut ctx.accounts.duel_account;
        
        require!(
            duel.status == DuelStatus::Open,
            ErrorCode::DuelNotOpen
        );
        require!(
            duel.creator != ctx.accounts.opponent.key(),
            ErrorCode::CannotDuelYourself
        );

        duel.opponent = ctx.accounts.opponent.key();
        duel.opponent_nft_mint = opponent_nft_mint;
        duel.opponent_encrypted_fighter_stats = encrypted_fighter_stats;
        duel.opponent_encrypted_strategy = encrypted_strategy;
        duel.status = DuelStatus::Matched;

        // Transfer opponent's stake to escrow
        let cpi_accounts = Transfer {
            from: ctx.accounts.opponent_token_account.to_account_info(),
            to: ctx.accounts.escrow_token_account.to_account_info(),
            authority: ctx.accounts.opponent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, duel.stake_amount)?;

        emit!(DuelMatchedEvent {
            duel_id: duel.id,
            opponent: ctx.accounts.opponent.key(),
        });

        Ok(())
    }

    pub fn execute_battle(
        ctx: Context<ExecuteBattle>,
        computation_offset: u64,
        player1_pubkey: [u8; 32],
        player1_nonce: u128,
        player2_pubkey: [u8; 32],
        player2_nonce: u128,
    ) -> Result<()> {
        let duel = &ctx.accounts.duel_account;
        
        require!(
            duel.status == DuelStatus::Matched,
            ErrorCode::DuelNotReady
        );

        // Prepare encrypted battle data for MPC
        // We need to pass each combo move separately since EncryptedU8Array doesn't exist
        let args = vec![
            // Player 1 data
            Argument::ArcisPubkey(player1_pubkey),
            Argument::PlaintextU128(player1_nonce),
            Argument::EncryptedU16(duel.encrypted_fighter_stats.attack),
            Argument::EncryptedU16(duel.encrypted_fighter_stats.defense),
            Argument::EncryptedU16(duel.encrypted_fighter_stats.speed),
            Argument::EncryptedU8(duel.encrypted_fighter_stats.special_move),
            Argument::EncryptedU8(duel.encrypted_strategy.stance),
            Argument::EncryptedU8(duel.encrypted_strategy.target_stat),
            Argument::EncryptedU8(duel.encrypted_strategy.combo1),
            Argument::EncryptedU8(duel.encrypted_strategy.combo2),
            Argument::EncryptedU8(duel.encrypted_strategy.combo3),
            Argument::PlaintextU64(duel.stake_amount),
            
            // Player 2 data
            Argument::ArcisPubkey(player2_pubkey),
            Argument::PlaintextU128(player2_nonce),
            Argument::EncryptedU16(duel.opponent_encrypted_fighter_stats.attack),
            Argument::EncryptedU16(duel.opponent_encrypted_fighter_stats.defense),
            Argument::EncryptedU16(duel.opponent_encrypted_fighter_stats.speed),
            Argument::EncryptedU8(duel.opponent_encrypted_fighter_stats.special_move),
            Argument::EncryptedU8(duel.opponent_encrypted_strategy.stance),
            Argument::EncryptedU8(duel.opponent_encrypted_strategy.target_stat),
            Argument::EncryptedU8(duel.opponent_encrypted_strategy.combo1),
            Argument::EncryptedU8(duel.opponent_encrypted_strategy.combo2),
            Argument::EncryptedU8(duel.opponent_encrypted_strategy.combo3),
            Argument::PlaintextU64(duel.stake_amount),
        ];

        queue_computation(
            ctx.accounts, 
            computation_offset, 
            args, 
            vec![], 
            None
        )?;

        Ok(())
    }

    /// Callback to handle battle results from MPC
    #[arcium_callback(encrypted_ix = "execute_battle")]
    pub fn execute_battle_callback(
        ctx: Context<BattleCallback>,
        output: ComputationOutputs<ExecuteBattleOutput>,
    ) -> Result<()> {
        let result = match output {
            ComputationOutputs::Success(ExecuteBattleOutput { field_0 }) => field_0,
            _ => return Err(ErrorCode::BattleComputationFailed.into()),
        };

        let duel = &mut ctx.accounts.duel_account;
        
        // Determine winner
        let (winner, loser) = match result {
            1 => (duel.creator, duel.opponent),
            2 => (duel.opponent, duel.creator),
            _ => {
                // Draw - return stakes
                duel.status = DuelStatus::Draw;
                emit!(BattleResultEvent {
                    duel_id: duel.id,
                    winner: Pubkey::default(),
                    result: "Draw".to_string(),
                });
                return Ok(());
            }
        };

        duel.winner = winner;
        duel.status = DuelStatus::Completed;

        emit!(BattleResultEvent {
            duel_id: duel.id,
            winner,
            result: format!("Player {} wins!", if result == 1 { "1" } else { "2" }),
        });

        Ok(())
    }

    /// Claim winnings after battle completion
    pub fn claim_winnings(ctx: Context<ClaimWinnings>) -> Result<()> {
        let duel = &ctx.accounts.duel_account;
        
        require!(
            duel.status == DuelStatus::Completed,
            ErrorCode::BattleNotCompleted
        );
        require!(
            duel.winner == ctx.accounts.winner.key(),
            ErrorCode::NotTheWinner
        );
        require!(
            !duel.winnings_claimed,
            ErrorCode::WinningsAlreadyClaimed
        );

        let total_stake = duel.stake_amount * 2;
        let platform_fee = (total_stake * PLATFORM_FEE_BPS as u64) / 10000;
        let winner_payout = total_stake - platform_fee;

        // Transfer winnings from escrow to winner
        let duel_key = duel.key();
        let seeds = &[
            b"escrow",
            duel_key.as_ref(),
            &[ctx.bumps.escrow_token_account],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.escrow_token_account.to_account_info(),
            to: ctx.accounts.winner_token_account.to_account_info(),
            authority: ctx.accounts.escrow_token_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, winner_payout)?;

        let duel = &mut ctx.accounts.duel_account;
        duel.winnings_claimed = true;

        emit!(WinningsClaimedEvent {
            duel_id: duel.id,
            winner: ctx.accounts.winner.key(),
            amount: winner_payout,
        });

        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct DuelAccount {
    pub id: u64,
    pub creator: Pubkey,
    pub opponent: Pubkey,
    pub nft_mint: Pubkey,
    pub opponent_nft_mint: Pubkey,
    pub stake_amount: u64,
    pub encrypted_fighter_stats: EncryptedFighterStats,
    pub encrypted_strategy: EncryptedStrategy,
    pub opponent_encrypted_fighter_stats: EncryptedFighterStats,
    pub opponent_encrypted_strategy: EncryptedStrategy,
    pub status: DuelStatus,
    pub winner: Pubkey,
    pub winnings_claimed: bool,
    pub created_at: i64,
}


#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct EncryptedFighterStats {
    pub attack: [u8; 32],
    pub defense: [u8; 32],
    pub speed: [u8; 32],
    pub special_move: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct EncryptedStrategy {
    pub stance: [u8; 32],
    pub target_stat: [u8; 32],
    pub combo1: [u8; 32],  // First combo move
    pub combo2: [u8; 32],  // Second combo move
    pub combo3: [u8; 32],  // Third combo move
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, InitSpace)]
pub enum DuelStatus {
    Open,
    Matched,
    InBattle,
    Completed,
    Draw,
    Cancelled,
}

// Account contexts
#[derive(Accounts)]
#[instruction(duel_id: u64)]
pub struct CreateDuel<'info> {
    #[account(mut)]
    pub player: Signer<'info>,
    #[account(
        init,
        payer = player,
        space = 8 + DuelAccount::INIT_SPACE,
        seeds = [b"duel", player.key().as_ref(), duel_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub duel_account: Account<'info, DuelAccount>,
    #[account(mut)]
    pub player_token_account: Account<'info, TokenAccount>,
    #[account(
        init,
        payer = player,
        token::mint = token_mint,
        token::authority = escrow_token_account,
        seeds = [b"escrow", duel_account.key().as_ref()],
        bump,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    /// CHECK: Token mint for staking
    pub token_mint: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinDuel<'info> {
    #[account(mut)]
    pub opponent: Signer<'info>,
    #[account(
        mut,
        constraint = duel_account.status == DuelStatus::Open
    )]
    pub duel_account: Account<'info, DuelAccount>,
    #[account(mut)]
    pub opponent_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"escrow", duel_account.key().as_ref()],
        bump,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[queue_computation_accounts("execute_battle", payer)]
#[derive(Accounts)]
#[instruction(computation_offset: u64)]
pub struct ExecuteBattle<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub duel_account: Account<'info, DuelAccount>,
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
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_EXECUTE_BATTLE))]
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
pub struct BattleCallback<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    pub arcium_program: Program<'info, Arcium>,
    #[account(address = derive_comp_def_pda!(COMP_DEF_OFFSET_EXECUTE_BATTLE))]
    pub comp_def_account: Account<'info, ComputationDefinitionAccount>,
    #[account(address = ::anchor_lang::solana_program::sysvar::instructions::ID)]
    /// CHECK: Instructions sysvar
    pub instructions_sysvar: AccountInfo<'info>,
    #[account(mut)]
    pub duel_account: Account<'info, DuelAccount>,
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

#[derive(Accounts)]
pub struct ClaimWinnings<'info> {
    pub winner: Signer<'info>,
    #[account(mut)]
    pub duel_account: Account<'info, DuelAccount>,
    #[account(
        mut,
        seeds = [b"escrow", duel_account.key().as_ref()],
        bump,
    )]
    pub escrow_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub winner_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

// Events
#[event]
pub struct DuelCreatedEvent {
    pub duel_id: u64,
    pub creator: Pubkey,
    pub stake_amount: u64,
}

#[event]
pub struct DuelMatchedEvent {
    pub duel_id: u64,
    pub opponent: Pubkey,
}

#[event]
pub struct BattleResultEvent {
    pub duel_id: u64,
    pub winner: Pubkey,
    pub result: String,
}

#[event]
pub struct WinningsClaimedEvent {
    pub duel_id: u64,
    pub winner: Pubkey,
    pub amount: u64,
}

// Error codes
#[error_code]
pub enum ErrorCode {
    #[msg("Duel is not open for joining")]
    DuelNotOpen,
    #[msg("Cannot duel yourself")]
    CannotDuelYourself,
    #[msg("Duel is not ready for battle")]
    DuelNotReady,
    #[msg("Battle computation failed")]
    BattleComputationFailed,
    #[msg("Battle not completed")]
    BattleNotCompleted,
    #[msg("You are not the winner")]
    NotTheWinner,
    #[msg("Winnings already claimed")]
    WinningsAlreadyClaimed,
    #[msg("Cluster not set")]
    ClusterNotSet,
}