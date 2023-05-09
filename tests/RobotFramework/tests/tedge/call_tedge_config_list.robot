*** Settings ***
Documentation       Purpose of this test is to verify that the tedge config list and tedge config list --all
...                 will result with Return Code 0
...                 Set new device type and return to default value
...                 Set new kay path and return to default value
...                 Set new cert path and return to default value
...                 Set new c8y.root_cert_path and return to default value
...                 Set new c8y.smartrest.templates and return to default value
...                 Set new az.root_cert_path and return to default value
...                 Set new az.mapper.timestamp and return to default value
...                 Set new mqtt.bind_address and return to default value
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
    Execute Command    sudo tedge config set device.type changed-type    #Changing device.type to "changed-type"
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.type=changed-type

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.type
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.type=thin-edge.io

set/unset device.key_path
    #Changing device.key_path
    Execute Command
    ...    sudo tedge config set device.key_path /etc/tedge/device-certs1/tedge-private-key.pem
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.key_path=/etc/tedge/device-certs1/tedge-private-key.pem

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.key_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.key_path=/etc/tedge/device-certs/tedge-private-key.pem

set/unset device.cert_path
    #Changing device.cert_path
    Execute Command
    ...    sudo tedge config set device.cert_path /etc/tedge/device-certs1/tedge-certificate.pem
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    device.cert_path=/etc/tedge/device-certs1/tedge-certificate.pem

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset device.cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    device.cert_path=/etc/tedge/device-certs/tedge-certificate.pem

set/unset c8y.root_cert_path
    Execute Command    sudo tedge config set c8y.root_cert_path /etc/ssl/certs1    #Changing c8y.root_cert_path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.root_cert_path=/etc/ssl/certs1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.root_cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    c8y.root_cert_path=/etc/ssl/certs

set/unset c8y.smartrest.templates
    Execute Command    sudo tedge config set c8y.smartrest.templates 1    #Changing c8y.smartrest.templates
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    c8y.smartrest.templates=["1"]

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset c8y.smartrest.templates
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    c8y.smartrest.templates=[]

set/unset az.root_cert_path
    Execute Command    sudo tedge config set az.root_cert_path /etc/ssl/certs1    #Changing az.root_cert_path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.root_cert_path=/etc/ssl/certs1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.root_cert_path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    az.root_cert_path=/etc/ssl/certs

set/unset az.mapper.timestamp
    Execute Command    sudo tedge config set az.mapper.timestamp false    #Changing az.mapper.timestamp
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    az.mapper.timestamp=false

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset az.mapper.timestamp
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    az.mapper.timestamp=true

set/unset mqtt.bind_address
    Execute Command    sudo tedge config set mqtt.bind_address 127.1.1.1    #Changing mqtt.bind_address
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.bind_address=127.1.1.1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.bind_address
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.bind_address=127.0.0.1

set/unset mqtt.bind.port
    Execute Command    sudo tedge config set mqtt.bind.port 8888    #Changing mqtt.bind.port
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    mqtt.bind.port=8888

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset mqtt.bind.port
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.bind.port=1883

set/unset tmp.path
    Execute Command    sudo tedge config set tmp.path /tmp1    #Changing tmp.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    tmp.path=/tmp1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset tmp.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    tmp.path=/tmp

set/unset logs.path
    Execute Command    sudo tedge config set logs.path /var/log1    #Changing logs.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    logs.path=/var/log1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset logs.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    logs.path=/var/log

set/unset run.path
    Execute Command    sudo tedge config set run.path /run1    #Changing run.path
    ${set}    Execute Command    tedge config list
    Should Contain    ${set}    run.path=/run1

    #Undo the change by using the 'unset' command, value returns to default one
    Execute Command
    ...    sudo tedge config unset run.path
    ${unset}    Execute Command    tedge config list
    Should Contain    ${unset}    run.path=/run
