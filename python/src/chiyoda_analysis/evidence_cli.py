"""CLI for reproducible research-data acquisition and content locking."""

from __future__ import annotations

import argparse
from typing import Sequence

from .evidence import EvidenceError, fetch_catalog, load_catalog, verify_catalog_files


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Acquire and content-lock Chiyoda evidence sources")
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("fetch", "lock"):
        command = commands.add_parser(name)
        command.add_argument("catalog", help="path to an evidence catalog JSON file")
        command.add_argument("--data-root", default="data/raw", help="root for non-versioned raw data")
    arguments = parser.parse_args(argv)
    try:
        catalog = load_catalog(arguments.catalog)
        if arguments.command == "fetch":
            paths = fetch_catalog(catalog, arguments.data_root)
            print(f"acquired and content-locked {len(paths)} source file(s)")
        else:
            paths = verify_catalog_files(catalog, arguments.data_root)
            print(f"content-locked {len(paths)} source file(s)")
    except EvidenceError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
