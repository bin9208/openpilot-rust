# Original fixed hardware-control policy cases

Captured all 35 original check_hardware_control.py cases using unchanged
hardware_control_source.run and the original generic board methods. No native
candidate generated expected outputs. Manifest pins every executed source plus
packed/expanded corpus hashes and size. Fourteen independent whole-checker
success/failure/encoding captures preserve artifact order.

This fixed-input gate does not replace the generic hardware_control_source helper
or original hardware methods. They remain live pinned dependencies until their
separate dynamic native coverage is complete. The retired fixed checker has an
explicit reviewed SHA identity; changed reintroduced source is rejected.
