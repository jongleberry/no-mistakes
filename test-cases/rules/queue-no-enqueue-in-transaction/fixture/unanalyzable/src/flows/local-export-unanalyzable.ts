import { transaction } from "@sample/database";
import { missingTx, opaqueRunTx } from "../db-named";
import {
  constNsAlias,
  localCycleA,
  mutableSend,
  objHelpers,
  opaqueMemberSend,
  opaqueSend,
  spreadJobs,
} from "../jobs-named";

declare const method: string;
declare const unknownQueues: Record<string, { add: (job: { receiptId: string }) => Promise<void> }>;
declare class CustomQueue {
  add(job: { receiptId: string }): Promise<void>;
}

export function mapOpaqueLocalExports(rows: { id: string }[]) {
  return transaction("write", async () => {
    rows.map(opaqueMemberSend);
    rows.map(spreadJobs.enqueue);
    rows.map(constNsAlias as unknown as (row: { id: string }) => unknown);
    rows.map(mutableSend);
    rows.map(localCycleA);
    rows.map((objHelpers as unknown as { missing: (row: { id: string }) => unknown }).missing);
    await opaqueSend({ id: "direct-opaque-publish" });
    const dynamicQueue = unknownQueues[method];
    await dynamicQueue.add({ receiptId: "aliased-dynamic-receiver" });
    let mutableSet = new Set<string>();
    mutableSet = unknownQueues[method] as unknown as Set<string>;
    mutableSet.add("reassigned-receiver");
    const customQueue = new CustomQueue();
    await customQueue.add({ receiptId: "custom-ctor" });
  });
}

export async function runOpaqueAndMissingTransactions() {
  await opaqueRunTx("write", () => {});
  await missingTx("write", () => {});
}
