"""`python -m drawland map …`: the command line."""

import sys

from ._drawland import cli


def main() -> None:
    sys.exit(cli(sys.argv[1:]))


if __name__ == "__main__":
    main()
