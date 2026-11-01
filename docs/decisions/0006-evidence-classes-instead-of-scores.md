# 0006. Evidence classes instead of numeric scores

Status: accepted, 2026-11-01

## Context

Numeric confidence (for example "87%") suggests a calibrated probability.
Nothing in static analysis of arbitrary repositories supports that
calibration.

## Decision

Classify every Seam as VERIFIED, PARSED, INFERRED, CORRELATED or UNKNOWN by
the weakest link in its chain, and grade confidence as strong, moderate or
weak with written reasons. Deterministic resolution steps do not lower the
class; assumptions do. History alone can only yield CORRELATED.

## Consequences

- Results are explainable line by line, and tests can assert the class and
  the reasons rather than a number.
- Ranking inside a class uses explicit, documented ordering (findings first,
  then confidence, then name), not a hidden score.
