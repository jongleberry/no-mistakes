# `queue-no-enqueue-in-transaction`

Reports a configured queue publish in the body of an inline transaction callback.

## Why and when

A queue publish that persists independently of a database transaction can survive
its rollback. A worker can then receive work that depends on data the transaction
never committed. Enable this rule for transaction APIs that invoke their callbacks
before committing and publishers whose effects survive rollback.

## What it catches

The rule matches a transaction call and a publish by their resolved import specifier
and export name. An imported alias such as `import { enqueue as publish } from
"@sample/jobs"` retains the `enqueue` export identity. Configure each module
specifier and export the resolver exposes for your calls. Unresolved calls,
shadowing locals, and calls imported from unrelated modules stay outside the check.

The transaction call uses a direct import binding so its argument positions
can be proved. Calls through bound aliases and tagged templates stay outside
the check. The
configured argument must be an inline arrow or non-generator function
expression. The rule checks direct calls owned by that callback, including expression
bodies. It does not prove whether a particular branch runs. A named callback, an opaque spread at or before
the callback slot, and a generator body leave the transaction boundary
unproven. A separate nested function or deferred callback has its own body.
Deferred class instance fields also run outside that body.
The rule leaves nested publishes alone unless their callback belongs to a separate
configured transaction call.

## Options and defaults

The rule is off by default. Opt in with a repository-scoped application and
supply both non-empty lists below. Module and export names must contain text.
Each transaction module and export pair may appear once. Use `include` and `exclude` to select files by
repository-relative paths.

| Option | Meaning |
| --- | --- |
| `transactions` | Transaction APIs, declared as `{ module, export, callbackArgument }`. The callback argument index is zero-based and has no default. Each listed API must call the callback before commit. |
| `publishers` | Independently durable queue APIs, declared as `{ module, export }`. Leave transaction-bound outbox writers outside this list. |

```yaml
rules:
  - rule: queue-no-enqueue-in-transaction
    scope: repository
    include: ["src/**/*.ts"]
    options:
      transactions:
        - module: "@sample/database"
          export: transaction
          callbackArgument: 1
      publishers:
        - module: "@sample/jobs"
          export: enqueue
```

## Valid example

Return the data the publisher needs from the transaction callback. Call the
publisher once the transaction resolves and the data has committed.

```ts
import { transaction } from "@sample/database";
import { enqueue } from "@sample/jobs";

const receiptId = await transaction("write", async (tx) => {
  const id = await tx.insertReceipt();
  await tx.appendAuditEntry(id);
  return id;
});

await enqueue({ receiptId });
```

## Counterexample

The enqueue can succeed before a later write fails and rolls back the receipt.

```ts
import { transaction } from "@sample/database";
import { enqueue } from "@sample/jobs";

await transaction("write", async (tx) => {
  const receiptId = await tx.insertReceipt();
  await enqueue({ receiptId });
  await tx.appendAuditEntry(receiptId);
});
```

## Fix

Move this enqueue after the transaction commits. Return the job data from the
callback and enqueue it after the transaction call resolves.

## Suppression

A documented exception is an enqueue that provides the only durable handoff.
Keep it inside that transaction so a failed enqueue rolls the row back. The
enqueue must tolerate collisions, and the worker must be idempotent and treat a
missing row as a retry.

Use `no-mistakes-disable-next-line queue-no-enqueue-in-transaction` directly
above the publish or `no-mistakes-disable-line queue-no-enqueue-in-transaction`
on the publish line. Use `no-mistakes-disable-file queue-no-enqueue-in-transaction`
for a reviewed file-wide exception. Record why the queue is the sole durable
handoff.

## Analysis

The rule uses the prepared call graph at `max` effort. A request for `low`
reports an effort warning and uses `max`. Selected files with parse errors
produce incomplete analysis through the check report.

## Related rules

[`no-single-enqueue-in-loop`](no-single-enqueue-in-loop.md) checks for
per-item publishes that should use a bulk API.
