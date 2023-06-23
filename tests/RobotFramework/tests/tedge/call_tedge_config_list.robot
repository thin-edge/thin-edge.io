*** Settings ***
Documentation       Purpose of this test is to verify that the tedge config list and tedge config list --all
...                 will result with Return Code 0
...                 Set new device type and return to default value
...                 Set new kay path and return to default value
...                 Set new cert path and return to default value
...                 Set new c8y.root_cert_path and return to default value
...                 Set new c8y.smartrest.templates and return to default value
...                 Set new c8y.topics and return to default value
...                 Set new az.root_cert_path and return to default value
...                 Set new az.mapper.timestamp and return to default value
...                 Set new az.topics and return to default value
...                 Set new aws.topics and return to default value
...                 Set new mqtt.bind.address and return to default value
...                 Set new mqtt.bind.port and return to default value
...                 Set new tmp.path and return to default value
...                 Set new logs.path and return to default value
...                 Set new run.path and return to default value

Resource            ../../resources/common.resource
Library             ThinEdgeIO

Suite Setup         Setup
Suite Teardown      Get Logs

Force Tags          theme:cli    theme:configuration


*** Test Cases ***
tedge config list
    Execute Command    tedge config list

tedge config list --all
    Execute Command    tedge config list --all

set/unset device.type
    Execute Command    sudo tedge config set device.type changed-type    # Changing device.type to "changed-type"
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.type=changed-type

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.type
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.type=thin-edge.io

set/unset device.key_path
    # Changing device.key_path
    Execute Command
    ...    sudo tedge config set device.key_path /etc/tedge/device-certs1/tedge-private-key.pem
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.key_path=/etc/tedge/device-certs1/tedge-private-key.pem

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.key_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.key_path=/etc/tedge/device-certs/tedge-private-key.pem

set/unset device.cert_path
    # Changing device.cert_path
    Execute Command
    ...    sudo tedge config set device.cert_path /etc/tedge/device-certs1/tedge-certificate.pem
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.cert_path=/etc/tedge/device-certs1/tedge-certificate.pem

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.cert_path=/etc/tedge/device-certs/tedge-certificate.pem

set/unset c8y.root_cert_path
    Execute Command    sudo tedge config set c8y.root_cert_path /etc/ssl/certs1    # Changing c8y.root_cert_path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.root_cert_path=/etc/ssl/certs1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.root_cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    c8y.root_cert_path=/etc/ssl/certs

set/unset c8y.smartrest.templates
    Execute Command    sudo tedge config set c8y.smartrest.templates 1    # Changing c8y.smartrest.templates
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.smartrest.templates=["1"]

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.smartrest.templates
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    c8y.smartrest.templates=[]

set/unset c8y.topics
    Execute Command    sudo tedge config set c8y.topics topic1,topic2    # Changing c8y.topics
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.topics=["topic1", "topic2"]

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.topics
    ${unset}    Execute Command    tedge config list
    Should Contain
    ...    ${unset}
    ...    c8y.topics=["tedge/measurements", "tedge/measurements/+", "tedge/alarms/+/+", "tedge/alarms/+/+/+", "tedge/events/+", "tedge/events/+/+", "tedge/health/+", "tedge/health/+/+"]

set/unset az.root_cert_path
    Execute Command    sudo tedge config set az.root_cert_path /etc/ssl/certs1    # Changing az.root_cert_path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.root_cert_path=/etc/ssl/certs1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.root_cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    az.root_cert_path=/etc/ssl/certs

set/unset az.topics
    Execute Command    sudo tedge config set az.topics topic1,topic2    # Changing az.topics
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.topics=["topic1", "topic2"]

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.topics
    ${unset}    Execute Command    tedge config list
    Should Contain
    ...    ${unset}
    ...    az.topics=["tedge/measurements", "tedge/measurements/+", "tedge/health/+", "tedge/health/+/+"]

set/unset aws.topics
    Execute Command    sudo tedge config set aws.topics topic1,topic2    # Changing aws.topics
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    aws.topics=["topic1", "topic2"]

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset aws.topics
    ${unset}    Execute Command    tedge config list
    Should Contain
    ...    ${unset}
    ...    aws.topics=["tedge/measurements", "tedge/measurements/+", "tedge/alarms/+/+", "tedge/alarms/+/+/+", "tedge/events/+", "tedge/events/+/+", "tedge/health/+", "tedge/health/+/+"]

set/unset aws.url
    Execute Command    sudo tedge config set aws.url your-endpoint.amazonaws.com    # Changing aws.url
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    aws.url=your-endpoint.amazonaws.com

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset aws.url
    ${unset}    Execute Command    tedge config list
    Should not Contain    ${unset}    aws.url=

set/unset aws.root_cert_path
    Execute Command    sudo tedge config set aws.root_cert_path /etc/ssl/certs1    # Changing aws.aws.root_cert_path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    aws.root_cert_path=/etc/ssl/certs1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset aws.root_cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    aws.root_cert_path=/etc/ssl/certs

set/unset aws.mapper.timestamp
    Execute Command    sudo tedge config set aws.mapper.timestamp false    # Changing aws.mapper.timestamp
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    aws.mapper.timestamp=false

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset aws.mapper.timestamp
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    aws.mapper.timestamp=true

set/unset az.mapper.timestamp
    Execute Command    sudo tedge config set az.mapper.timestamp false    # Changing az.mapper.timestamp
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.mapper.timestamp=false

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.mapper.timestamp
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    az.mapper.timestamp=true

set/unset mqtt.bind.address
    Execute Command    sudo tedge config set mqtt.bind.address 127.1.1.1    # Changing mqtt.bind.address
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.bind.address=127.1.1.1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.bind.address
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.bind.address=127.0.0.1

set/unset mqtt.bind.port
    Execute Command    sudo tedge config set mqtt.bind.port 8888    # Changing mqtt.bind.port
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.bind.port=8888

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.bind.port
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.bind.port=1883

set/unset http.bind.port
    Execute Command    sudo tedge config set http.bind.port 7777    # Changing http.bind.port
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    http.bind.port=7777

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset http.bind.port
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    http.bind.port=8000

set/unset tmp.path
    Execute Command    sudo tedge config set tmp.path /tmp1    # Changing tmp.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    tmp.path=/tmp1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset tmp.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    tmp.path=/tmp

set/unset logs.path
    Execute Command    sudo tedge config set logs.path /var/log1    # Changing logs.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    logs.path=/var/log1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset logs.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    logs.path=/var/log

set/unset run.path
    Execute Command    sudo tedge config set run.path /run1    # Changing run.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    run.path=/run1

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset run.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    run.path=/run

set/unset firmware.child.update.timeout
    # Changing firmware.child.update.timeout
    Execute Command
    ...    sudo tedge config set firmware.child.update.timeout 4000
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    firmware.child.update.timeout=4000

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset firmware.child.update.timeout
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    firmware.child.update.timeout=3600

set/unset c8y.url
    Execute Command    sudo tedge config set c8y.url your-tenant.cumulocity.com    # Changing c8y.url
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.url=your-tenant.cumulocity.com

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.url
    ${unset}    Execute Command    tedge config list
    Should not Contain    ${unset}    c8y.url=

set/unset az.url
    Execute Command    sudo tedge config set az.url MyAzure.azure-devices.net    # Changing az.url
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.url=MyAzure.azure-devices.net

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.url
    ${unset}    Execute Command    tedge config list
    Should not Contain    ${unset}    az.url=

set/unset mqtt.external.bind.port
    Execute Command    sudo tedge config set mqtt.external.bind.port 8888    # Changing mqtt.external.bind.port
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.bind.port=8888

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.bind.port
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.bind.port=

mqtt.external.bind.address
    # Changing mqtt.external.bind.address
    Execute Command
    ...    sudo tedge config set mqtt.external.bind.address 0.0.0.0
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.bind.address=0.0.0.0

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.bind.address
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.bind.address=

mqtt.external.bind.interface
    # Changing mqtt.external.bind.interface
    Execute Command
    ...    sudo tedge config set mqtt.external.bind.interface wlan0
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.bind.interface=wlan0

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.bind.interface
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.bind.interface=

set/unset mqtt.external.ca_path
    # Changing mqtt.external.ca_path
    Execute Command
    ...    sudo tedge config set mqtt.external.ca_path /etc/ssl/certsNote
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.ca_path=/etc/ssl/certsNote

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.ca_path
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.ca_path=

set/unset mqtt.external.cert_file
    # Changing mqtt.external.cert_file
    Execute Command
    ...    sudo tedge config set mqtt.external.cert_file /etc/tedge/device-certs/tedge-certificate.pemNote
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.cert_file=/etc/tedge/device-certs/tedge-certificate.pemNote

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.cert_file
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.cert_file=

set/unset mqtt.external.key_file
    # Changing mqtt.external.key_file
    Execute Command
    ...    sudo tedge config set mqtt.external.key_file /etc/tedge/device-certs/tedge-private-key.pemNote
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.key_file=/etc/tedge/device-certs/tedge-private-key.pemNote

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.external.key_file
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.key_file=

set/unset software.plugin.default
    Execute Command    sudo tedge config set software.plugin.default apt    # Changing software.plugin.default
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    software.plugin.default=apt

    # Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset software.plugin.default
    ${unset}    Execute Command    tedge config list
    Should Not Contain    ${unset}    software.plugin.default=
