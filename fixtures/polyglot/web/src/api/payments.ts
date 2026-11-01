import { postJson } from "./http";
import type { PaymentReceipt, PaymentRequest } from "./types";

export function createPayment(request: PaymentRequest): Promise<PaymentReceipt> {
  return postJson<PaymentReceipt>("/payments", request);
}
