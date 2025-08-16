import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { PublicKey, Keypair } from "@solana/web3.js";
import { Stalesnake } from "../target/types/stalesnake";
import { randomBytes } from "crypto";
import {
  awaitComputationFinalization,
  getArciumEnv,
  getCompDefAccOffset,
  getArciumAccountBaseSeed,
  getArciumProgAddress,
  uploadCircuit,
  buildFinalizeCompDefTx,
  RescueCipher,
  deserializeLE,
  getMXEAccAddress,
  getMempoolAccAddress,
  getCompDefAccAddress,
  getExecutingPoolAccAddress,
  x25519,
  getComputationAccAddress,
  getMXEPublicKey,
} from "@arcium-hq/client";
import * as fs from "fs";
import * as os from "os";
import { expect } from "chai";

describe("Stalesnake", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.Stalesnake as Program<Stalesnake>;
  const provider = anchor.getProvider();

  type Event = anchor.IdlEvents<(typeof program)["idl"]>;
  const awaitEvent = async <E extends keyof Event>(eventName: E) => {
    let listenerId: number;
    const event = await new Promise<Event[E]>((res) => {
      listenerId = program.addEventListener(eventName, (event) => {
        res(event);
      });
    });
    await program.removeEventListener(listenerId);

    return event;
  };

  const arciumEnv = getArciumEnv();

  // Complete test suite for Stalesnake battle system
  it("Tests the complete Stalesnake duel flow", async () => {
    const owner = readKpJson(`${os.homedir()}/.config/solana/id.json`);
    const player1 = Keypair.generate();
    const player2 = Keypair.generate();
    const unauthorizedPlayer = Keypair.generate();

    const mxePublicKey = await getMXEPublicKeyWithRetry(
      provider as anchor.AnchorProvider,
      program.programId
    );

    console.log("MXE x25519 pubkey is", mxePublicKey);

    // Step 1: Initialize computation definition
    console.log("Initializing execute_battle computation definition");
    const initBattleSig = await initBattleCompDef(program, owner, false);
    console.log(
      "Execute battle computation definition initialized with signature",
      initBattleSig
    );

    // Step 2: Test complete duel flow
    console.log("\n--- Testing complete duel flow ---");

    // Generate encryption keys for Player 1
    const player1PrivateKey = x25519.utils.randomPrivateKey();
    const player1PublicKey = x25519.getPublicKey(player1PrivateKey);
    const player1SharedSecret = x25519.getSharedSecret(
      player1PrivateKey,
      mxePublicKey
    );
    const player1Cipher = new RescueCipher(player1SharedSecret);

    // Generate encryption keys for Player 2
    const player2PrivateKey = x25519.utils.randomPrivateKey();
    const player2PublicKey = x25519.getPublicKey(player2PrivateKey);
    const player2SharedSecret = x25519.getSharedSecret(
      player2PrivateKey,
      mxePublicKey
    );
    const player2Cipher = new RescueCipher(player2SharedSecret);

    // Airdrop funds to players
    console.log("Airdropping funds to players");
    await Promise.all([
      airdropToPlayer(player1.publicKey),
      airdropToPlayer(player2.publicKey),
    ]);

    // Create mock NFT mints
    const player1NftMint = Keypair.generate().publicKey;
    const player2NftMint = Keypair.generate().publicKey;

    // Step 3: Create a duel
    const duelId = Date.now();
    const stakeAmount = new anchor.BN(1000000); // 0.001 SOL

    // Player 1 creates fighter stats and strategy
    const player1FighterStats = {
      attack: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32], // Encrypted attack: 85
      defense: [2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33], // Encrypted defense: 70
      speed: [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34], // Encrypted speed: 90
      specialMove: [4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35], // Encrypted special_move: 2
    };

    const player1Strategy = {
      stance: [5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36], // Encrypted stance: 1 (defensive)
      targetStat: [6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37], // Encrypted target_stat: 0 (attack)
      combo1: [7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38], // Encrypted combo1: 1
      combo2: [8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39], // Encrypted combo2: 2
      combo3: [9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40], // Encrypted combo3: 3
    };

    console.log("Player 1 creating duel");
    const createDuelTx = await program.methods
      .createDuel(
        new anchor.BN(duelId),
        player1NftMint,
        stakeAmount,
        player1FighterStats,
        player1Strategy
      )
      .accounts({
        player: player1.publicKey,
        playerTokenAccount: player1.publicKey, // Mock token account
        tokenMint: player1.publicKey, // Mock token mint
      })
      .signers([player1])
      .rpc({ commitment: "confirmed" });

    console.log("Duel created with signature:", createDuelTx);

    // Step 4: Player 2 joins the duel
    const player2FighterStats = {
      attack: [10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41], // Encrypted attack: 80
      defense: [11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42], // Encrypted defense: 75
      speed: [12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43], // Encrypted speed: 85
      specialMove: [13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44], // Encrypted special_move: 1
    };

    const player2Strategy = {
      stance: [14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45], // Encrypted stance: 0 (aggressive)
      targetStat: [15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46], // Encrypted target_stat: 2 (speed)
      combo1: [16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47], // Encrypted combo1: 2
      combo2: [17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48], // Encrypted combo2: 1
      combo3: [18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49], // Encrypted combo3: 2
    };

    console.log("Player 2 joining duel");
    const joinDuelTx = await program.methods
      .joinDuel(
        player2NftMint,
        player2FighterStats,
        player2Strategy
      )
      .accounts({
        opponent: player2.publicKey,
        opponentTokenAccount: player2.publicKey, // Mock token account
      })
      .signers([player2])
      .rpc({ commitment: "confirmed" });

    console.log("Player 2 joined duel with signature:", joinDuelTx);

    // Step 5: Execute the battle
    const battleEventPromise = awaitEvent("battleResultEvent");
    const computationOffset = new anchor.BN(randomBytes(8), "hex");

    const player1Nonce = new anchor.BN(deserializeLE(randomBytes(16)).toString());
    const player2Nonce = new anchor.BN(deserializeLE(randomBytes(16)).toString());

    console.log("Executing battle");
    const executeBattleTx = await program.methods
      .executeBattle(
        computationOffset,
        Array.from(player1PublicKey),
        player1Nonce,
        Array.from(player2PublicKey),
        player2Nonce
      )
      .accounts({
        payer: owner.publicKey,
        computationAccount: getComputationAccAddress(
          program.programId,
          computationOffset
        ),
        mxeAccount: getMXEAccAddress(program.programId),
        mempoolAccount: getMempoolAccAddress(program.programId),
        executingPool: getExecutingPoolAccAddress(program.programId),
        compDefAccount: getCompDefAccAddress(
          program.programId,
          Buffer.from(getCompDefAccOffset("execute_battle")).readUInt32LE()
        ),
        clusterAccount: arciumEnv.arciumClusterPubkey,
      })
      .signers([owner])
      .rpc({ commitment: "confirmed" });

    console.log("Battle execution signature:", executeBattleTx);

    // Wait for battle computation finalization
    const battleFinalizeSig = await awaitComputationFinalization(
      provider as anchor.AnchorProvider,
      computationOffset,
      program.programId,
      "confirmed"
    );
    console.log("Battle finalize signature:", battleFinalizeSig);

    const battleEvent = await battleEventPromise;
    console.log(`Battle result: ${battleEvent.result}`);

    // Verify the battle completed
    expect(battleEvent.result).to.not.equal("Draw");
    expect(battleEvent.winner).to.not.equal(PublicKey.default);

    // Step 6: Claim winnings
    const winner = battleEvent.winner;
    const winnerKeypair = winner.equals(player1.publicKey) ? player1 : player2;

    console.log("Claiming winnings");
    const claimWinningsTx = await program.methods
      .claimWinnings()
      .accounts({
        winner: winnerKeypair.publicKey,
        winnerTokenAccount: winnerKeypair.publicKey, // Mock token account
      })
      .signers([winnerKeypair])
      .rpc({ commitment: "confirmed" });

    console.log("Winnings claimed with signature:", claimWinningsTx);

    // Step 7: Test unauthorized player trying to claim winnings
    console.log("\n--- Testing unauthorized claim attempt ---");
    
    await airdropToPlayer(unauthorizedPlayer.publicKey);

    try {
      await program.methods
        .claimWinnings()
        .accounts({
          winner: unauthorizedPlayer.publicKey,
          winnerTokenAccount: unauthorizedPlayer.publicKey,
        })
        .signers([unauthorizedPlayer])
        .rpc({ commitment: "confirmed" });

      expect.fail("Unauthorized player was able to claim winnings");
    } catch (error) {
      console.log("Expected error caught:", error.message);
      expect(error).to.be.an("error");
    }

    console.log("\n--- All tests completed successfully ---");
  });

  // Helper function to airdrop funds
  async function airdropToPlayer(playerPubkey: PublicKey) {
    const airdropTx = await provider.connection.requestAirdrop(
      playerPubkey,
      2 * anchor.web3.LAMPORTS_PER_SOL
    );
    await provider.connection.confirmTransaction({
      signature: airdropTx,
      blockhash: (await provider.connection.getLatestBlockhash()).blockhash,
      lastValidBlockHeight: (
        await provider.connection.getLatestBlockhash()
      ).lastValidBlockHeight,
    });
  }
});

// Helper function to read keypair from JSON file
function readKpJson(path: string): anchor.web3.Keypair {
  const file = fs.readFileSync(path);
  return anchor.web3.Keypair.fromSecretKey(
    new Uint8Array(JSON.parse(file.toString()))
  );
}

// Initialize battle computation definition
async function initBattleCompDef(
  program: Program<Stalesnake>,
  owner: anchor.web3.Keypair,
  uploadRawCircuit: boolean
): Promise<string> {
  const baseSeedCompDefAcc = getArciumAccountBaseSeed(
    "ComputationDefinitionAccount"
  );
  const offset = getCompDefAccOffset("execute_battle");

  const compDefPDA = PublicKey.findProgramAddressSync(
    [baseSeedCompDefAcc, program.programId.toBuffer(), offset],
    getArciumProgAddress()
  )[0];

  console.log(`Comp def PDA for execute_battle:`, compDefPDA.toBase58());

  const sig = await program.methods
    .initBattleCompDef()
    .accounts({
      compDefAccount: compDefPDA,
      payer: owner.publicKey,
      mxeAccount: getMXEAccAddress(program.programId),
    })
    .signers([owner])
    .rpc({
      commitment: "confirmed",
    });

  console.log(`Init execute_battle computation definition transaction`, sig);

  if (uploadRawCircuit) {
    const rawCircuit = fs.readFileSync(`build/execute_battle.arcis`);
    await uploadCircuit(
      program.provider as anchor.AnchorProvider,
      "execute_battle",
      program.programId,
      rawCircuit,
      true
    );
  } else {
    const finalizeTx = await buildFinalizeCompDefTx(
      program.provider as anchor.AnchorProvider,
      Buffer.from(offset).readUInt32LE(),
      program.programId
    );

    const latestBlockhash =
      await program.provider.connection.getLatestBlockhash();
    finalizeTx.recentBlockhash = latestBlockhash.blockhash;
    finalizeTx.lastValidBlockHeight = latestBlockhash.lastValidBlockHeight;

    finalizeTx.sign(owner);
    await program.provider.sendAndConfirm(finalizeTx);
  }
  return sig;
}

async function getMXEPublicKeyWithRetry(
  provider: anchor.AnchorProvider,
  programId: PublicKey,
  maxRetries: number = 10,
  retryDelayMs: number = 500
): Promise<Uint8Array> {
  for (let attempt = 1; attempt <= maxRetries; attempt++) {
    try {
      const mxePublicKey = await getMXEPublicKey(provider, programId);
      if (mxePublicKey) {
        return mxePublicKey;
      }
    } catch (error) {
      console.log(`Attempt ${attempt} failed to fetch MXE public key:`, error);
    }

    if (attempt < maxRetries) {
      console.log(
        `Retrying in ${retryDelayMs}ms... (attempt ${attempt}/${maxRetries})`
      );
      await new Promise((resolve) => setTimeout(resolve, retryDelayMs));
    }
  }

  throw new Error(
    `Failed to fetch MXE public key after ${maxRetries} attempts`
  );
}
