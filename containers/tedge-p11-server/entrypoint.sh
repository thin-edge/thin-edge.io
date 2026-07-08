#!/bin/sh
set -e

# Entrypoint for the tedge-p11-server container.
#
# The image runs as the non-root `tedge` user (see the Dockerfile `USER`). There is no privilege
# dropping: access to the passed-through HSM device (e.g. a TPM at /dev/tpmrm0) is granted by adding
# the host's device group as a supplementary group to the container, via `group_add` (docker/compose)
# or `securityContext.supplementalGroups` (Kubernetes). See the README ("TPM 2.0 device access").
#
# Responsibilities (all as the current, non-root user):
#   1. Default the PKCS#11 module to this variant's bundled module.
#   2. Ensure the socket/token-store directories exist and clear a stale socket.
#   3. Initialize the token if needed (idempotent).
#   4. exec tedge-p11-server.
#
# Configuration is passed through to tedge-p11-server via environment variables (or CLI args):
#   TEDGE_DEVICE_CRYPTOKI_MODULE_PATH   path to the PKCS#11 module (.so) to load
#   TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH   where to create the UNIX socket
#   TEDGE_DEVICE_CRYPTOKI_PIN           default PIN for the token
#   TEDGE_DEVICE_CRYPTOKI_URI           token/object URI filter (RFC 7512)

SOCKET_PATH="${TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH:-/run/tedge-p11-server/tedge-p11-server.sock}"

log() {
    echo "[entrypoint] $*" >&2
}

# 1. Default the PKCS#11 module to this variant's bundled module (recorded at build time in
# /usr/lib/tedge-p11/.default-module). `minimal` has no default; an explicit value always wins.
if [ -z "${TEDGE_DEVICE_CRYPTOKI_MODULE_PATH:-}" ] && [ -r /usr/lib/tedge-p11/.default-module ]; then
    TEDGE_DEVICE_CRYPTOKI_MODULE_PATH=$(cat /usr/lib/tedge-p11/.default-module)
    export TEDGE_DEVICE_CRYPTOKI_MODULE_PATH
    log "using bundled PKCS#11 module ${TEDGE_DEVICE_CRYPTOKI_MODULE_PATH}"
fi

# Start pcscd (USB smartcard daemon) for the USB variant. pcscd does NOT need root: it needs a
# writable runtime dir (/run/pcscd, pre-created and owned by tedge in the image) and access to the
# USB device node (granted via `group_add`/`supplementalGroups` for the reader's group, same as the
# TPM). No-op for TPM/SoftHSM. `auto` starts it only when a USB device tree is passed through.
_want_pcscd=0
case "${P11_START_PCSCD:-auto}" in
    1|true|yes) _want_pcscd=1 ;;
    auto) [ -d /dev/bus/usb ] && command -v pcscd >/dev/null 2>&1 && _want_pcscd=1 ;;
    *) : ;;  # 0/off/no
esac

if [ "$_want_pcscd" = 1 ]; then
    if [ -w /run/pcscd ] || [ "$(id -u)" = "0" ]; then
        # A USB CCID smartcard gives *exclusive* access: exactly one process may hold the card at a
        # time. So we start pcscd and then leave the card entirely to tedge-p11-server - we do NOT
        # probe it here (e.g. opensc-tool/pkcs11-tool), because a second PC/SC client power-cycles the
        # card and can leave it unresponsive ("Card: No"), and we do NOT restart pcscd (its hotplug
        # does not work in a container and churning it wedges the reader).
        #
        # IMPORTANT: the *host* must not also run pcscd for this reader. Disable it on the host
        # (`systemctl disable --now pcscd.socket pcscd.service`) or share the host's pcscd via a
        # mounted /run/pcscd - otherwise the host daemon claims the reader (typically on boot) and
        # this container cannot. See the README ("USB tokens").
        log "starting pcscd"
        rm -f /run/pcscd/pcscd.comm /run/pcscd/pcscd.pid 2>/dev/null || true
        pcscd || log "WARNING: pcscd failed to start"
        # Give pcscd a moment to enumerate the reader before the server's PKCS#11 module initializes.
        sleep "${P11_PCSCD_SETTLE:-5}"
    else
        log "WARNING: /run/pcscd is not writable; cannot start pcscd. Mount a writable /run/pcscd, or run pcscd on the host and bind-mount its socket."
    fi
fi

# 2. Ensure the socket + token-store directories exist. With named volumes these already exist and
# are owned by the tedge user (inherited from the image); for empty bind mounts / emptyDir, rely on
# the volume being writable by this user (e.g. Kubernetes `fsGroup`).
mkdir -p "$(dirname "$SOCKET_PATH")" 2>/dev/null || true
mkdir -p /etc/tedge/hsm/tokens "${TPM2_PKCS11_STORE:-/etc/tedge/hsm}" 2>/dev/null || true

# Remove a stale socket from a previous run (e.g. after SIGKILL); tedge-p11-server binds the socket
# itself and would otherwise fail with "Address already in use", and clients would hit
# "Connection refused" on the dangling socket.
if [ -S "$SOCKET_PATH" ]; then
    log "removing stale socket at ${SOCKET_PATH}"
    rm -f "$SOCKET_PATH" 2>/dev/null || log "WARNING: could not remove stale socket ${SOCKET_PATH}"
fi

# 3. Initialize the token if it is not already initialized (idempotent), before the server binds so
# the module is loaded with the token present. Set P11_INIT=0 to disable. Only softhsm and tpm2
# modules are auto-initialized.
case "${P11_INIT:-auto}" in
    0|off) ;;
    *)
        if [ -x /usr/local/bin/init-hsm.sh ]; then
            /usr/local/bin/init-hsm.sh || log "WARNING: token init failed (continuing)"
        fi
        ;;
esac

# 4. Run the server. umask 0002 keeps the socket group-writable, so a client sharing the tedge
# group (but not the exact uid) can also connect.
umask 0002
log "starting tedge-p11-server as uid $(id -u):$(id -g) (groups: $(id -G)), socket: ${SOCKET_PATH}"
exec "$@"
