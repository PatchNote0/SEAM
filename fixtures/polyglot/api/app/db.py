import psycopg
from psycopg.rows import dict_row

from app.settings import settings

ACCOUNT_BY_ID = """
    SELECT id, email, display_name, created_at
    FROM accounts
    WHERE id = %(account_id)s
"""

INVOICES_FOR_ACCOUNT = """
    SELECT id, amount_cents, currency, issued_at, paid
    FROM invoices
    WHERE account_id = %(account_id)s
    ORDER BY issued_at DESC
"""


def _connect() -> psycopg.Connection:
    return psycopg.connect(settings.database_url, row_factory=dict_row)


def fetch_account(account_id: int) -> dict | None:
    with _connect() as conn:
        return conn.execute(ACCOUNT_BY_ID, {"account_id": account_id}).fetchone()


def fetch_invoices(account_id: int) -> list[dict]:
    with _connect() as conn:
        return conn.execute(INVOICES_FOR_ACCOUNT, {"account_id": account_id}).fetchall()
