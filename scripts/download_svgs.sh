#!/bin/bash

# Define your Munin credentials
MUNIN_USERNAME="your_username"
MUNIN_PASSWORD="your_password"

# List of SVG files to download
SVG_FILES=("tedgecpuprocent-month.svg" "tedgemem-month.svg")

# Base URL of the Munin server
BASE_URL="https://munin.osadl.org/munin/osadl.org/rackfslot1.osadl.org/"

# Set the output folder relative to the project folder
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd ../.. && pwd)"
OUTPUT_FOLDER="$PROJECT_DIR/thin-edge.io/docs/src/benchmarks"

# Print the project directory and output folder for debugging
echo "Project Directory: $PROJECT_DIR"
echo "Output Folder: $OUTPUT_FOLDER"

# Create the output folder if it doesn't exist
mkdir -p "$OUTPUT_FOLDER"

# Download each SVG file
for SVG_FILE in "${SVG_FILES[@]}"; do
    SVG_URL="${BASE_URL}${SVG_FILE}"
    OUTPUT_PATH="${OUTPUT_FOLDER}/${SVG_FILE}"

    # Print the output path for debugging
    echo "Downloading $SVG_FILE to $OUTPUT_PATH"

    # Download the file using curl with basic authentication and skipping certificate verification
    curl --insecure --user "$MUNIN_USERNAME:$MUNIN_PASSWORD" "$SVG_URL" --output "$OUTPUT_PATH"

    # Check if the download was successful
    if [[ $? -eq 0 ]]; then
        echo "Downloaded $SVG_FILE to $OUTPUT_PATH"
    else
        echo "Failed to download $SVG_FILE"
    fi
done
