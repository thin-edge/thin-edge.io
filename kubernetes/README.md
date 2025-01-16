# thin-edge.io on Kubernetes

:construction: This is a work-in-progress so don't expect everything to work out of the box in all situations. Contributions are welcomed to improve the setup.

This document describes the steps required to get thin-edge.io deployed in a Kubernetes (k8s) cluster.

## Pre-requisites

The following pre-requisites are required to run the example:

* A kubernetes cluster - [k3s](https://k3s.io/) is a good option for a single node cluster if you're just trying things out locally
* kubectl - cli to interact with kubernetes
* [helm](https://helm.sh/)

Kubernetes know-how is also assumed, so if you have trouble with setting up Kubernetes, please consult the public Kubernetes documentation or seek help in community forums (e.g. stack overflow etc.).

## Getting started

The following guide should help you install thin-edge.io as a helm chart.

1. Create the device certificate key pair

    **Option 1: Create self-signed certificates using tedge cli**

    ```sh
    mkdir device-certs
    tedge --config-dir $(pwd) cert create --device-id mydevice001
    ```

    The certificates will be stored under the `./device-certs` folder.

    Alternatively, you can also create device certificates using a CA and openssl. Please consult the openssl documentation on how to do this.

1. Add device certificate to Cumulocity's Trusted Certificates

    You can uploaded it to Cumulocity using the Device Management UI, or using the go-c8y-cli tool:

    ```sh
    c8y devicemanagement certificates create --name mydevice001 --file ./device-certs/tedge-certificate.pem --status ENABLED --autoRegistrationEnabled
    ```

1. Create a kubernetes namespace where the helm chart will be deployed to

    ```sh
    kubectl create namespace tedge
    ```

1. Create a Secret for the device certificate key pair

    ```sh
    kubectl create secret generic tedge-certs --from-file=./device-certs/tedge-certificate.pem --from-file=./device-certs/tedge-private-key.pem --namespace tedge
    ```

1. Install the thin-edge.io helm chart

    ```sh
    helm install tedge-chart ./tedge --set c8y.url=example.eu-latest.cumulocity.com --namespace tedge
    ```

    Change the `c8y.url=` value to the MQTT endpoint of your Cumulocity tenant.

1. Validate the device created in Cumulocity

    If you are using go-c8y-cli, then you can open the Device Management application to your device using the following command:

    ```sh
    c8y identity get --name mydevice001 | c8y applications open
    ```

1. The mosquitto broker can be accessed by other pods in the cluster via the `mosquitto.<namespace>.svc.cluster.local` endpoint

    **Using an existing service**

    The MQTT service endpoint can be verified from within a deployment using the following command:

    ```sh
    kubectl exec -n tedge -it service/mosquitto -- mosquitto_sub -h 'mosquitto.tedge.svc.cluster.local' -p 1883 -t '#' -W 5 -v
    ```

    **Using a new pod**

    The MQTT service endpoint can also be verified from other pods by creating a test pod, and executing a command from it.

    ```sh
    kubectl run -n tedge --restart=Never --image eclipse-mosquitto tedge-test -- sleep infinity
    kubectl exec -n tedge -it pod/tedge-test -- mosquitto_sub -h 'mosquitto.tedge.svc.cluster.local' -p 1883 -t '#' -W 3 -v
    kubectl delete -n tedge pod/tedge-test
    ```

    ```sh
    te/device/main/service/mosquitto-c8y-bridge {"@id":"example001:device:main:service:mosquitto-c8y-bridge","@parent":"device/main//","@type":"service","name":"mosquitto-c8y-bridge","type":"service"}
    te/device/main/service/mosquitto-c8y-bridge/status/health 1
    te/device/main/service/tedge-mapper-c8y {"@parent":"device/main//","@type":"service","type":"service"}
    te/device/main/service/tedge-mapper-c8y/status/health {"pid":1,"status":"up","time":1710236345.2828279}
    te/device/main/service/tedge-agent {"@parent":"device/main//","@type":"service","type":"service"}
    te/device/main/service/tedge-agent/status/health {"pid":1,"status":"up","time":1710236338.9187257}
    te/device/main///twin/c8y_Agent {"name":"thin-edge.io","url":"https://thin-edge.io","version":"1.0.1"}
    te/device/main///cmd/config_snapshot {"types":["tedge-configuration-plugin","tedge-log-plugin","tedge.toml"]}
    te/device/main///cmd/config_update {"types":["tedge-configuration-plugin","tedge-log-plugin","tedge.toml"]}
    te/device/main///cmd/log_upload {"types":["software-management"]}
    te/device/main///cmd/restart {}
    te/device/main///cmd/software_list {}
    te/device/main///cmd/software_update {}
    Timed out
    command terminated with exit code 27
    ```

## Uninstall helm chart

You can uninstall the helm chart using the following command:

```
helm uninstall tedge-chart --namespace tedge
```

## Project structure

Key points about the deployment design are listed below:

* The thin-edge.io helm chart consists of two deployments:
    * a multi-container pod with `mosquitto` broker and `tedge-mapper` along with `tedge-bootstrap` init container
      that performs the bootstrapping
    * an independent deployment of `tedge-agent`
* The `mosquitto` broker and `tedge-mapper` are deployed in the same pod with `/etc/tedge` directory mounted as a shared volume,
  so that the mapper and the broker can access the bridge configurations created by the `tedge-bootstrap` container.
* A custom mosquitto configuration is used for the `mosquitto` container so that it picks up configuration extensions
  from the `/etc/tedge/mosquitto-conf` directory where the bridge configurations are created.
* The mosquitto broker from the first deployment is exposed as a service so that the `tedge-agent` deployment can access it.
  Similarly, the file-transfer-service of the `tedge-agent` is also exposed as a service for the mapper to use it.
* The config settings required by the thin-edge.io components are provided via env variables.
* The device certificate key pair generated and provided to the cluster as a `Secret`.
* The persistent volume for the shared `/etc/tedge` directory is a local volume.

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
