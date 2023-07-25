#!/bin/sh
######################################################################
# Create some test files which will be served by a http server
#
# The http server will make some test files available via the following urls:
#
#  * http://localhost:80/10MB               (no speedlimit)
#  * http://localhost:80/speedlimit/10MB    (throttled)
#
######################################################################

set -e

WWW_DIR=/var/www/testfiles

create_test_file() {
    name="$1"
    size_mb="$2"

    TEST_FILE="$WWW_DIR/$name"
    if [ -f "$TEST_FILE" ]; then
        echo "Test file already exists: $TEST_FILE"
        return
    fi

    mkdir -p "$WWW_DIR"
    dd if=/dev/zero of="$TEST_FILE" bs=1M count="$size_mb"
}

# Create dummy files
create_test_file "10MB" 10

echo "Starting http server"
systemctl enable nginx
systemctl restart nginx
