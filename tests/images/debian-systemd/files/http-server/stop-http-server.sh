#!/bin/sh
######################################################################
# Stop the http server
######################################################################

set -e
systemctl disable nginx
systemctl stop nginx
