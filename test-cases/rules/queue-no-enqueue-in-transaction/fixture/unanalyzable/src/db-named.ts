import { transaction as starTx } from "./db-star";
export { transaction, transaction as beginTransaction, createDb } from "@sample/database";
export { transaction as beginViaStar } from "./db-star";
export { starTx as localStarTx };
export * as dbStarNs from "./db-star";
