# Native HTTP read recovery (#140, #141)

The support CI failure exposed repeated uploads after interrupted response reads. The fix retries the read under its original deadline. A separate source-oracle timing assumption is corrected without changing the ten-second runtime timeout. Captures, red/green reproduction and precise scope are recorded in [HTTP interrupted validation](../rust-port/http-interrupted-validation.md).

Exact-SHA Actions remain required before integration. Full runtime normal-startup/upload acceptance stays open under #1. Docs-Not-Needed: native experimental transport correctness and test timing; production selection and user-visible settings are unchanged.
