import { decodeEventLog } from "viem";
import { vaultAbi } from "./abis/Vault.json";

export function decodeDeposit(log: {
  topics: [`0x${string}`, ...`0x${string}`[]];
  data: `0x${string}`;
}) {
  return decodeEventLog({
    abi: vaultAbi,
    eventName: "Deposit",
    topics: log.topics,
    data: log.data,
  });
}

export function decodeDepositAgain(log: {
  topics: [`0x${string}`, ...`0x${string}`[]];
  data: `0x${string}`;
}) {
  return decodeEventLog({
    abi: vaultAbi,
    eventName: "Deposit",
    topics: log.topics,
    data: log.data,
  });
}

export function unresolvedDecode(unknownOptions: Record<string, unknown>) {
  return decodeEventLog({
    abi: vaultAbi,
    ...unknownOptions,
  });
}
