from fastapi import APIRouter, HTTPException

from app import db
from app.models import AccountOut, InvoiceOut

router = APIRouter(prefix="/accounts", tags=["accounts"])


@router.get("/{account_id}", response_model=AccountOut)
def get_account(account_id: int) -> AccountOut:
    row = db.fetch_account(account_id)
    if row is None:
        raise HTTPException(status_code=404, detail="account not found")
    return AccountOut(**row)


@router.get("/{account_id}/invoices", response_model=list[InvoiceOut])
def list_invoices(account_id: int) -> list[InvoiceOut]:
    return [InvoiceOut(**row) for row in db.fetch_invoices(account_id)]
