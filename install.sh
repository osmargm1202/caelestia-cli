#!/usr/bin/env bash
set -euo pipefail

rm -rf dist
python -m build --wheel
sudo python -m installer --overwrite-existing dist/*.whl
sudo cp completions/caelestia.fish /usr/share/fish/vendor_completions.d/caelestia.fish
