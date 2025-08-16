use arcis_imports::*;

#[encrypted]
mod circuits {
    use arcis_imports::*;

    pub struct PlayerData {
        pub attack: u16,
        pub defense: u16,
        pub speed: u16,
    }

    /// Simple battle execution - compares encrypted stats and determines winner
    #[instruction]
    pub fn execute_battle(
        player1: Enc<Shared, PlayerData>,
        player2: Enc<Shared, PlayerData>,
    ) -> u8 {
        let p1 = player1.to_arcis();
        let p2 = player2.to_arcis();

        // Add randomness
        let rng = ArcisRNG::gen_integer_from_width(32) % 100;
        
        // Calculate battle scores
        let p1_score = p1.attack + p1.defense + p1.speed + (rng % 20) as u16;
        let p2_score = p2.attack + p2.defense + p2.speed + ((100 - rng) % 20) as u16;

        // Return winner: 1 = player1, 2 = player2, 0 = draw
        if p1_score > p2_score {
            1
        } else if p2_score > p1_score {
            2
        } else {
            0
        }.reveal()
    }
}
