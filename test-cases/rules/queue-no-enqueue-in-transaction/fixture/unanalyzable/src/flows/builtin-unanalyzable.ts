import { createDb, transaction } from "@sample/database";
import { createQueue, enqueue } from "@sample/jobs";
import type { Queue } from "@sample/jobs";
import * as jobsNs from "@sample/jobs";
import { enqueue as starEnqueue, queue as starQueue } from "../jobs-star";

declare const method: string;
declare const flag: boolean;
declare const rows: { id: string }[];
declare const unknownJobs: Record<string, (job: { receiptId: string }) => Promise<void>>;
declare const unknownQueues: Record<string, { add: (job: { receiptId: string }) => Promise<void> }>;
declare const callbacks: [(row: { id: string }) => unknown];
declare function getCallback(): (row: { id: string }) => unknown;
function localFallback(_job: { receiptId: string }) {}

const publisherQueue = createQueue();
const opaqueTx = createDb();
await opaqueTx("write", () => {});

await transaction("write", async (tx: (sql: string) => Promise<void>) => {
  rows.map((r) => unknownJobs[method]({ receiptId: r.id }));
  rows.map(starEnqueue);
  rows.map(jobsNs as unknown as (row: { id: string }) => unknown);
  await starQueue.add({ receiptId: "star-member" });
  await unknownQueues[method].add({ receiptId: "dynamic-receiver" });
  await publisherQueue.add({ receiptId: "publisher-factory" });
  await publisherQueue.publish({ receiptId: "non-builtin-member" });
  let chosen: (job: { receiptId: string }) => unknown = localFallback;
  if (flag) {
    chosen = enqueue;
  }
  await chosen({ receiptId: "known-ambiguous" });
  `${starQueue.id}`.trim();
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
  typedQueue: Queue,
) {
  return transaction("write", async () => {
    items.map(opaqueCallback);
    await typedQueue.add({ receiptId: "typed-publisher" });
  });
}
