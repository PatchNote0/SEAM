export interface Account {
  id: number;
  email: string;
  name: string;
  created_at: string;
}

export interface Invoice {
  id: number;
  amount_cents: number;
  currency: string;
  issued_at: string;
  paid: boolean;
}

export interface PaymentRequest {
  account_id: number;
  amount_cents: number;
  currency: string;
}

export interface PaymentReceipt {
  paymentId: string;
  status: "accepted" | "rejected";
  ledger_entry_id: number;
}

export interface User {
  id: number;
  login: string;
}
