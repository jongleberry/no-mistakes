import { transaction } from "@sample/database";
import {
  constNsAlias,
  localCycleA,
  mutableSend,
  objHelpers,
  opaqueMemberSend,
  spreadJobs,
} from "../jobs-named";

export function mapOpaqueLocalExports(rows: { id: string }[]) {
  return transaction("write", async () => {
    rows.map(opaqueMemberSend);
    rows.map(spreadJobs.enqueue);
    rows.map(constNsAlias as unknown as (row: { id: string }) => unknown);
    rows.map(mutableSend);
    rows.map(localCycleA);
    rows.map((objHelpers as unknown as { missing: (row: { id: string }) => unknown }).missing);
  });
}
