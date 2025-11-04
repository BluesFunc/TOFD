// app/src/client_mixed.ts
import * as anchor from "@coral-xyz/anchor";
import {
  TOKEN_2022_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  NATIVE_MINT, // классический WSOL mint (So111... на всех кластерах)
  getAssociatedTokenAddress,
  createAssociatedTokenAccountInstruction,
  getAccount,
} from "@solana/spl-token";
import { BN, Program } from "@coral-xyz/anchor";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";
import NodeWallet from "@coral-xyz/anchor/dist/cjs/nodewallet";

// ПОДКИНЬ СВОИ ПУТИ К IDL/ТИПАМ (они должны соответствовать lib.rs из прошлого сообщения)
import idl from "../../target/idl/dex_fixed_mixed.json";
import type { DexFixedMixed } from "../../target/types/dex_fixed_mixed";

// === НАСТРОЙКИ ===

// твой Token-2022 mint
const MINT_TOKEN = new PublicKey("5zUfzQRh6sTYJE5VRNpDKgxVtFfszWsfqnQ3FCPMBEEV");

// классический WSOL mint
const MINT_WSOL = NATIVE_MINT;

// стартовая ликвидность пула (atoms, предполагаем decimals=9)
const LIQ_TOKEN = 1_000_000_000; // 1 TOKEN
const LIQ_WSOL = 500_000_000; // 0.5 WSOL

// размеры обменов
const BUY_WSOL_IN = 100_000_000; // 0.1 WSOL -> ~0.2 TOKEN по курсу 1T=0.5WSOL
const SELL_TOKEN_IN = 200_000_000; // 0.2 TOKEN -> ~0.1 WSOL

async function getKeypairFromFile(path: string): Promise<Keypair> {
  const fs = await import("fs");
  const raw = fs.readFileSync(path, "utf-8");
  const secret = JSON.parse(raw);
  return Keypair.fromSecretKey(Uint8Array.from(secret));
}

async function ensureAtaIx(
  connection: Connection,
  payer: PublicKey,
  owner: PublicKey,
  mint: PublicKey,
  programId: PublicKey,
  allowOwnerOffCurve = false
) {
  const ata = await getAssociatedTokenAddress(
    mint,
    owner,
    allowOwnerOffCurve,
    programId
  );
  const info = await connection.getAccountInfo(ata);
  if (!info) {
    return {
      ata,
      ix: createAssociatedTokenAccountInstruction(
        payer,
        ata,
        owner,
        mint,
        programId
      ),
    };
  }
  return { ata, ix: null };
}

async function printBalances(
  connection: Connection,
  label: string,
  owner: PublicKey,
  tokenAta: PublicKey,
  wsolAta: PublicKey
) {
  const [t, w] = await Promise.all([
    getAccount(connection, tokenAta, "confirmed", TOKEN_2022_PROGRAM_ID).catch(
      () => null
    ),
    getAccount(connection, wsolAta, "confirmed", TOKEN_PROGRAM_ID).catch(
      () => null
    ),
  ]);
  console.log(
    `${label}
  ${owner.toBase58()}
    TOKEN-2022: ${t ? t.amount.toString() / 1000_000_000: "0"}
    WSOL (classic): ${w ? w.amount.toString() / 1000_000_000 : "0"}`
  );
}

async function main() {
  const url = process.env.SOLANA_URL ?? "http://127.0.0.1:8899";
  const connection = new Connection(url, "confirmed");

  const keypath =
    process.env.SOLANA_KEYPAIR ?? `${process.env.HOME}/.config/solana/id.json`;
  const kp = await getKeypairFromFile(keypath);

  const wallet = new NodeWallet(kp);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  anchor.setProvider(provider);

  const program = new Program<DexFixedMixed>(idl as DexFixedMixed, provider);

  // sanity-checks: mint владельцы
  const tokenMintInfo = await connection.getAccountInfo(MINT_TOKEN);
  if (!tokenMintInfo) throw new Error("MINT_TOKEN not found on cluster");
  if (!tokenMintInfo.owner.equals(TOKEN_2022_PROGRAM_ID)) {
    throw new Error(
      `MINT_TOKEN must be Token-2022. Owner=${tokenMintInfo.owner.toBase58()}`
    );
  }
  const wsolMintInfo = await connection.getAccountInfo(MINT_WSOL);
  if (!wsolMintInfo) {
    throw new Error(
      "WSOL mint account not found on cluster – switch to devnet/localnet with WSOL, or mint mock"
    );
  }
  if (!wsolMintInfo.owner.equals(TOKEN_PROGRAM_ID)) {
    throw new Error(
      `WSOL mint must be classic SPL. Owner=${wsolMintInfo.owner.toBase58()}`
    );
  }

  // аккаунт пула
  const pool = Keypair.generate();

  // PDA владельца хранилищ пула
  const [vaultAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), pool.publicKey.toBuffer()],
    program.programId
  );

  // адреса хранилищ пула (ATA под соответствующие программы токенов, owner = PDA)
  const vaultTokenAta = await getAssociatedTokenAddress(
    MINT_TOKEN,
    vaultAuthority,
    true, // allowOwnerOffCurve для PDA
    TOKEN_2022_PROGRAM_ID
  );
  const vaultWsolAta = await getAssociatedTokenAddress(
    MINT_WSOL,
    vaultAuthority,
    true,
    TOKEN_PROGRAM_ID
  );

  // адреса ATA пользователя (owner on-curve)
  const { ata: userTokenAta, ix: createUserTokenAtaIx } = await ensureAtaIx(
    connection,
    kp.publicKey,
    kp.publicKey,
    MINT_TOKEN,
    TOKEN_2022_PROGRAM_ID,
    false
  );
  const { ata: userWsolAta, ix: createUserWsolAtaIx } = await ensureAtaIx(
    connection,
    kp.publicKey,
    kp.publicKey,
    MINT_WSOL,
    TOKEN_PROGRAM_ID,
    false
  );

  // airdrop для локалнета (на devnet можешь убрать)
  try {
    await connection.requestAirdrop(kp.publicKey, 3_000_000_000);
  } catch {}

  // создадим недостающие ATA у пользователя
  if (createUserTokenAtaIx || createUserWsolAtaIx) {
    const tx = new Transaction();
    if (createUserTokenAtaIx) tx.add(createUserTokenAtaIx);
    if (createUserWsolAtaIx) tx.add(createUserWsolAtaIx);
    const sig = await connection.sendTransaction(tx, [kp]);
    await connection.confirmTransaction(sig, "confirmed");
  }

  await printBalances(
    connection,
    "BEFORE init",
    kp.publicKey,
    userTokenAta,
    userWsolAta
  );

  // initialize — кладём ликвидность из user ATA в vault ATA
  {
    const sig = await program.methods
      .initialize(new BN(LIQ_TOKEN), new BN(LIQ_WSOL))
      .accounts({
        signer: kp.publicKey,
        pool: pool.publicKey,
        vaultAuthority,
        vaultToken: vaultTokenAta,
        vaultWsol: vaultWsolAta,
        userToken: userTokenAta,
        userWsol: userWsolAta,
        mintToken: MINT_TOKEN,
        mintWsol: MINT_WSOL,
        tokenProgramToken: TOKEN_2022_PROGRAM_ID, // твой токен
        tokenProgramWsol: TOKEN_PROGRAM_ID, // классический WSOL
        associatedTokenProgram: new PublicKey(
          "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
        ),
        systemProgram: SystemProgram.programId,
      } as any)
      .signers([kp, pool])
      .rpc();
    console.log("initialize sig:", sig);
  }

  await printBalances(
    connection,
    "AFTER init (user)",
    kp.publicKey,
    userTokenAta,
    userWsolAta
  );

  // BUY: отправляем WSOL (classic) → получаем твой Token-2022
  {
    const sig = await program.methods
      .buy(new BN(BUY_WSOL_IN))
      .accounts({
        user: kp.publicKey,
        pool: pool.publicKey,
        vaultAuthority,
        vaultToken: vaultTokenAta,
        vaultWsol: vaultWsolAta,
        userToken: userTokenAta,
        userWsol: userWsolAta,
        mintToken: MINT_TOKEN,
        mintWsol: MINT_WSOL,
        tokenProgramToken: TOKEN_2022_PROGRAM_ID,
        tokenProgramWsol: TOKEN_PROGRAM_ID,
      } as any)
      .signers([kp])
      .rpc();
    console.log("buy sig:", sig);
  }

  await printBalances(
    connection,
    "AFTER BUY (user)",
    kp.publicKey,
    userTokenAta,
    userWsolAta
  );

  // SELL: отправляем твой Token-2022 → получаем WSOL (classic)
  {
    const sig = await program.methods
      .sell(new BN(SELL_TOKEN_IN))
      .accounts({
        user: kp.publicKey,
        pool: pool.publicKey,
        vaultAuthority,
        vaultToken: vaultTokenAta,
        vaultWsol: vaultWsolAta,
        userToken: userTokenAta,
        userWsol: userWsolAta,
        mintToken: MINT_TOKEN,
        mintWsol: MINT_WSOL,
        tokenProgramToken: TOKEN_2022_PROGRAM_ID,
        tokenProgramWsol: TOKEN_PROGRAM_ID,
      } as any)
      .signers([kp])
      .rpc();
    console.log("sell sig:", sig);
  }

  await printBalances(
    connection,
    "AFTER SELL (user)",
    kp.publicKey,
    userTokenAta,
    userWsolAta
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
