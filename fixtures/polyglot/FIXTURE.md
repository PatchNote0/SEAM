# Ledgerline: the polyglot fixture

A small accounting system used to demonstrate and test SEAM. It is real code:
the API's pytest suite and the web client's type check and Vitest suite pass
(see "Checking the fixture" below). The defects listed here are deliberate.
Production code in SEAM must never refer to them; only tests may.

## Components

| Component | Directory | Language | Role |
|---|---|---|---|
| `@ledgerline/web` | `web/` | TypeScript | Browser client. `src/api/http.ts` wraps `fetch`; the other modules call the wrappers. |
| `ledgerline-api` | `api/` | Python, FastAPI, Pydantic | Accounts, invoices and payments. Contract in `api/openapi.yaml`. |
| `ledger` | `ledger/` | Rust, Axum, sqlx | Ledger entries and balances. |
| `db` | `db/migrations/` | PostgreSQL | Schema migrations, applied in order. |
| Compose | `docker-compose.yml` | YAML | Service names, build contexts and environment. `web` reaches the API through `VITE_API_URL=http://api:8000`. |

## Boundaries a reader would expect SEAM to find

| Consumer | Producer | Mechanism | Contract |
|---|---|---|---|
| `web` `fetchAccount` | `api` `get_account` | `GET /accounts/{account_id}` | `getAccount` |
| `web` `listInvoices` | `api` `list_invoices` | `GET /accounts/{account_id}/invoices` | `listInvoices` |
| `web` `createPayment` | `api` `create_payment` | `POST /payments` | `createPayment` |
| `api` `record_entry` | `ledger` `create_entry` | `POST /ledger/entries` | none |
| `api`, `ledger` | `db` | tables `accounts`, `invoices`, `ledger_entries` | migrations |

## Deliberate defects

| # | Defect | Where | Detected since |
|---|---|---|---|
| 1 | **Incompatible response field.** The web client's `Account` reads `name`; the API's `AccountOut` and the contract send `display_name`. | `web/src/api/types.ts`, `api/app/models.py`, `api/openapi.yaml` | Phase 2 |
| 2 | **Outdated client.** `fetchUser` calls `GET /v1/users/{id}`, which no service implements. | `web/src/api/legacy.ts` | Phase 2 |
| 3 | **Renamed SQL column.** Migration 0002 renames `accounts.name` to `display_name`; the ledger's balance query still selects `a.name`. | `db/migrations/0002_rename_account_name.sql`, `ledger/src/main.rs` | Phase 4 (planned) |
| 4 | **Missing environment variable.** The API requires `PAYMENTS_WEBHOOK_SECRET`; Compose does not set it for `api`. | `api/app/settings.py`, `docker-compose.yml` | Phase 3 (planned) |
| 5 | **Incomplete test coverage.** Nothing tests `POST /payments` on either side, while both account endpoints have tests. | `web/src/api/payments.ts`, `api/app/routers/payments.py` | Phase 3 |
| 6 | **Historical contract change.** A scripted Git history renames the field in the API without updating the client, for Change Rehearsal. | to be added under `history/` | Phase 5 (planned) |

Two further properties exercise SEAM's honesty rather than its detection:

- `web/src/api/search.ts` builds its path from a runtime argument
  (`/${collection}/search`). SEAM must record it as UNKNOWN instead of
  guessing a target.
- `PaymentReceipt.ledger_entry_id` is `number` in the client but
  `Optional[int]` in the API and `[integer, "null"]` in the contract: the
  consumer's type does not admit the `null` the producer may send.

## Checking the fixture

```sh
# API tests (Python 3.12+)
cd api && pip install "fastapi>=0.115" "httpx>=0.28" "psycopg[binary]>=3.2" "pydantic>=2.9" pytest
python -m pytest -q          # 3 passed

# Web client (Node 22)
cd web && npm install
npx tsc --noEmit             # no errors
npx vitest run               # 2 passed
```

The web tests pass while defect 1 is present, because they mock the
response with the old field. That is the situation SEAM exists for.
