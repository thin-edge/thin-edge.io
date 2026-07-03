# tedge-p11-server container

Runs the [`tedge-p11-server`](../../docs/src/references/tedge-p11-server.md) PKCS#11 signing service
in its own container, so that a containerized (or otherwise isolated) `tedge` can perform
HSM-backed MQTT client authentication **without installing `tedge-p11-server` on the host**.

The HSM on the host is made available to this container via device passthrough, and `tedge`
connects to the service over a UNIX socket shared between the two containers.

## How it works

```
┌──────────────────────┐   UNIX socket    ┌────────────────────────────┐   PKCS#11    ┌─────────┐
│ tedge container      │◄────(volume)────►│ tedge-p11-server container │◄───module───►│  HSM    │
│ device.cryptoki.mode │                  │  (device passthrough)      │              │ (host)  │
│   = socket           │                  │                            │              │         │
└──────────────────────┘                  └────────────────────────────┘              └─────────┘
```

The socket created by `tedge-p11-server` (mode `0660`/`0775`, owned by its `tedge` user) is usable
by the `tedge` container when the socket directory is shared as a volume **and both containers use
the same uid/gid**. Build this image with `USER_UID`/`USER_GID` matching the tedge client image:
`ghcr.io/thin-edge/tedge-container-bundle` runs as `999:992`, while `ghcr.io/thin-edge/tedge` uses
`1000:1000`. This replaces the host `usermod`/`groupmod` step needed in the host-installed setup.

## Base image: why Debian (glibc), not Alpine (musl)

Unlike the [`tedge`](../tedge) image (Alpine/musl), this image is based on **Debian (glibc)**.
This is not a free choice:

- `tedge-p11-server` `dlopen()`s the PKCS#11 module into its own process, so the **binary and the
  module must share the same libc**.
- The only musl artifact published for `tedge-p11-server` is a *statically linked* binary, which
  **cannot `dlopen()` a module at all**. The usable binary (shipped in the `.deb`/`.apk`) is
  glibc-linked.
- A glibc process can't load Alpine's musl-built modules, and a libc shim like `gcompat` doesn't
  bridge this (it can't make a glibc-linked module loadable in a musl process). So both the binary
  and the modules have to be glibc → Debian.

## Variants (image size)

The bundled PKCS#11 module is selected at build time with the `VARIANT` build-arg, so you only ship
the module for the hardware you actually use. Approximate pull sizes (arm64):

| `VARIANT` | Modules included                          | Module path                              | ~pull size |
| --------- | ----------------------------------------- | ---------------------------------------- | ---------- |
| `minimal` | none (mount the module at runtime)        | *(provide your own)*                     | ~31 MB     |
| `usb`     | OpenSC + pcscd (USB tokens)               | `/usr/lib/tedge-p11/opensc-pkcs11.so`    | ~35 MB     |
| `tpm`     | tpm2-pkcs11 (TPM 2.0)                     | `/usr/lib/tedge-p11/libtpm2_pkcs11.so`   | ~37 MB     |
| `softhsm` | SoftHSM2 + pkcs11-tool (testing)          | `/usr/lib/tedge-p11/libsofthsm2.so`      | ~36 MB     |
| `full`    | all of the above                          | any of the above                         | ~41 MB     |

> Note: `docker images` on some setups (e.g. colima/buildkit) reports a much larger figure than the
> real image because it also counts build-cache and attestation layers. Use
> `docker save <image> | gzip -c | wc -c` for the true pull size.

Modules are exposed at stable, arch-independent symlinks under `/usr/lib/tedge-p11/`.

## Published images

CI builds and publishes one multi-arch image per variant, tagged with a `-<variant>` suffix. Pull
the one for your hardware, e.g.:

```sh
docker pull ghcr.io/thin-edge/tedge-p11-server:latest-tpm      # released
docker pull ghcr.io/thin-edge/tedge-p11-server-main:latest-tpm # latest from main
```

(`-main` mirrors the `tedge-main` image for pre-release builds.) The `docker-compose.*.yaml`
examples in this directory `build:` from source instead; swap `build:` for the published `image:` if
you'd rather not build locally.

## Building from source

The image installs the `tedge-p11-server` **Debian package** from `./packages/`. Copy the matching
`.deb` (same set produced for the `tedge` image) into that directory first, then pick a variant:

```sh
cp ../tedge/packages/tedge-p11-server_*_arm64.deb ./packages/

# USB token image
docker build --platform linux/arm64 --build-arg VARIANT=usb -t tedge-p11-server:usb .

# TPM 2.0 image
docker build --platform linux/arm64 --build-arg VARIANT=tpm -t tedge-p11-server:tpm .
```

`./packages/` is git-ignored (see the repo `.gitignore`). CI does the same thing, feeding all the
target `.deb`s in via the [build workflow](../../.github/workflows/build-workflow.yml).

## Running

There is a complete, runnable example per HSM type, each wiring this service to a `tedge`
(container-bundle) client over the shared socket. They follow the tedge-container-bundle
[Container network with a HSM guide][hsm-guide], but run `tedge-p11-server` in its own non-root
container instead of on the host:

| Example | HSM | Notes |
| ------- | --- | ----- |
| [`docker-compose.tpm.yaml`](./docker-compose.tpm.yaml)        | TPM 2.0             | passes `/dev/tpmrm0`, `group_add` the `tss` gid |
| [`docker-compose.usb.yaml`](./docker-compose.usb.yaml)        | USB (Nitrokey, ...) | passes `/dev/bus/usb`, `group_add` the reader gid, in-container pcscd |
| [`docker-compose.softhsm.yaml`](./docker-compose.softhsm.yaml)| SoftHSM2 (testing)  | no device/group; software token |

All three were validated on a Raspberry Pi 4 (TPM: Infineon SLB9672; USB: Nitrokey HSM 2). The
p11-server image runs as non-root `999:992` to match the bundle client, so the shared socket needs no
host `usermod`/`groupmod`. Bring one up with, e.g.:

```sh
cp ../tedge/packages/tedge-p11-server_*.deb ./packages/
export TEDGE_C8Y_URL=example.c8y.cumulocity.com
docker compose -f docker-compose.softhsm.yaml up --build -d   # or .tpm.yaml / .usb.yaml
```

Then enroll the device once (see each file's header for the exact commands): create the key on the
token, create the device certificate, and upload it to Cumulocity.

[hsm-guide]: https://github.com/thin-edge/tedge-container-bundle/blob/main/docs/CONTAINER_OPTION2_with_hsm.md

Configuration is supplied via `TEDGE_DEVICE_CRYPTOKI_*` environment variables (or CLI args appended
to the container command):

Every variant ships sensible defaults, so a typical deployment sets **no** `tedge-p11-server`
environment at all — just pass the device and share the socket volume. Each setting below is
optional and only needs overriding for non-default setups:

| Variable                            | Default | Purpose                                             |
| ----------------------------------- | ------- | --------------------------------------------------- |
| `TEDGE_DEVICE_CRYPTOKI_MODULE_PATH` | this variant's bundled module (`usb`→opensc, `tpm`→tpm2, `softhsm`→softhsm2, `full`→tpm2; `minimal`→none) | PKCS#11 module to load |
| `TEDGE_DEVICE_CRYPTOKI_SOCKET_PATH` | `/run/tedge-p11-server/tedge-p11-server.sock` | socket location |
| `TEDGE_DEVICE_CRYPTOKI_PIN`         | `123456` | default/user PIN (also used to log in for auto-init) |
| `TEDGE_DEVICE_CRYPTOKI_URI`         | —       | token/object URI filter (RFC 7512)                  |
| `P11_START_PCSCD`                   | `auto`  | `auto` / `1` / `0` — start `pcscd` for USB tokens (no-op unless `/dev/bus/usb` is present) |
| `P11_INIT`                          | `auto`  | `auto` / `0` — initialize the token at startup if missing |
| `P11_TOKEN_LABEL`                   | `tedge` | token label to create/use                           |
| `P11_SO_PIN`                        | `12345678` | security-officer PIN for token init              |

The `minimal` variant is the exception: it bundles no module, so you must mount one and set
`TEDGE_DEVICE_CRYPTOKI_MODULE_PATH`. For `full` (which bundles all modules) the default is the TPM
module; override it for USB/SoftHSM. Remember to change `TEDGE_DEVICE_CRYPTOKI_PIN`/`P11_SO_PIN`
from the defaults for production.

### Token initialization

There are two distinct steps to prepare an HSM:

1. **Token initialization** — create the token/slot and set its PIN. This must be done where the
   module and device live, i.e. **in this container**. On startup the entrypoint runs
   [`init-hsm.sh`](./init-hsm.sh) (unless `P11_INIT=0`), which **idempotently** initializes a token
   labelled `$P11_TOKEN_LABEL` if one does not already exist:
   - **TPM 2.0**: `p11tool --initialize` + `--initialize-pin` (bundled `gnutls-bin`), against the
     passed-through TPM (e.g. `/dev/tpmrm0`). This mirrors
     [`configuration/contrib/pkcs11/tedge-init-hsm.sh`](../../configuration/contrib/pkcs11/tedge-init-hsm.sh),
     the host/systemd equivalent.
     For TPM the tedge user also needs access to the passed-through device node — see
     [TPM 2.0 device access](#tpm-20-device-access) below.
   - **SoftHSM2**: `softhsm2-util --init-token`.
   - **USB tokens**: not auto-initialized (initialization is device-specific and can be destructive).

2. **Key creation** — done by the tedge *client* with `tedge cert create-key-hsm`, whose request is
   proxied to this server over the socket (the server has direct module access and generates the key
   on the token). This requires the token from step 1 to already exist. After creating the key,
   create/upload the device certificate (`tedge cert create-csr` / `tedge cert upload c8y`).

To provision the token/key outside the container instead, set `P11_INIT=0` and manage the store
(the `p11-hsm-store` volume → `/etc/tedge/hsm`, i.e. `TPM2_PKCS11_STORE`) yourself.

### TPM 2.0 device access

The image runs **as the non-root `tedge` user** (uid/gid `999:992`) — there is no root and no
privilege dropping. The socket therefore matches the tedge client's uid/gid, and access to the TPM
is granted the same way you'd grant any non-root process access to a device: put the process in the
device node's group. Two things have to line up:

**1. On the host: the device node must be group-accessible.**
On Debian/Ubuntu the `tpm-udev` package already ships
`/usr/lib/udev/rules.d/60-tpm-udev.rules`, which sets `/dev/tpmrm0` to `tss:tss`, mode `0660` — so
**no custom udev rule is needed**. You only need to add one (e.g. `/etc/udev/rules.d/tpm.rules`) on
a minimal system where `tpm-udev` is absent and the node comes up `root:root 0600`. Check with
`ls -l /dev/tpmrm0`.

**2. In the container: add the device's group as a supplementary group.** Pass the device and add
the host gid that owns `/dev/tpmrm0` (the `tss` group) with `group_add` (docker/compose):
```sh
docker run --device /dev/tpmrm0 --group-add "$(stat -c %g /dev/tpmrm0)" ... tedge-p11-server:tpm
```
In compose that's the `group_add: ["114"]` entry (see [`docker-compose.tpm.yaml`](./docker-compose.tpm.yaml)).
Find the gid with `stat -c %g /dev/tpmrm0` or `getent group tss` — it's `114` on Debian/Ubuntu by
default but is host-specific. The token store (`TPM2_PKCS11_STORE`, the `p11-hsm-store` volume) also
has to be writable by uid/gid `999:992` — named volumes inherit that from the image; for bind mounts
pre-`chown` them (or use Kubernetes `fsGroup`).

### USB tokens (non-root)

USB tokens work non-root too, using the **same pattern** as the TPM — `pcscd` does not actually need
root. The image runs `pcscd` inside the `usb`/`full` variant as the `tedge` user (it only needs the
pre-created, writable `/run/pcscd`, which the image ships). What you provide from outside is USB
device access:

1. **Host udev rule** giving a group read/write to the reader, e.g. for a NitroKey (adjust the
   vendor id / group):
   ```
   # /etc/udev/rules.d/99-hsm-usb.rules
   SUBSYSTEM=="usb", ATTRS{idVendor}=="20a0", MODE="0660", GROUP="plugdev"
   ```
2. **Pass the device** into the container and **`group_add`** the reader's group:
   ```yaml
   # docker-compose.usb.yaml (tedge-p11-server service)
   devices:
     - /dev/bus/usb:/dev/bus/usb
   group_add:
     - "<plugdev-gid>"     # gid of the reader's group on the host
   ```

The container then runs `pcscd` as `tedge`, which claims the reader via the group-accessible USB
node — no root anywhere. (Alternatively, run `pcscd` on the host and bind-mount `/run/pcscd` into the
container; then the container needs neither the USB device nor `pcscd` itself.)

> **CRITICAL — only one `pcscd` may own the reader.** A USB CCID smartcard gives *exclusive* access,
> so the container's `pcscd` cannot use the reader if the **host** also runs `pcscd`. Many distros
> ship `pcscd.socket` enabled, which starts a host `pcscd` on demand (often at boot) that claims the
> token — this is the usual cause of *"works at first, but after a reboot tedge-p11-server can't find
> the key"*. Fix it by disabling the host daemon:
> ```sh
> sudo systemctl disable --now pcscd.socket pcscd.service
> ```
> (Or the reverse: keep the host `pcscd` and bind-mount `/run/pcscd` into the container with
> `P11_START_PCSCD=0`, so the container uses the host daemon instead of its own.) Likewise, don't run
> other PC/SC clients (`opensc-tool`, `pkcs11-tool`, `pcsc_scan`) against the reader while the server
> is using it — power-cycling the exclusive card can leave it unresponsive (`Card: No`) until it is
> physically re-plugged. The container is deliberately hands-off: it starts `pcscd` once and lets
> tedge-p11-server be the sole user of the card.

### Kubernetes

This non-root model maps directly onto Kubernetes:

- **Exposing the device.** A `hostPath` mount of `/dev/tpmrm0` alone is usually blocked by the
  pod's device cgroup. Use a **TPM device plugin** (which sets the cgroup allow and can advertise the
  device), or, as a coarse fallback, `securityContext.privileged: true`.
- **Group access.** `supplementalGroups` is the Kubernetes equivalent of `group_add`:
  ```yaml
  securityContext:
    runAsUser: 999          # tedge
    runAsGroup: 992
    supplementalGroups: [114]   # host gid owning /dev/tpmrm0 (the `tss` group)
    fsGroup: 992                # so the token-store volume is writable
  ```
  Mount a `PersistentVolumeClaim` at `/etc/tedge/hsm`
  to persist the token store, and share an `emptyDir` at `/run/tedge-p11-server` with the tedge
  container for the socket.

### Trying it with SoftHSM2 (no hardware)

Build the `softhsm` variant (`--build-arg VARIANT=softhsm`), which bundles SoftHSM2 and
`pkcs11-tool`. SoftHSM needs no device, so it runs with no extra options — the container
auto-selects the module and initializes the token on startup:

```sh
# start the server (runs as non-root tedge; token auto-initialized)
docker run -d --name tedge-p11-server \
  -v p11store:/etc/tedge/hsm \
  -v p11-socket:/run/tedge-p11-server \
  tedge-p11-server:softhsm

# create the device key on the token (as the tedge user; no gosu/root needed)
docker exec tedge-p11-server \
  pkcs11-tool --module /usr/lib/tedge-p11/libsofthsm2.so --login --pin 123456 \
    --keypairgen --key-type EC:prime256v1 --label my-key --id 01
```

Then point `tedge` at it with `device.cryptoki.mode = socket` and the shared socket path (or let the
tedge client create the key over the socket with `tedge cert create-key-hsm`).
