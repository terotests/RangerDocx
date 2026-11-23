#!/usr/bin/env python3
"""27-table-vmerge.docx: cells merged down rows (w:vMerge), one merge whose
text is taller than the rows it covers, and one centred vertically."""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import make_fixtures as mf  # noqa: E402

R = {"vMerge": "restart"}
C = {"text": "", "vMerge": "continue"}

mf.write_docx(
    "27-table-vmerge.docx",
    [
        mf.p([("Vertical merge", True, False, None, None)], pstyle="Heading1"),
        mf.p([("Cells merged down rows with w:vMerge.", False, False, None, None)]),
        mf.table(
            [
                ["Region", "Quarter", "Sales"],
                [dict(R, text="North"), "Q1", "120"],
                [C, "Q2", "135"],
                [C, "Q3", "150"],
                [dict(R, text="South region, a merged cell with more text than its two rows can hold, so the last row grows"), "Q1", "90"],
                [C, "Q2", "95"],
            ],
            col_widths=[3000, 2400, 2400],
        ),
    ],
)
