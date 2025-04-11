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
usage() {
    cat <<EOT
Reliable certificate renewer to check if a certificate should be renewed
and to renew the certificate in a reliable manner by backing up the previous
certificate, and rolling back if the new certificate does not pass the validation
check.

The scripts relies on the Cumulocity certificate-authority feature to work.

Usage

  $0 needs-renewal <cloud>      Check if the certificate needs renewal (exit code 0 if it does)
  $0 renew <cloud>              Renew the certificate for the given cloud

Examples

$0 needs-renewal c8y 
# Check if the Cumulocity certificate needs renewal

$0 renew c8y
# Renew the Cumulocity certificate

EOT
}

if [ $# -lt 2 ]; then
    echo "ERROR: missing required positional arguments" >&2
    usage
    exit 1
fi

COMMAND="$1"
CLOUD="$2"
shift
shift
CERT_PATH=$(tedge config get "${CLOUD}.device.cert_path")
BACKUP_CERTIFICATE="${CERT_PATH}.bak"

#
# Helpers
#
verify_certificate_or_rollback() {
    if tedge reconnect "$CLOUD"; then
        echo "Successfully reconnected to $CLOUD. Removing backup certificate" >&2
        rm -f "$BACKUP_CERTIFICATE"
        return
    fi

    # rollback
    echo "Failed to connect to ${CLOUD}, restoring last known working certificate"
    echo "------ BEGIN Failed Certificate ------" >&2 ||:
    head -n 100 "$CERT_PATH" >&2 ||:
    echo "------ END Failed Certificate ------" >&2 ||:
    mv "$BACKUP_CERTIFICATE" "$CERT_PATH"
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

#
# Commands
#
needs_renewal() {
    if [ -f "$BACKUP_CERTIFICATE" ]; then
        echo "Found a left-over certificate backup file which is a sign that the renewal did not fully complete" >&2
        exit 0
    fi

    /usr/bin/tedge cert needs-renewal "$CLOUD"
}

renew() {
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
        if is_backup_same; then
            echo "Certificate and backup are the same file so removing the backup" >&2
            rm -f "$BACKUP_CERTIFICATE"
        else
            echo "Certificate has been changed by the renewal process even though it was not successful, so restoring the backup" >&2
            mv "$BACKUP_CERTIFICATE" "$CERT_PATH" 
        fi

        echo "Failed to renew certificate" >&2
        exit 1
    fi

    verify_certificate_or_rollback
}


#
# Main
#
case "$COMMAND" in
    needs-renewal)
        needs_renewal
        ;;
    renew)
        renew
        ;;
    *)
        echo "ERROR: Unknown subcommand. $COMMAND" >&2
        usage
        exit 1
        ;;
esac
