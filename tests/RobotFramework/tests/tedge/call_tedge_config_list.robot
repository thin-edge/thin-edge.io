*** Settings ***
Documentation    Purpose of this test is to verify that the tedge config list and tedge config list --all
...              will result with Return Code 0
...              Set new device type and return to default value
...              Set new kay path and return to default value
...              Set new cert path and return to default value
...              Set new c8y.root.cert.path and return to default value
...              Set new c8y.smartrest.templates and return to default value
...              Set new az.root.cert.path and return to default value
...              Set new az.mapper.timestamp and return to default value
...              Set new mqtt.bind_address and return to default value
...              Set new mqtt.port and return to default value
...              Set new tmp.path and return to default value
...              Set new logs.path and return to default value
...              Set new run.path and return to default value

Resource    ../../resources/common.resource
Library    ThinEdgeIO

Test Tags    theme:cli    theme:configuration
Suite Setup            Setup
Suite Teardown         Get Logs


*** Test Cases ***
tedge config list
    Execute Command    tedge config list

tedge config list --all
    Execute Command    tedge config list --all

set/unset device.type
    Execute Command    sudo tedge config set device.type changed-type    #Changing device.type to "changed-type"
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    device.type=changed-type

    Execute Command    sudo tedge config unset device.type    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    device.type=thin-edge.io

set/unset device.key.path
    Execute Command    sudo tedge config set device.key.path /etc/tedge/device-certs1/tedge-private-key.pem    #Changing device.key.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    device.key.path=/etc/tedge/device-certs1/tedge-private-key.pem

    Execute Command    sudo tedge config unset device.key.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    device.key.path=/etc/tedge/device-certs/tedge-private-key.pem

set/unset device.cert.path
    Execute Command    sudo tedge config set device.cert.path /etc/tedge/device-certs1/tedge-certificate.pem   #Changing device.cert.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    device.cert.path=/etc/tedge/device-certs1/tedge-certificate.pem

    Execute Command    sudo tedge config unset device.cert.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    device.cert.path=/etc/tedge/device-certs/tedge-certificate.pem

set/unset c8y.root.cert.path
    Execute Command    sudo tedge config set c8y.root.cert.path /etc/ssl/certs1   #Changing c8y.root.cert.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    c8y.root.cert.path=/etc/ssl/certs1

    Execute Command    sudo tedge config unset c8y.root.cert.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    c8y.root.cert.path=/etc/ssl/certs

set/unset c8y.smartrest.templates
    Execute Command    sudo tedge config set c8y.smartrest.templates 1   #Changing c8y.smartrest.templates
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    c8y.smartrest.templates=["1"]

    Execute Command    sudo tedge config unset c8y.smartrest.templates    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    c8y.smartrest.templates=[]

set/unset az.root.cert.path
    Execute Command    sudo tedge config set az.root.cert.path /etc/ssl/certs1   #Changing az.root.cert.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    az.root.cert.path=/etc/ssl/certs1

    Execute Command    sudo tedge config unset az.root.cert.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    az.root.cert.path=/etc/ssl/certs

set/unset aws.url
    Execute Command    sudo tedge config set aws.url your-endpoint.amazonaws.com   #Changing aws.url
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    aws.url=your-endpoint.amazonaws.com

    Execute Command    sudo tedge config unset aws.url    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should not Contain    ${unset}    aws.url=

set/unset aws.root.cert.path
    Execute Command    sudo tedge config set aws.root.cert.path /etc/ssl/certs1   #Changing aws.root.cert.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    aws.root.cert.path=/etc/ssl/certs1

    Execute Command    sudo tedge config unset aws.root.cert.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    aws.root.cert.path=/etc/ssl/certs

set/unset aws.mapper.timestamp
    Execute Command    sudo tedge config set aws.mapper.timestamp false  #Changing aws.mapper.timestamp
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    aws.mapper.timestamp=false

    Execute Command    sudo tedge config unset aws.mapper.timestamp    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    aws.mapper.timestamp=true

set/unset az.mapper.timestamp
    Execute Command    sudo tedge config set az.mapper.timestamp false  #Changing az.mapper.timestamp
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    az.mapper.timestamp=false

    Execute Command    sudo tedge config unset az.mapper.timestamp    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    az.mapper.timestamp=true

set/unset mqtt.bind_address
    Execute Command    sudo tedge config set mqtt.bind_address 127.1.1.1  #Changing mqtt.bind_address
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.bind_address=127.1.1.1

    Execute Command    sudo tedge config unset mqtt.bind_address    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.bind_address=127.0.0.1

set/unset mqtt.port
    Execute Command    sudo tedge config set mqtt.port 8888  #Changing mqtt.port
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.port=8888

    Execute Command    sudo tedge config unset mqtt.port    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    mqtt.port=1883

set/unset http.port
    Execute Command    sudo tedge config set http.port 7777  #Changing http.port
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    http.port=7777

    Execute Command    sudo tedge config unset http.port    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    http.port=8000

set/unset tmp.path
    Execute Command    sudo tedge config set tmp.path /tmp1  #Changing tmp.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    tmp.path=/tmp1

    Execute Command    sudo tedge config unset tmp.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    tmp.path=/tmp

set/unset logs.path
    Execute Command    sudo tedge config set logs.path /var/log1  #Changing logs.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    logs.path=/var/log1

    Execute Command    sudo tedge config unset logs.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    logs.path=/var/log

set/unset run.path
    Execute Command    sudo tedge config set run.path /run1  #Changing run.path
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    run.path=/run1

    Execute Command    sudo tedge config unset run.path    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    run.path=/run

set/unset firmware.child.update.timeout
    Execute Command    sudo tedge config set firmware.child.update.timeout 4000  #Changing firmware.child.update.timeout
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    firmware.child.update.timeout=4000

    Execute Command    sudo tedge config unset firmware.child.update.timeout    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Contain    ${unset}    firmware.child.update.timeout=3600

set/unset c8y.url
    Execute Command    sudo tedge config set c8y.url your-tenant.cumulocity.com  #Changing c8y.url
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    c8y.url=your-tenant.cumulocity.com

    Execute Command    sudo tedge config unset c8y.url    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should not Contain    ${unset}    c8y.url=

set/unset az.url
    Execute Command    sudo tedge config set az.url MyAzure.azure-devices.net  #Changing az.url
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    az.url=MyAzure.azure-devices.net

    Execute Command    sudo tedge config unset az.url    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should not Contain    ${unset}    az.url=

set/unset mqtt.external.port 
    Execute Command    sudo tedge config set mqtt.external.port 8888  #Changing mqtt.external.port
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.port=8888

    Execute Command    sudo tedge config unset mqtt.external.port     #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.port=

mqtt.external.bind_address
    Execute Command    sudo tedge config set mqtt.external.bind_address 0.0.0.0  #Changing mqtt.external.bind_address
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.bind_address=0.0.0.0

    Execute Command    sudo tedge config unset mqtt.external.bind_address    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.bind_address=

mqtt.external.bind_interface
    Execute Command    sudo tedge config set mqtt.external.bind_interface wlan0  #Changing mqtt.external.bind_interface
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.bind_interface=wlan0

    Execute Command    sudo tedge config unset mqtt.external.bind_interface    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.bind_interface=

set/unset mqtt.external.capath
    Execute Command    sudo tedge config set mqtt.external.capath /etc/ssl/certsNote   #Changing mqtt.external.capath
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.capath=/etc/ssl/certsNote

    Execute Command    sudo tedge config unset mqtt.external.capath    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.capath=

set/unset mqtt.external.certfile
    Execute Command    sudo tedge config set mqtt.external.certfile /etc/tedge/device-certs/tedge-certificate.pemNote   #Changing mqtt.external.certfile
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.certfile=/etc/tedge/device-certs/tedge-certificate.pemNote

    Execute Command    sudo tedge config unset mqtt.external.certfile    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.certfile=

set/unset mqtt.external.keyfile
    Execute Command    sudo tedge config set mqtt.external.keyfile /etc/tedge/device-certs/tedge-private-key.pemNote   #Changing mqtt.external.keyfile
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    mqtt.external.keyfile=/etc/tedge/device-certs/tedge-private-key.pemNote

    Execute Command    sudo tedge config unset mqtt.external.keyfile    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    mqtt.external.keyfile=

set/unset software.plugin.default
    Execute Command    sudo tedge config set software.plugin.default apt   #Changing software.plugin.default
    ${set}     Execute Command    tedge config list
    Should Contain    ${set}    software.plugin.default=apt

    Execute Command    sudo tedge config unset software.plugin.default    #Undo the change by using the 'unset' command, value returns to default one
    ${unset}     Execute Command    tedge config list
    Should Not Contain    ${unset}    software.plugin.default=
