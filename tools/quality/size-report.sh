#!/usr/bin/env bash
# SDK budgets use manifest identities, including the files actually transferred.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
exec node web/tools/test/web-size-report.mjs "$@"
