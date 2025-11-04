import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Lab3 } from "../target/types/lab3";
import { Keypair, sendAndConfirmTransaction, Transaction } from "@solana/web3.js";
import { sendTransaction } from "@solana-developers/helpers";


describe("lab3", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.lab3 as Program<Lab3>;

  it("Is initialized!", async () => {
    const provider = anchor.getProvider()
    const payer = provider.wallet.payer
    const counter = Keypair.generate()

    const initInstruction = await program.methods.initalize().accounts(
      {payer: payer.publicKey,
      counter: counter.publicKey}
    ).instruction()

    const incrementInstructionSet = []

    for (let i = 0; i < 3; i++){
      incrementInstructionSet.push( 
        await program.methods.increment()
        .accounts({counterAccount: counter.publicKey}).instruction()
      )
    }

    const decrementInstruction = await program.methods.decrement()
    .accounts({counterAccount: counter.publicKey}).instruction()

    const transaction = new Transaction().add(
      initInstruction,
      ...incrementInstructionSet,
      decrementInstruction
    )

    const transactionSignature = await sendTransaction(
      provider.connection,
      transaction,
      [payer, counter]
    )

 
    await provider.connection.confirmTransaction(transactionSignature, "finalized");

    const confirmedTx = await provider.connection.getTransaction(transactionSignature, {
      commitment: "finalized",
    });

    console.log("Confirmed transaction logs:", confirmedTx?.meta?.logMessages);

    const counterAccount = await program.account.counterAccount.fetch(counter.publicKey)

    console.log("Counter value:", counterAccount.value)


  });
});
