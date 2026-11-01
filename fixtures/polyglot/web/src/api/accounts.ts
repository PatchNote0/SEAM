import { getJson } from "./http";
import type { Account, Invoice } from "./types";

export function fetchAccount(accountId: number): Promise<Account> {
  return getJson<Account>(`/accounts/${accountId}`);
}

export function listInvoices(accountId: number): Promise<Invoice[]> {
  return getJson<Invoice[]>(`/accounts/${accountId}/invoices`);
}
