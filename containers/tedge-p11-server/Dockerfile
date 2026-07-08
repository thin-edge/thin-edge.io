# tedge-p11-server container
#
# Runs the thin-edge.io PKCS#11 signing service in its own container so that a containerized (or
# otherwise isolated) tedge can perform HSM-backed MQTT client authentication without tedge-p11-server
# being installed on the host. The HSM is made available to this container via device passthrough.
#
# The image is Debian based (glibc): tedge-p11-server dlopen()s the PKCS#11 module into its own
# process, so the binary and the module must share the same libc. The published tedge-p11-server
# apk/deb binaries are glibc-linked, and the only musl artifact is a *static* binary which cannot
# dlopen a module at all - so an Alpine/musl image is not an option here.
#
# To keep the image small, the bundled PKCS#11 module is selected at build time with the VARIANT
# build-arg, so you only pay for the hardware you actually use:
#
#   VARIANT=minimal   just the tedge-p11-server binary; provide the PKCS#11 module via a bind mount
#   VARIANT=usb       OpenSC + pcscd for USB smartcard tokens (NitroKey HSM 2, Yubikey, ...)
#   VARIANT=tpm       tpm2-pkcs11 for TPM 2.0
#   VARIANT=softhsm   SoftHSM2 (+ pkcs11-tool) software token, for testing without hardware
#   VARIANT=full      everything above, in a single image
#
# The thin-edge.io binaries are statically linked (musl) for tedge itself, but the tedge-p11-server
# binary shipped in the deb is glibc-linked and runs on glibc-based Debian.

ARG VARIANT=usb

FROM debian:12-slim AS build
# Use a build layer to avoid adding binaries from every target into the output image
ARG TARGETPLATFORM

# Extract the tedge-p11-server binary from the matching Debian package
COPY packages/ /packages/
RUN set -e; \
    # Map the Docker platform to the Rust target that produced the .deb and to the .deb architecture
    # suffix. The arm naming needs care: nfpm maps both armv7 (arm7) and armv6 (arm6) to Debian
    # "armhf", so the build renames the armv6 package to `_armv6.deb` to avoid the clash (see
    # ci/build_scripts/package.sh). Moreover two Rust targets build arm6/`_armv6.deb` - the hard-float
    # `arm-unknown-linux-musleabihf` and the soft-float `arm-unknown-linux-musleabi` - so the filename
    # alone is ambiguous; we disambiguate by the artifact directory (the Rust target). For Docker's
    # linux/arm/v6 (e.g. Raspberry Pi Zero/1) we want the hard-float build.
    case "${TARGETPLATFORM}" in \
        "linux/amd64")  RUST_TARGET=x86_64-unknown-linux-musl;      DEB_ARCH=amd64 ;; \
        "linux/arm64")  RUST_TARGET=aarch64-unknown-linux-musl;     DEB_ARCH=arm64 ;; \
        "linux/arm/v7") RUST_TARGET=armv7-unknown-linux-musleabihf; DEB_ARCH=armhf ;; \
        "linux/arm/v6") RUST_TARGET=arm-unknown-linux-musleabihf;   DEB_ARCH=armv6 ;; \
        *) echo "Unsupported target platform: TARGETPLATFORM=$TARGETPLATFORM"; exit 1 ;; \
    esac; \
    pat="tedge-p11-server_${TEDGE_VERSION:-"*"}_${DEB_ARCH}.deb"; \
    # Prefer the deb from the matching target's artifact directory (CI layout: packages-<target>/...);
    # fall back to a flat layout (local builds, one deb per arch) by arch.
    matches=$(find /packages -path "*${RUST_TARGET}*" -name "$pat"); \
    [ -n "$matches" ] || matches=$(find /packages -name "$pat"); \
    test -n "$matches" || { echo "No tedge-p11-server deb found for ${TARGETPLATFORM} (arch=${DEB_ARCH}, target=${RUST_TARGET})"; exit 1; }; \
    # Any remaining duplicates should be identical; only distinct filenames (different versions) are
    # genuinely ambiguous.
    distinct=$(printf '%s\n' "$matches" | xargs -n1 basename | sort -u); \
    if [ "$(printf '%s\n' "$distinct" | wc -l)" -gt 1 ]; then \
        echo "ERROR: multiple tedge-p11-server versions match ${TARGETPLATFORM}; set TEDGE_VERSION to disambiguate:"; \
        printf '  %s\n' $distinct; exit 1; \
    fi; \
    deb=$(printf '%s\n' "$matches" | head -1); \
    echo "Extracting $deb"; \
    dpkg-deb -x "$deb" /out; \
    ls -l /out/usr/bin/tedge-p11-server

#
# Output
#
FROM debian:12-slim
ARG VARIANT

# Keep the tedge uid/gid fixed and aligned with the thin-edge.io (tedge) image so that the
# UNIX socket created by tedge-p11-server can be shared with the tedge container via a volume.
ARG USERNAME=tedge
ARG USER_UID=999
ARG USER_GID=992

ARG BUILDTIME
ARG REVISION
ARG VERSION

LABEL org.opencontainers.image.title="tedge-p11-server"
LABEL org.opencontainers.image.created=$BUILDTIME
LABEL org.opencontainers.image.version=$VERSION
LABEL org.opencontainers.image.revision=$REVISION
LABEL org.opencontainers.image.description="thin-edge.io PKCS#11 signing service for accessing host HSMs from a container (variant: ${VARIANT})"
LABEL org.opencontainers.image.source="https://github.com/thin-edge/thin-edge.io"
LABEL org.opencontainers.image.authors="thin-edge.io"
LABEL org.opencontainers.image.vendor="thin-edge.io"
LABEL org.opencontainers.image.licenses="Apache 2.0"
LABEL org.opencontainers.image.url="https://thin-edge.io"
LABEL org.opencontainers.image.documentation="https://thin-edge.github.io/thin-edge.io/"
LABEL org.opencontainers.image.base.name="debian:12-slim"

# Common tools plus the VARIANT-specific PKCS#11 modules and drivers:
#   usb     -> opensc (opensc-pkcs11.so) + pcsc-lite/ccid (USB reader driver + daemon)
#   tpm     -> libtpm2-pkcs11-1 (libtpm2_pkcs11.so)
#   softhsm -> softhsm2 (libsofthsm2.so) + opensc tools (pkcs11-tool) for testing
#   full    -> all of the above
#   minimal -> nothing extra (module provided at runtime via a bind mount)
# Docs/man/locales are stripped afterwards to keep the image small.
RUN set -e; \
    apt-get update; \
    common="ca-certificates"; \
    # gnutls-bin provides p11tool, used by init-hsm.sh to initialize the TPM 2.0 token (+3 MB).
    case "${VARIANT}" in \
      minimal) pkgs="" ;; \
      usb)     pkgs="opensc opensc-pkcs11 libccid pcscd" ;; \
      tpm)     pkgs="libtpm2-pkcs11-1 gnutls-bin" ;; \
      softhsm) pkgs="softhsm2 opensc opensc-pkcs11" ;; \
      full)    pkgs="opensc opensc-pkcs11 libccid pcscd libtpm2-pkcs11-1 softhsm2 gnutls-bin" ;; \
      *) echo "Unsupported VARIANT: ${VARIANT} (expected: minimal|usb|tpm|softhsm|full)"; exit 1 ;; \
    esac; \
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $common $pkgs; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/* /var/cache/debconf/* \
           /usr/share/doc/* /usr/share/man/* /usr/share/info/* \
           /usr/share/locale/*/LC_MESSAGES /usr/share/lintian/*

# install tedge-p11-server
COPY --from=build /out/usr/bin/tedge-p11-server /usr/bin/

# Create stable, arch-independent symlinks for whichever PKCS#11 modules are present (the Debian
# packages install them under a multiarch directory such as /usr/lib/x86_64-linux-gnu/), and record
# this variant's *intended* default module so the entrypoint can use it without any configuration.
# Note: the softhsm variant also ships opensc (for pkcs11-tool), so the default can't be inferred by
# scanning - it must be pinned per variant here.
RUN mkdir -p /usr/lib/tedge-p11 \
    && for m in \
         "$(find /usr/lib -name 'opensc-pkcs11.so' 2>/dev/null | head -1):opensc-pkcs11.so" \
         "$(find /usr/lib -name 'libtpm2_pkcs11.so' 2>/dev/null | head -1):libtpm2_pkcs11.so" \
         "$(find /usr/lib -name 'libsofthsm2.so' 2>/dev/null | head -1):libsofthsm2.so"; do \
         src="${m%%:*}"; dst="${m##*:}"; \
         [ -n "$src" ] && ln -sf "$src" "/usr/lib/tedge-p11/$dst" || true; \
       done \
    && case "${VARIANT}" in \
         usb)     def=opensc-pkcs11.so ;; \
         tpm)     def=libtpm2_pkcs11.so ;; \
         softhsm) def=libsofthsm2.so ;; \
         full)    def=libtpm2_pkcs11.so ;; \
         *)       def= ;; \
       esac; \
       [ -n "$def" ] && echo "/usr/lib/tedge-p11/$def" > /usr/lib/tedge-p11/.default-module || true \
    && ls -l /usr/lib/tedge-p11/ || true

# setup user, config-dir, socket dir and pcscd runtime dir
RUN groupadd -g "$USER_GID" "$USERNAME" \
    && useradd -u "$USER_UID" -g "$USER_GID" -M -N -s /usr/sbin/nologin "$USERNAME" \
    && mkdir -p /etc/tedge/hsm/tokens /run/tedge-p11-server /run/pcscd \
    # Point SoftHSM2 at a token store writable by the tedge user (default /var/lib/softhsm is
    # root-owned). Only relevant for the softhsm/full variants. The config lives outside
    # /etc/tedge/hsm so it isn't shadowed when that dir is a mounted volume.
    && printf 'directories.tokendir = /etc/tedge/hsm/tokens\nobjectstore.backend = file\nlog.level = INFO\n' > /etc/tedge/softhsm2.conf \
    # /run/pcscd is pre-created and owned by tedge so the USB variant can run pcscd without root
    # (pcscd only needs a writable runtime dir + access to the USB device node).
    && chown -R "$USER_UID:$USER_GID" /etc/tedge /run/tedge-p11-server /run/pcscd

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
COPY init-hsm.sh /usr/local/bin/init-hsm.sh
RUN chmod +x /usr/local/bin/entrypoint.sh /usr/local/bin/init-hsm.sh

# Where the signing service exposes its UNIX socket. Share this directory with the tedge
# container (e.g. a named volume mounted at the same path in both containers).
ENV TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH=/run/tedge-p11-server/tedge-p11-server.sock

# TPM 2.0 store location (used by tpm2-pkcs11 via libtpm2_pkcs11.so)
ENV TPM2_PKCS11_STORE=/etc/tedge/hsm

# SoftHSM2 config/token store (only used with the softhsm/full variants, for testing)
ENV SOFTHSM2_CONF=/etc/tedge/softhsm2.conf

# pcscd is only relevant to USB tokens. It runs non-root inside the container (it just needs the
# pre-created writable /run/pcscd + USB device access via group_add). "auto" starts it when a USB
# device tree is passed through; "0" disables it. See the README ("USB tokens").
ENV P11_START_PCSCD=auto

# Initialize the PKCS#11 token at startup if not already initialized (idempotent). Only the
# softhsm and tpm2 modules are auto-initialized; set to "0"/"off" to disable. See init-hsm.sh.
ENV P11_INIT=auto
ENV P11_TOKEN_LABEL=tedge

# Run as the non-root tedge user (security-first). Access to a passed-through HSM device is granted
# by adding the host's device group as a supplementary group (`group_add` / `supplementalGroups`),
# not by running as root. The uid/gid must match the tedge client image (see USER_UID/USER_GID).
USER $USER_UID:$USER_GID

ENTRYPOINT [ "/usr/local/bin/entrypoint.sh" ]
CMD [ "/usr/bin/tedge-p11-server" ]
