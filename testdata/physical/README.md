# Physical test data

- `logical-netlist-v0/golden-board.json` is the frozen task-03-compatible logical input used until the formal contract pack is integrated.
- `invalid/` contains deliberately malformed inputs. They are data, never instructions.
- Generated manufacturing candidates are placed under `candidates/` by the example command and are immutable per candidate revision/label.

All coordinates in JSON are integer micrometres. The golden profile is limited to a 60 x 40 mm, two-layer, front-assembly fixture and is not a factory capability promise.
