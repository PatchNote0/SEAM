from datetime import datetime, timezone

from fastapi.testclient import TestClient

from app.main import app

client = TestClient(app)

ADA = {
    "id": 7,
    "email": "ada@example.com",
    "display_name": "Ada Lovelace",
    "created_at": datetime(2026, 1, 4, 9, tzinfo=timezone.utc),
}


def test_get_account_returns_display_name(monkeypatch):
    monkeypatch.setattr("app.db.fetch_account", lambda account_id: ADA)
    response = client.get("/accounts/7")
    assert response.status_code == 200
    assert response.json()["display_name"] == "Ada Lovelace"


def test_unknown_account_is_404(monkeypatch):
    monkeypatch.setattr("app.db.fetch_account", lambda account_id: None)
    assert client.get("/accounts/99").status_code == 404


def test_list_invoices(monkeypatch):
    invoice = {
        "id": 3,
        "amount_cents": 4200,
        "currency": "EUR",
        "issued_at": datetime(2026, 2, 1, tzinfo=timezone.utc),
        "paid": False,
    }
    monkeypatch.setattr("app.db.fetch_invoices", lambda account_id: [invoice])
    response = client.get("/accounts/7/invoices")
    assert response.json()[0]["amount_cents"] == 4200
