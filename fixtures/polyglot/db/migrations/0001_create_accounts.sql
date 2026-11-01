CREATE TABLE accounts (
    id          BIGSERIAL PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE invoices (
    id            BIGSERIAL PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts (id),
    amount_cents  BIGINT NOT NULL CHECK (amount_cents >= 0),
    currency      CHAR(3) NOT NULL,
    issued_at     TIMESTAMPTZ NOT NULL,
    paid          BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE ledger_entries (
    id            BIGSERIAL PRIMARY KEY,
    account_id    BIGINT NOT NULL REFERENCES accounts (id),
    amount_cents  BIGINT NOT NULL,
    currency      CHAR(3) NOT NULL,
    recorded_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
