from datetime import datetime
from typing import Optional

from pydantic import BaseModel, Field


class AccountOut(BaseModel):
    id: int
    email: str
    display_name: str
    created_at: datetime


class InvoiceOut(BaseModel):
    id: int
    amount_cents: int
    currency: str
    issued_at: datetime
    paid: bool


class PaymentIn(BaseModel):
    account_id: int
    amount_cents: int = Field(gt=0)
    currency: str = Field(min_length=3, max_length=3)


class PaymentReceipt(BaseModel):
    payment_id: str = Field(serialization_alias="paymentId")
    status: str
    ledger_entry_id: Optional[int] = None
