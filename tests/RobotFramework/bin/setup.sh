#!/bin/bash
#
# Setup the testing environment by configuring python and
# building the test container device images
#

set -e

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
PROJECT_DIR=$( cd --  "$SCRIPT_DIR/../../../" && pwd )
pushd "$SCRIPT_DIR/.." >/dev/null || exit 1

# Required to prevent dbus errors on raspberry pi
export PYTHON_KEYRING_BACKEND=keyring.backends.null.Keyring

#
# Setup python virtual environment and install dependencies
#
# Add local bin to path
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

# Create virtual environment
python3 -m venv .venv

echo "Activating virtual environment"
# shellcheck disable=SC1091
source .venv/bin/activate
pip3 install --upgrade pip

REQUIREMENTS=(
    -r
    "requirements/requirements.txt"
    -r
    "requirements/requirements.dev.txt"
)

# Support installing only selected device adapters to minimize
# dependencies for specific test runners
if [ $# -gt 0 ]; then
    while [ $# -gt 0 ]; do
        ADAPTER="$1"
        case "$ADAPTER" in
            local|docker|ssh)
                if [ -f "requirements.adapter-${1}.txt" ]; then
                    echo "Install device test adapter: $1"
                    REQUIREMENTS+=(
                        -r
                        "requirements/requirements.adapter-${ADAPTER}.txt"
                    )
                fi
                ;;
            *)
                echo "Invalid device test adapter. Only 'local', 'docker' or 'ssh' are supported"
                exit 1
                ;;
        esac
        shift
    done
else
    # include all adapters
    REQUIREMENTS+=(
        -r
        "requirements/requirements.adapter.txt"
    )
fi

pip3 install "${REQUIREMENTS[@]}"

SITE_PACKAGES=$(find .venv -type d -name "site-packages" | head -1)
if [ -d "$SITE_PACKAGES" ]; then
    echo "$(pwd)/libraries" > "$SITE_PACKAGES/workspace.pth"
else
    echo "Could not add libraries path to site-packages. Reason: failed to find site-packages folder"
    exit 1
fi

#
# Setup dotenv file
#
DOTENV_FILE="$PROJECT_DIR/.env"
DOTENV_TEMPLATE="$PROJECT_DIR/tests/RobotFramework/devdata/env.template"

show_dotenv_help () {
    echo
    echo "Please edit your .env file with the secrets which are required for testing"
    echo
    echo "  $DOTENV_FILE"
    echo
}

if [ ! -f "$DOTENV_FILE" ]; then
    echo
    echo
    echo "Creating the .env file from the template"
    cp "$DOTENV_TEMPLATE" "$DOTENV_FILE"
    show_dotenv_help
elif ! grep "# Testing" "$DOTENV_FILE" >/dev/null; then
    echo
    echo
    echo "Adding required Testing variables to your existing .env file"
    cat "$DOTENV_TEMPLATE" >> "$DOTENV_FILE"
    show_dotenv_help
else
    echo
    echo
    echo "Your .env file already contains the '# Testing' section"
    echo "If test tests are still not working then check for any newly added settings in the template .env file"
    echo
    echo "  Current file:  $DOTENV_FILE"
    echo "  Template file: $DOTENV_TEMPLATE"
    echo
fi

#
# Create a symlink to the local folder due to support
# running via the Robocorp extensions, or running the commands
# manually on the command line
# Note: This is not ideal but it works
#
if [ ! -f .env ]; then
    if [ ! -L .env ]; then
        echo "Creating symlink to project .env file"
        ln -s "$DOTENV_FILE" ".env"
    fi
fi

popd >/dev/null || exit 1
