import httpx

from app.settings import settings


def record_entry(account_id: int, amount_cents: int, currency: str) -> int:
    response = httpx.post(
        f"{settings.ledger_url}/ledger/entries",
        json={"account_id": account_id, "amount_cents": amount_cents, "currency": currency},
        timeout=5.0,
    )
    response.raise_for_status()
    return response.json()["entry_id"]
