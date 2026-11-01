# 0005. No async runtime in the engine yet

Status: accepted, 2026-11-01

## Context

The brief lists Tokio. The current pipeline reads local files, parses them
and writes SQLite. None of it waits on sockets or timers.

## Decision

Use rayon for parallel extraction and plain threads elsewhere. Introduce
Tokio with the file watcher and the desktop bridge, where event loops and
cancellation of long-lived tasks justify it.

## Consequences

- Smaller dependency tree and simpler stack traces in the engine.
- The watcher will need a clear boundary between async event handling and
  the synchronous engine; the engine API already takes a cancellation token
  and a progress callback for that reason.
