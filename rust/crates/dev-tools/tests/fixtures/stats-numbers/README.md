# Independent statsd numeric corpus

All 598 cases from the unchanged fixed-seed original `check_stats_numbers.py`
were evaluated against its extracted original `statsd.py` aggregation on Python
3.12.15. Inputs preserve Python float text and every original sample; expected
strings include signed zero, nonfinite values and exact aggregation formatting.
The native candidate did not generate expectations. Original implementation,
checker, compressed/plain corpus hashes, size and case count are pinned.

The corpus replaces this fixed-input checker only, not generic statsd, sorting,
Python random-number generation or the remaining original comparison tools.
`failures.json` runs the unchanged original main against controlled process
results to preserve report-write/stdout ordering on malformed/count/exit/encoding
failures and mismatches. The report is overwritten only at the original stage;
missing parent directories are not created. Original checker identity may be
absent after reviewed retirement; altered reintroduced source is rejected.
