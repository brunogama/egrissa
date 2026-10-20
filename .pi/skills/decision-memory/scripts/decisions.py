#!/usr/bin/env python3
"""Compatibility launcher; canonical helper lives under .agents/skills."""

from pathlib import Path
from runpy import run_path

run_path(
    Path(__file__).resolve().parents[4] / ".agents/skills/decision-memory/scripts/decisions.py",
    run_name="__main__",
)
