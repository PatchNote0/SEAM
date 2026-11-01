# Reviewing a Seam

How to check a discovered Seam against its source evidence before trusting it or claiming a detection works.

Run `seam explain <seam-id> --repo <path>` and check each point against the files it cites, not against the report.

1. **Evidence exists.** Open every cited span. The excerpt must match the file at that line. A stale mark means the file changed since the scan.
2. **Producer and consumer are the right way round.** The producer implements the route, table or variable; the consumer depends on it.
3. **The mechanism matches the code.** Method and path (or table, column, variable) appear literally or through a recorded resolution step.
4. **Resolution steps are real.** Each wrapper, import, prefix or constant step must be reproducible by reading the cited lines.
5. **Assumptions are written down.** Anything the analysis could not prove appears under assumptions, with the capability that was missing.
6. **Types line up.** For each field mapping, compare the consumer type, the producer type and the contract by hand. A finding must survive that check.
7. **Classification is earned.** VERIFIED needs zero assumptions. Any assumption means INFERRED. A missing side means PARSED. No resolvable boundary means UNKNOWN.
8. **No look-alike matches.** If the only link is a similar string in a comment, a log line or a test name, the Seam is a false positive: write a negative test, then fix the matcher.

Report each problem with the evidence span that shows it and the rule above that it breaks.
