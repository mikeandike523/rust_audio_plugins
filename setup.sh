#!/bin/bash

set -euo pipefail

# Store which python command is valid as a variable
# First, test python3, if that doesnt work, test python
# if that doesnt work, exit with error

if command -v python3 &>/dev/null; then
    PYTHON_CMD="python3"
elif command -v python &>/dev/null; then
    PYTHON_CMD="python"
else
    echo "Error: Python is not installed. Please install Python 3.6 or higher."
    exit 1
fi

# python command -m venv .venv
$PYTHON_CMD -m venv .venv

# If chmod command is available, make the ./__inenv executable
if command -v chmod &>/dev/null; then
    chmod +x ./__inenv
fi

# call ./__inenv to install the dependencies
./__inenv pip install -r requirements.txt

echo "Setup complete."
