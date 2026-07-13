# thin-edge.io on Kubernetes

:construction: This is a work-in-progress so don't expect everything to work out of the box in all situations. Contributions are welcomed to improve the setup.

This document describes the steps required to get thin-edge.io deployed in a Kubernetes (k8s) cluster.

## Pre-requisites

The following pre-requisites are required to run the example:

* A kubernetes cluster - [k3s](https://k3s.io/) is a good option for a single node cluster if you're just trying things out locally. The chart's persistent volumes default to the `ReadWriteOncePod` access mode so Kubernetes enforces that only one pod ever mounts them (preventing concurrent access to the agent/certificate files); this requires k8s >= 1.22 (GA in 1.29). On older clusters, override `persistence.accessMode` and `certs.accessMode` back to `ReadWriteOnce`.
* kubectl - cli to interact with kubernetes
* [helm](https://helm.sh/)

Kubernetes know-how is also assumed, so if you have trouble with setting up Kubernetes, please consult the public Kubernetes documentation or seek help in community forums (e.g. stack overflow etc.).

## Getting started

This guide installs thin-edge.io as a helm chart using the default certificate
flow: the device certificate is downloaded automatically at startup using a
Cumulocity one-time password, so you don't need to create it beforehand. By
default the chart generates a random one-time password (preserved across
`helm upgrade`) and shows how to retrieve it, and the registration URL, in its
post-install notes.

1. Create a kubernetes namespace where the helm chart will be deployed to

    ```sh
    kubectl create namespace tedge
    ```

1. Install the thin-edge.io helm chart

    ```sh
    helm upgrade --install tedge ./tedge \
        --set c8y.url=$C8Y_DOMAIN \
        --set device.id=my-device-001 \
        --namespace tedge
    ```

    Change the `c8y.url` value to the MQTT endpoint of your Cumulocity tenant.

1. Register the device using the one-time password

    The chart stores the generated password in the `tedge-cert-otp` Secret.
    Retrieve it (the Helm post-install notes print this command too):

    ```sh
    kubectl get secret tedge-cert-otp -n tedge -o jsonpath="{.data['one-time-password']}" | base64 -d; echo
    ```

    Or print the full Cumulocity device-registration URL:

    ```sh
    OTP=$(kubectl get secret tedge-cert-otp -n tedge -o jsonpath="{.data['one-time-password']}" | base64 -d)
    printf "\n\thttps://$C8Y_DOMAIN/apps/devicemanagement/index.html#/deviceregistration?externalId=my-device-001&one-time-password=$OTP\n\n"
    ```

    To pin a specific password instead (for example to pre-register the device),
    see [Alternative deployment options](#alternative-deployment-options).

1. Validate the device created in Cumulocity

    The `tedge` pod reports `Ready` only once it holds a device certificate, so
    `kubectl get pods` (and the Helm post-install notes) reflect whether the
    device has registered.

    If you are using go-c8y-cli, then you can open the Device Management application to your device using the following command:

    ```sh
    c8y identity get --name my-device-001 | c8y applications open
    ```

1. (Optional) Verify the MQTT broker

    The mosquitto broker can be accessed by other pods in the cluster via the `mqtt.<namespace>.svc.cluster.local` endpoint.

    **Using an existing service**

    The MQTT service endpoint can be verified from within a deployment using the following command:

    ```sh
    kubectl exec -n tedge -it service/mqtt -- mosquitto_sub -h 'mqtt.tedge.svc.cluster.local' -p 1883 -t '#' -W 5 -v
    ```

    **Using a new pod**

    The MQTT service endpoint can also be verified from other pods by creating a test pod, and executing a command from it.

    ```sh
    kubectl run -n tedge --restart=Never --image eclipse-mosquitto tedge-test -- sleep infinity
    kubectl exec -n tedge -it pod/tedge-test -- mosquitto_sub -h 'mqtt.tedge.svc.cluster.local' -p 1883 -t '#' -W 3 -v
    kubectl delete -n tedge pod/tedge-test
    ```

    ```sh
    te/device/main/service/mosquitto-c8y-bridge {"@id":"example001:device:main:service:mosquitto-c8y-bridge","@parent":"device/main//","@type":"service","name":"mosquitto-c8y-bridge","type":"service"}
    te/device/main/service/mosquitto-c8y-bridge/status/health 1
    te/device/main/service/tedge-mapper-c8y {"@parent":"device/main//","@type":"service","type":"service"}
    te/device/main/service/tedge-mapper-c8y/status/health {"pid":1,"status":"up","time":1710236345.2828279}
    te/device/main/service/tedge-agent {"@parent":"device/main//","@type":"service","type":"service"}
    te/device/main/service/tedge-agent/status/health {"pid":1,"status":"up","time":1710236338.9187257}
    te/device/main///twin/c8y_Agent {"name":"thin-edge.io","url":"https://thin-edge.io","version":"2.0.1"}
    te/device/main///cmd/config_snapshot {"types":["tedge-configuration-plugin","tedge-log-plugin","tedge.toml"]}
    te/device/main///cmd/config_update {"types":["tedge-configuration-plugin","tedge-log-plugin","tedge.toml"]}
    te/device/main///cmd/log_upload {"types":["software-management"]}
    te/device/main///cmd/restart {}
    te/device/main///cmd/software_list {}
    te/device/main///cmd/software_update {}
    Timed out
    command terminated with exit code 27
    ```

## Alternative deployment options

The [Getting started](#getting-started) guide above uses the default certificate
flow (`pvc` mode with a generated one-time password). This section covers the
alternatives. The chart supports two certificate sources via `certs.source`:

* `pvc` (default): the certificate is stored on a writable volume, so startup can download it (using a one-time password) or renew it automatically.
* `secret`: a certificate you created beforehand is mounted read-only from a Kubernetes Secret. In this mode the container does not attempt to download or renew the certificate.

### Pin a specific one-time password (pvc mode)

By default the one-time password is generated randomly. To pin a specific value
instead — for example to pre-register the device — set `certs.oneTimePassword`:

```sh
helm upgrade --install tedge ./tedge \
    --namespace tedge \
    --set c8y.url=$C8Y_DOMAIN \
    --set device.id=my-device-001 \
    --set certs.oneTimePassword="$C8Y_CERT_OTP"
```

### Use an existing certificate from a Secret (secret mode)

Provide a pre-created device certificate via a Kubernetes Secret:

```sh
c8y devices enroll --id my-device-001 --key ./tedge-private-key.pem --cert tedge-certificate.pem

kubectl create secret generic tedge-certs \
    --from-file=./tedge-certificate.pem \
    --from-file=./tedge-private-key.pem \
    --namespace tedge

helm upgrade --install tedge ./tedge \
    --namespace tedge \
    --set c8y.url=$C8Y_DOMAIN \
    --set certs.source=secret
```

The Secret defaults to the name `tedge-certs`; override it with
`certs.secretName` if you use a different name. You can also create the
certificate key pair using a CA and openssl instead of the tedge CLI.

## Certificate renewal

`tedge cert renew` re-uses the existing private key and requests a fresh
certificate, so only the public certificate changes. thin-edge does not renew
on a schedule by itself — renewal has to be triggered.

* **`pvc` mode (default):** the certificate lives on a writable volume, so it can
  be renewed in place. The tedge container runs a background loop (enabled by
  default) that periodically runs `tedge cert renew` before the certificate
  expires and then signals the main tedge process (SIGHUP) to reload the new
  certificate. Because it runs inside the same pod, only that pod ever
  writes to the certificate volume — no second pod, no kubectl/RBAC, and no
  multi-node mount issues. Configure it via `certs.renewal.enabled` and
  `certs.renewal.intervalSeconds`.
* **`secret` mode:** the certificate Secret is mounted read-only and a renewed
  certificate written in-pod would not propagate back to the Secret, so the
  chart does not attempt renewal. You must renew externally — e.g. a job that
  renews the certificate and updates the Secret via the Kubernetes API (which
  needs RBAC to `patch` the Secret), then triggers a rollout so the pod picks up
  the new certificate. For long-lived devices, prefer `pvc` mode.

## Uninstall helm chart

You can uninstall the helm chart using the following command:

```sh
helm uninstall tedge --namespace tedge
```

## Project structure

Key points about the deployment design are listed below:

* The chart deploys two workloads:
    * an `mqtt` deployment running the `mosquitto` broker, exposed as the `mqtt`
      service (port 1883) for local MQTT communication
    * a `tedge` deployment running `tedge run all c8y` in a single container,
      which starts the tedge-agent, the Cumulocity mapper (with its built-in
      Cumulocity bridge), and the other built-in services
* The `tedge` container connects to the `mosquitto` broker via the `mqtt`
  service and connects out to Cumulocity directly using the device certificate,
  so no separate mosquitto bridge configuration is required.
* The `tedge` deployment exposes its Cumulocity proxy (port 8001) and
  file-transfer-service (port 8000) as the `tedge` service.
* Configuration is provided to the container via `TEDGE_*` environment variables.
* The device certificate is either downloaded at startup using a Cumulocity
  one-time password and stored on a PersistentVolumeClaim (`certs.source=pvc`,
  the default), or mounted read-only from a Kubernetes Secret
  (`certs.source=secret`). The one-time password is held in the chart-managed
  `tedge-cert-otp` Secret.
* Agent data (and, in `pvc` mode, the certificate) is persisted on
  PersistentVolumeClaims. By default the chart also creates hostPath-backed
  PersistentVolumes, which suits a single-node cluster.
* The `tedge` pod reports `Ready` only once it holds a device certificate; in
  `pvc` mode a background loop in the container renews the certificate before it
  expires and signals the process to reload it.

## Misc.

### Using dedicated colima instance on MacOS

If you want to use a dedicated k3s single node cluster on MacOS, then you can use colima to create a dedicated environment (different from the default colima instance).

**Containerd runtime**

You can create a colima instance using containerd as the runtime using the following command:

```sh
colima start k3s --runtime containerd --kubernetes
```

Afterwards you can configure kubectl to use the new context:

```sh
kubectl config use-context colima-k3s
```
