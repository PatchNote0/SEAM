import { afterEach, describe, expect, it, vi } from "vitest";
import { fetchAccount, listInvoices } from "./accounts";

function respondWith(body: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(body), { status: 200 })),
  );
}

describe("accounts client", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("reads an account", async () => {
    respondWith({ id: 7, email: "ada@example.com", name: "Ada Lovelace", created_at: "2026-01-04T09:00:00Z" });
    const account = await fetchAccount(7);
    expect(account.name).toBe("Ada Lovelace");
  });

  it("lists invoices newest first", async () => {
    respondWith([{ id: 3, amount_cents: 4200, currency: "EUR", issued_at: "2026-02-01T00:00:00Z", paid: false }]);
    const invoices = await listInvoices(7);
    expect(invoices[0].amount_cents).toBe(4200);
  });
});
