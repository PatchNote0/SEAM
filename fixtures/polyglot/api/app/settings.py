import os
from dataclasses import dataclass


@dataclass(frozen=True)
class Settings:
    database_url: str
    ledger_url: str
    webhook_secret: str


settings = Settings(
    database_url=os.environ["DATABASE_URL"],
    ledger_url=os.environ.get("LEDGER_URL", "http://localhost:8080"),
    webhook_secret=os.environ["PAYMENTS_WEBHOOK_SECRET"],
)
