import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Pp } from "../target/types/pp";
import { expect } from "chai";
import { configPda, requestPda, requestId } from "../sdk";
const { Keypair, SystemProgram, LAMPORTS_PER_SOL } = anchor.web3;
const BN = anchor.BN;

describe("policypilot-solana (local validator)", () => {
  const provider = anchor.AnchorProvider.env(); anchor.setProvider(provider);
  const program = anchor.workspace.pp as Program<Pp>;
  const admin = provider.wallet as anchor.Wallet;
  const executor = Keypair.generate(); const bob = Keypair.generate(); const eve = Keypair.generate();
  const config = configPda(program.programId, admin.publicKey);
  const exec = (key: string, to: anchor.web3.PublicKey, amt: number, signer = executor) => {
    const id = requestId(key);
    return program.methods.execute([...id], new BN(amt)).accountsPartial({
      config, request: requestPda(program.programId, config, id), executor: signer.publicKey, recipient: to, systemProgram: SystemProgram.programId,
    }).signers([signer]).rpc();
  };
  const expectErr = async (p: Promise<any>, code: string) => {
    try { await p; expect.fail("should revert"); } catch (e: any) { expect(String(e)).to.match(new RegExp(code)); }
  };
  before(async () => {
    const sig = await provider.connection.requestAirdrop(executor.publicKey, LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(sig);
    await program.methods.initialize(executor.publicKey, new BN(1_000_000), new BN(2_500_000)).accountsPartial({ config, admin: admin.publicKey }).rpc();
    await provider.sendAndConfirm(new anchor.web3.Transaction().add(SystemProgram.transfer({ fromPubkey: admin.publicKey, toPubkey: config, lamports: 10_000_000 })));
    await program.methods.setRecipient(bob.publicKey, true).accountsPartial({ config, admin: admin.publicKey }).rpc();
  });
  it("executes within policy", async () => {
    await exec("inv-1", bob.publicKey, 1_000_000);
    expect(await provider.connection.getBalance(bob.publicKey)).to.eq(1_000_000);
    expect((await program.account.config.fetch(config)).totalSpent.toNumber()).to.eq(1_000_000);
  });
  it("rejects replayed requestId", async () => { await expectErr(exec("inv-1", bob.publicKey, 1), "already in use"); });
  it("rejects non-allowlisted recipient", async () => { await expectErr(exec("inv-2", eve.publicKey, 1), "RecipientNotAllowed"); });
  it("rejects over per-request limit", async () => { await expectErr(exec("inv-3", bob.publicKey, 1_000_001), "PerRequestLimitExceeded"); });
  it("rejects zero amount", async () => { await expectErr(exec("inv-4", bob.publicKey, 0), "ZeroAmount"); });
  it("rejects non-executor", async () => {
    const sig = await provider.connection.requestAirdrop(eve.publicKey, LAMPORTS_PER_SOL); await provider.connection.confirmTransaction(sig);
    await expectErr(exec("inv-5", bob.publicKey, 1, eve), "NotExecutor");
  });
  it("enforces cumulative budget", async () => {
    await exec("inv-6", bob.publicKey, 1_000_000);
    await expectErr(exec("inv-7", bob.publicKey, 600_000), "BudgetExceeded");
    await exec("inv-8", bob.publicKey, 500_000);
    expect((await program.account.config.fetch(config)).totalSpent.toNumber()).to.eq(2_500_000);
  });
  it("pause blocks execution; admin-only", async () => {
    await expectErr(program.methods.setPaused(true).accountsPartial({ config, admin: eve.publicKey }).signers([eve]).rpc(), "NotAdmin");
    await program.methods.setLimits(new BN(1_000_000), new BN(5_000_000)).accountsPartial({ config, admin: admin.publicKey }).rpc();
    await program.methods.setPaused(true).accountsPartial({ config, admin: admin.publicKey }).rpc();
    await expectErr(exec("inv-9", bob.publicKey, 1), "Paused");
    await program.methods.setPaused(false).accountsPartial({ config, admin: admin.publicKey }).rpc();
    await exec("inv-9", bob.publicKey, 1);
  });
});
