import type { Account } from "../api/types";

export function AccountHeader({ account }: { account: Account }) {
  return (
    <header>
      <h1>{account.name}</h1>
      <p>{account.email}</p>
    </header>
  );
}
