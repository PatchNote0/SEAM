import hmac

from fastapi import APIRouter, Header, HTTPException

from app.ledger_client import record_entry
from app.models import PaymentIn, PaymentReceipt
from app.settings import settings

router = APIRouter(tags=["payments"])


@router.post("/payments", response_model=PaymentReceipt, status_code=201)
def create_payment(payment: PaymentIn, x_signature: str = Header(default="")) -> PaymentReceipt:
    expected = hmac.new(settings.webhook_secret.encode(), str(payment.account_id).encode(), "sha256")
    if x_signature and not hmac.compare_digest(expected.hexdigest(), x_signature):
        raise HTTPException(status_code=401, detail="bad signature")
    entry_id = record_entry(payment.account_id, payment.amount_cents, payment.currency)
    return PaymentReceipt(payment_id=f"pay_{entry_id}", status="accepted", ledger_entry_id=entry_id)
