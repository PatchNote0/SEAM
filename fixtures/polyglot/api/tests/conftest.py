import os

os.environ.setdefault("DATABASE_URL", "postgresql://test@localhost/test")
os.environ.setdefault("PAYMENTS_WEBHOOK_SECRET", "test-secret")
