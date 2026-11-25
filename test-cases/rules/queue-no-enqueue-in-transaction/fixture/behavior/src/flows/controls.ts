import { transaction } from "@sample/database";
import { enqueue } from "@sample/jobs";
import { enqueue as previewEnqueue } from "@sample/preview";

declare function afterCommit(callback: () => Promise<void>): void;
declare const mode: [string];
declare const callbacks: [() => Promise<void>];
declare const method: string;
declare const unknownJobs: Record<string, typeof enqueue>;

await transaction("write", async () => {
  await previewEnqueue({ receiptId: "preview" });
  await unknownJobs[method]({ receiptId: "dynamic" });
  afterCommit(async () => {
    await enqueue({ receiptId: "deferred" });
  });
});

async function preview(enqueue: (job: { receiptId: string }) => Promise<void>) {
  await transaction("write", async () => {
    await enqueue({ receiptId: "shadowed" });
  });
}

async function runPreview(
  transaction: (mode: string, callback: () => Promise<void>) => Promise<void>,
) {
  await transaction("read", async () => {
    await enqueue({ receiptId: "unrelated-transaction" });
  });
}

const publish = () => enqueue({ receiptId: "opaque-callback" });
await transaction("write", publish);
await transaction("write", function* () {
  yield enqueue({ receiptId: "generator" });
});
await transaction(...mode, () => enqueue({ receiptId: "spread-before-slot" }));
await transaction("write", ...callbacks);

const DeferredReceipt = await transaction("write", () => {
  return class ReceiptJob {
    queued = enqueue({ receiptId: "after-construction" });
  };
});
new DeferredReceipt();

await transaction("write", () => enqueue.bind(null));

const boundTransaction = transaction.bind(null, "write");
await boundTransaction(
  () => "callback-ran",
  () => enqueue({ receiptId: "unused-extra-argument" }),
);
await transaction("write", () => "committed").then(
  () => {},
  () => enqueue({ receiptId: "after-rollback" }),
);

await transaction`write ${() => enqueue({ receiptId: "tagged-argument" })}`;
