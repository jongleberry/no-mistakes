import { transaction } from "@sample/database";
import { enqueue } from "@sample/jobs";
import { enqueue as previewEnqueue } from "@sample/preview";

function afterCommit(_callback: () => Promise<void>) {}

await transaction("write", async () => {
  await previewEnqueue({ receiptId: "preview" });
  afterCommit(async () => {
    await enqueue({ receiptId: "deferred" });
  });
});

async function preview() {
  async function enqueue(_job: { receiptId: string }) {}
  await transaction("write", async () => {
    await enqueue({ receiptId: "shadowed" });
  });
}

async function runPreview() {
  async function transaction(_mode: string, callback: () => Promise<void>) {
    await callback();
  }
  await transaction("read", async () => {
    await enqueue({ receiptId: "unrelated-transaction" });
  });
}

const DeferredReceipt = await transaction("write", () => {
  return class ReceiptJob {
    queued = enqueue({ receiptId: "after-construction" });
  };
});
new DeferredReceipt();

await transaction("write", () => enqueue.bind(null));

await transaction("write", () => "committed").then(
  () => {},
  () => enqueue({ receiptId: "after-rollback" }),
);

export { preview, runPreview };
