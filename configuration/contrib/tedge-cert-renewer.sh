#!/bin/sh
set -e
#
# Try to renew the certificate, but assume the script can
# be killed at any point in time so backup the certificate before
# replacing it, and validate the new certificate and rollback to the
# previous certificate on failure.
#
# Key design points:
# * don't assume the current certificate is loaded by the bridge
# * if a backup exists, then the renewal process was interrupted
# * remove any backups after the new certificate has been verified
#

if [ $# -lt 1 ]; then
    echo "missing required position argument" >&2
    exit 1
fi

CLOUD="$1"
CERT_PATH=$(tedge config get "${CLOUD}.device.cert_path")
BACKUP_CERTIFICATE="${CERT_PATH}.bak"

verify_certificate_or_rollback() {
    if tedge reconnect "$CLOUD"; then
        echo "Successfully reconnected to $CLOUD. Removing backup certificate" >&2
        rm -f "${CERT_PATH}.bak"
        return
    fi

    # rollback
    echo "Failed to connect to ${CLOUD}, restoring last known working certificate"
    echo "------ BEGIN Failed Certificate ------" >&2 ||:
    head -n 100 "$CERT_PATH" >&2 ||:
    echo "------ END Failed Certificate ------" >&2 ||:
    mv "${CERT_PATH}.bak" "$CERT_PATH"
    tedge reconnect "$CLOUD" || echo "WARNING: Failed to reconnect after restoring previous certificate. Maybe it is just an transient error" >&2
}

is_backup_same() {
    if command -V md5sum >/dev/null 2>&1; then
        [ "$(md5sum < "$BACKUP_CERTIFICATE" ||:)" = "$(md5sum < "$CERT_PATH" ||:)" ]
    elif command -V sha256sum >/dev/null 2>&1; then
        [ "$(sha256sum < "$BACKUP_CERTIFICATE" ||:)" = "$(sha256sum < "$CERT_PATH" ||:)" ]
    elif command -V cmp >/dev/null 2>&1; then
        cmp -s "$BACKUP_CERTIFICATE" "$CERT_PATH"
    else
        # assume the files are not the same (more defensive)
        return 1
    fi   
}

# If a backup file already exists, than the script may of been interrupted, so check if the
# current connection is ok or not, and revert to the backup if necessary
# If we don't do this check, then it could result in the backup replacing a potentially good certificate
if [ -f "$BACKUP_CERTIFICATE" ]; then
    if is_backup_same; then
        echo "Back file is the same so removing it" >&2
        rm -f "$BACKUP_CERTIFICATE"
    else
        echo "Warning: backup certificate exists and is different to current file. path=$BACKUP_CERTIFICATE" >&2
        verify_certificate_or_rollback
    fi
fi

echo "Backup up certificate file. $BACKUP_CERTIFICATE" >&2
cp "$CERT_PATH" "$BACKUP_CERTIFICATE"

if ! tedge cert renew "$CLOUD"; then
    # Check if the existing certificate has been overwritten by
    # the renew command, if so, then the backup should be restored
    if ! is_backup_same; then
        echo "Certificate has been changed by the renewal process even though it was not successful, so restoring the backup" >&2
        mv "$BACKUP_CERTIFICATE" "$CERT_PATH"
    else
        rm -f "$BACKUP_CERTIFICATE"
    fi

    echo "Failed to renew certificate" >&2
    exit 1
fi

verify_certificate_or_rollback
