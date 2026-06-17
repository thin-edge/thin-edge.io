#!/bin/sh
# Print the cloud profile passed to the operation by the mapper.
# A mapper running for a named cloud profile sets TEDGE_CLOUD_PROFILE on the
# operation's child process; the default (unnamed) profile leaves it unset.
printf 'profile=%s' "${TEDGE_CLOUD_PROFILE}"
