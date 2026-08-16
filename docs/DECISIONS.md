# Skylode - Decisions

**The ledger moved to [decisions/](decisions/) — one file per decision.**

It was a single table of 154 rows whose lines averaged 524 characters and ran to
2 028 at the longest. At that width `git diff` reports *"this line changed"* about a
paragraph, so the ledger could not be reviewed, and a decision revisited had nowhere
to go but on top of the one it replaced. It is now 154 numbered records, wrapped, each
carrying its status and its cross-references, indexed in
[decisions/README.md](decisions/README.md).

Nothing was dropped: every verdict and every argument moved across intact, including
the 25 rejected ideas and the 2 withdrawn ones. What was added is structure — a record
revised in place now says so in its header and names the wording it replaced, instead
of burying it in paragraph three.

This file stays so the links pointing here keep resolving. Cite a decision by its
number from now on: **0069**, not *"the prestige row"*.

- The index, grouped by subject: [decisions/README.md](decisions/README.md)
- What the rules **are**: [MECHANICS.md](MECHANICS.md) and [SYSTEMS.md](SYSTEMS.md)
- What they **cost**: [BALANCE.md](BALANCE.md), generated from the code
