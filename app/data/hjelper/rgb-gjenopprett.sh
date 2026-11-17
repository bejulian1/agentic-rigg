#!/bin/bash
set -euo pipefail
here=$(dirname "$(readlink -f "$0")")
exec /usr/bin/python3 -I "$here/rgb-admin.py" restore "$@"
