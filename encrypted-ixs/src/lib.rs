use arcis_imports::*;

#[encrypted]
mod circuits {
    use arcis_imports::*;

    /// Fighter stats for the battle
    pub struct FighterStats {
        pub attack: u16,
        pub defense: u16,
        pub speed: u16,
        pub special_move: u8,
    }

    /// Battle strategy chosen by player
    pub struct BattleStrategy {
        pub stance: u8,        // 0=aggressive, 1=defensive, 2=balanced
        pub target_stat: u8,   // 0=attack, 1=defense, 2=speed
        pub combo1: u8,        // First combo move
        pub combo2: u8,        // Second combo move
        pub combo3: u8,        // Third combo move
    }

    /// Combined battle data for a player
    pub struct PlayerBattleData {
        pub fighter_stats: FighterStats,
        pub strategy: BattleStrategy,
        pub stake_amount: u64,
    }

    /// Executes the confidential battle between two players
    #[instruction]
    pub fn execute_battle(
        player1_data: Enc<Shared, PlayerBattleData>,
        player2_data: Enc<Shared, PlayerBattleData>,
    ) -> u8 {
        let p1 = player1_data.to_arcis();
        let p2 = player2_data.to_arcis();

        // Generate random factors for battle
        let rng_factor1 = (ArcisRNG::gen_integer_from_width(32) % 100) as u16;
        let rng_factor2 = (ArcisRNG::gen_integer_from_width(32) % 100) as u16;
        let rng_factor3 = (ArcisRNG::gen_integer_from_width(32) % 100) as u16;

        // Calculate effective stats based on strategy
        let p1_effective_attack = calculate_effective_stat(
            p1.fighter_stats.attack,
            p1.strategy.stance,
            0,
            rng_factor1,
        );
        let p1_effective_defense = calculate_effective_stat(
            p1.fighter_stats.defense,
            p1.strategy.stance,
            1,
            rng_factor2,
        );
        let p1_effective_speed = calculate_effective_stat(
            p1.fighter_stats.speed,
            p1.strategy.stance,
            2,
            rng_factor3,
        );

        let p2_effective_attack = calculate_effective_stat(
            p2.fighter_stats.attack,
            p2.strategy.stance,
            0,
            rng_factor1,
        );
        let p2_effective_defense = calculate_effective_stat(
            p2.fighter_stats.defense,
            p2.strategy.stance,
            1,
            rng_factor2,
        );
        let p2_effective_speed = calculate_effective_stat(
            p2.fighter_stats.speed,
            p2.strategy.stance,
            2,
            rng_factor3,
        );

        // Battle scoring system
        let mut p1_score = 0u32;
        let mut p2_score = 0u32;

        // Speed determines who attacks first
        if p1_effective_speed > p2_effective_speed {
            p1_score += 10;
        } else {
            p2_score += 10;
        }

        // Attack vs Defense calculations
        if p1_effective_attack > p2_effective_defense {
            p1_score += (p1_effective_attack - p2_effective_defense) as u32;
        }
        if p2_effective_attack > p1_effective_defense {
            p2_score += (p2_effective_attack - p1_effective_defense) as u32;
        }

        // Special move bonus based on combo
        p1_score += calculate_combo_bonus(
            p1.strategy.combo1,
            p1.strategy.combo2,
            p1.strategy.combo3,
            p1.fighter_stats.special_move
        );
        p2_score += calculate_combo_bonus(
            p2.strategy.combo1,
            p2.strategy.combo2,
            p2.strategy.combo3,
            p2.fighter_stats.special_move
        );

        // Determine winner: 0=draw, 1=player1 wins, 2=player2 wins
        let result = if p1_score > p2_score {
            1
        } else if p2_score > p1_score {
            2
        } else {
            0
        };

        result.reveal()
    }

    fn calculate_effective_stat(base_stat: u16, stance: u8, stat_type: u8, rng: u16) -> u16 {
        let mut multiplier = 100u16;
        
        // Stance bonuses
        if stance == 0 && stat_type == 0 { // Aggressive boosts attack
            multiplier = 120;
        } else if stance == 1 && stat_type == 1 { // Defensive boosts defense
            multiplier = 120;
        } else if stance == 2 { // Balanced gives small boost to all
            multiplier = 110;
        }

        // Add randomness (0-20% variance)
        let random_boost = rng % 20;
        
        (base_stat * (multiplier + random_boost)) / 100
    }

    fn calculate_combo_bonus(combo1: u8, combo2: u8, combo3: u8, special_move: u8) -> u32 {
        let mut bonus = 0u32;
        
        // Check if combo matches a pattern
        if combo1 == special_move && combo2 == special_move {
            bonus += 15; // Double special
        }
        if combo1 + 1 == combo2 && combo2 + 1 == combo3 {
            bonus += 10; // Sequential combo
        }
        
        bonus
    }
}
