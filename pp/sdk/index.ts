// Minimal SDK: PDA derivation + deterministic requestId (sha256 of a caller-supplied key).
import * as anchor from "@coral-xyz/anchor";
import { createHash } from "crypto";
export const configPda = (programId: anchor.web3.PublicKey, admin: anchor.web3.PublicKey) =>
  anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("config"), admin.toBuffer()], programId)[0];
export const requestPda = (programId: anchor.web3.PublicKey, config: anchor.web3.PublicKey, id: Buffer) =>
  anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("request"), config.toBuffer(), id], programId)[0];
export const requestId = (key: string) => createHash("sha256").update(key).digest();
