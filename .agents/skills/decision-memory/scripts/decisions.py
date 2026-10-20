#!/usr/bin/env python3
"""Append and query durable user decisions for this repository."""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import sys
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, NoReturn

SCHEMA_VERSION = 1
ROOT = Path(__file__).resolve().parents[4]
DEFAULT_STORE = ROOT / ".pi" / "decisions.jsonl"


def fail(message: str) -> NoReturn:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(2)


def require_text(value: str, field: str, maximum: int = 1_000) -> str:
    result = value.strip()
    if not result:
        fail(f"{field} must not be empty")
    if len(result) > maximum:
        fail(f"{field} must be at most {maximum} characters")
    return result


def read_records(store: Path) -> list[dict[str, Any]]:
    if not store.exists():
        return []

    records: list[dict[str, Any]] = []
    for line_number, line in enumerate(store.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            fail(f"invalid JSON in {store}:{line_number}: {error.msg}")
        if not isinstance(record, dict) or record.get("schema") != SCHEMA_VERSION:
            fail(f"unsupported decision record in {store}:{line_number}")
        records.append(record)
    return records


def lock_path(store: Path) -> Path:
    return store.with_name(f"{store.name}.lock")


def acquire_lock(store: Path, operation: int):
    store.parent.mkdir(parents=True, exist_ok=True)
    handle = lock_path(store).open("a+", encoding="utf-8")
    fcntl.flock(handle.fileno(), operation)
    return handle


def release_lock(handle: Any) -> None:
    fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
    handle.close()


def next_id(records: list[dict[str, Any]]) -> str:
    highest = 0
    for record in records:
        matched = re.fullmatch(r"D-(\d+)", str(record.get("id", "")))
        if matched:
            highest = max(highest, int(matched.group(1)))
    return f"D-{highest + 1:04d}"


def timestamp(value: str | None) -> str:
    if value is None:
        return datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail("--at must be an ISO-8601 timestamp")
    if parsed.tzinfo is None:
        fail("--at must include a timezone")
    return parsed.astimezone(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def command_record(args: argparse.Namespace) -> int:
    store = Path(args.store).resolve()
    handle = acquire_lock(store, fcntl.LOCK_EX)
    try:
        records = read_records(store)
        key = args.key.strip() if args.key else None
        if key:
            existing = next((record for record in records if record.get("key") == key), None)
            if existing:
                print(f"unchanged {existing['id']}")
                return 0

        supersedes = sorted(set(args.supersedes))
        known_ids = {str(record.get("id")) for record in records}
        unknown = [decision_id for decision_id in supersedes if decision_id not in known_ids]
        if unknown:
            fail(f"unknown superseded decision(s): {', '.join(unknown)}")

        tags = sorted(
            {
                tag
                for value in args.tag
                for tag in (part.strip().lower() for part in value.split(","))
                if tag
            }
        )
        record: dict[str, Any] = {
            "schema": SCHEMA_VERSION,
            "id": next_id(records),
            "recorded_at": timestamp(args.at),
            "status": args.status,
            "scope": require_text(args.scope, "scope", 200),
            "decision": require_text(args.decision, "decision"),
            "rationale": require_text(args.rationale, "rationale"),
            "source": require_text(args.source, "source", 200),
            "tags": tags,
            "supersedes": supersedes,
        }
        if key:
            record["key"] = require_text(key, "key", 200)

        with store.open("a", encoding="utf-8") as destination:
            destination.write(json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n")
            destination.flush()
            os.fsync(destination.fileno())
        print(f"recorded {record['id']}")
        return 0
    finally:
        release_lock(handle)


def searchable_text(record: dict[str, Any]) -> str:
    values = (
        record.get("scope", ""),
        record.get("decision", ""),
        record.get("rationale", ""),
        record.get("source", ""),
        " ".join(record.get("tags", [])),
    )
    return " ".join(str(value).casefold() for value in values)


def command_query(args: argparse.Namespace) -> int:
    store = Path(args.store).resolve()
    handle = acquire_lock(store, fcntl.LOCK_SH)
    try:
        records = read_records(store)
    finally:
        release_lock(handle)

    terms = re.findall(r"[\w-]+", args.text.casefold())
    candidates = [record for record in records if args.all_statuses or record.get("status") == "active"]
    ranked: list[tuple[int, dict[str, Any]]] = []
    for record in candidates:
        text = searchable_text(record)
        score = sum(text.count(term) for term in terms)
        if not terms or score:
            ranked.append((score, record))
    ranked.sort(key=lambda item: (item[0], str(item[1].get("id", ""))), reverse=True)
    selected = [record for _, record in ranked[: args.limit]]

    if args.json:
        print(json.dumps(selected, ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    if not selected:
        print("No matching active decisions.")
        return 0

    print(f"Decision summary ({len(selected)}):")
    for record in selected:
        print(f"- {record['id']} [{record['status']}] {record['scope']}: {record['decision']}")
        print(f"  Why: {record['rationale']}")
        if record.get("supersedes"):
            print(f"  Supersedes: {', '.join(record['supersedes'])}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--store", default=DEFAULT_STORE, help="JSONL decision store (default: repository .pi/decisions.jsonl)")
    commands = parser.add_subparsers(dest="command", required=True)

    record = commands.add_parser("record", help="append one durable decision")
    record.add_argument("--scope", required=True)
    record.add_argument("--decision", required=True)
    record.add_argument("--rationale", required=True)
    record.add_argument("--source", default="user")
    record.add_argument("--tag", action="append", default=[])
    record.add_argument("--supersedes", action="append", default=[])
    record.add_argument("--key", help="idempotency key from the source interaction")
    record.add_argument("--status", choices=("active", "superseded"), default="active")
    record.add_argument("--at", help="ISO-8601 timestamp, useful for deterministic tests")
    record.set_defaults(handler=command_record)

    query = commands.add_parser("query", help="return concise matching decisions")
    query.add_argument("text", nargs="?", default="")
    query.add_argument("--all-statuses", action="store_true")
    query.add_argument("--limit", type=int, default=10)
    query.add_argument("--json", action="store_true")
    query.set_defaults(handler=command_query)
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    if getattr(args, "limit", 1) < 1:
        fail("--limit must be at least 1")
    return args.handler(args)


if __name__ == "__main__":
    raise SystemExit(main())
