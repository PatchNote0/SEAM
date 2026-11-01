from fastapi import FastAPI

from app.routers import accounts, payments

app = FastAPI(title="Ledgerline API", version="2.3.0")

app.include_router(accounts.router)
app.include_router(payments.router)


@app.get("/health")
def health() -> dict[str, str]:
    return {"status": "ok"}
