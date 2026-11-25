import { transaction } from "@sample/database";
import { createQueue } from "@sample/jobs";
import * as jobsNs from "@sample/jobs";
import { enqueue as starEnqueue, queue as starQueue } from "../jobs-star";

declare const method: string;
declare const rows: { id: string }[];
declare const unknownJobs: Record<string, (job: { receiptId: string }) => Promise<void>>;
declare const unknownQueues: Record<string, { add: (job: { receiptId: string }) => Promise<void> }>;
declare const callbacks: [(row: { id: string }) => unknown];
declare function getCallback(): (row: { id: string }) => unknown;

const publisherQueue = createQueue();

await transaction("write", async (tx: (sql: string) => Promise<void>) => {
  rows.map((r) => unknownJobs[method]({ receiptId: r.id }));
  rows.map(starEnqueue);
  rows.map(jobsNs as unknown as (row: { id: string }) => unknown);
  await starQueue.add({ receiptId: "star-member" });
  await unknownQueues[method].add({ receiptId: "dynamic-receiver" });
  await publisherQueue.add({ receiptId: "publisher-factory" });
  rows.map(getCallback());
  rows.map(...callbacks);
  rows.map(function* (r) {
    yield r.id;
  });
  let handle = tx;
  handle = unknownJobs[method] as unknown as typeof tx;
  await handle("SELECT 1");
  (null as unknown as { add(): void }).add();
  const a1 = jobsNs.enqueue;
  const a2 = a1;
  const a3 = a2;
  const a4 = a3;
  const a5 = a4;
  const a6 = a5;
  rows.map(a6);
});

export function unprovableLocalCallback(
  items: { id: string }[],
  opaqueCallback: (row: { id: string }) => unknown,
) {
  return transaction("write", async () => {
    items.map(opaqueCallback);
  });
}
