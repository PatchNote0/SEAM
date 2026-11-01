# Evidence quality

Rules that keep every result traceable to source, reproducible and correctly classified.

- **Traceable.** Every claim maps to at least one evidence record with a path, span, excerpt and file hash. If you cannot point to the lines, do not make the claim.
- **Reproducible.** Running the same version on the same files yields the same Seams, ids, classes and findings. Sort everything; never depend on hash-map iteration order.
- **Classified by the weakest link.** Literal facts and deterministic resolution keep VERIFIED. Any assumption gives INFERRED. A missing counterpart gives PARSED. Unresolvable gives UNKNOWN. History alone gives CORRELATED, never more.
- **Assumptions in words.** Write each as a sentence a reviewer can check, naming what was not known: "The value of VITE_API_URL is not in the repository".
- **No percentages.** Confidence is strong, moderate or weak, with reasons.
- **Stale is visible.** If a cited file's hash changed, the Seam shows as stale until rescanned, whatever its stored class.
- **Unknowns are kept.** A call SEAM could not resolve is recorded and shown, not dropped.
- **Findings state facts.** "The consumer reads `name`; the producer's response has no field `name`." Not "this will crash".

Before merging a detection change, run the fixture tests and `seam explain` on one Seam of each class, and read every cited span.
