"""CLI entry point for non-mutating run-bundle inspection."""

from __future__ import annotations

import argparse
import json
from typing import Sequence

from .bundle import BundleError, load_bundle, summarize


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Inspect a verified Chiyoda run bundle")
    parser.add_argument("bundle", help="path to run.json")
    arguments = parser.parse_args(argv)
    try:
        print(json.dumps(summarize(load_bundle(arguments.bundle)), indent=2, sort_keys=True))
    except BundleError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

