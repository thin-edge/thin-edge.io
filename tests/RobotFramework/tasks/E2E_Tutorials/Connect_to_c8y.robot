#Command to execute:    robot -d \results --timestampoutputs --log health_tedge_mapper.html --report NONE health_tedge_mapper.robot

*** Settings ***
Resource    ../../resources/common.resource
Library    ThinEdgeIO
Library    String
Library    Cumulocity
Suite Teardown         Get Logs

*** Variables ***

${DEVICE_SN}


*** Tasks ***

Install thin-edge.io on your device
    ${device_sn}=    Setup       skip_bootstrap=True
    Set Suite Variable    $DEVICE_SN    ${device_sn}
    Uninstall tedge with purge
    Clear previous downloaded files if any
    Install thin-edge.io

Set the URL of your Cumulocity IoT tenant
    ${HOSTNAME}=      Replace String Using Regexp    ${C8Y_CONFIG.host}    ^.*://    ${EMPTY}
    ${HOSTNAME}=      Strip String    ${HOSTNAME}    characters=/
    Execute Command    sudo tedge config set c8y.url ${HOSTNAME}    # Set the URL of your Cumulocity IoT tenant

Create the certificate
    Execute Command    sudo tedge cert create --device-id ${DEVICE_SN}

    #You can then check the content of that certificate.
    ${output}=    Execute Command    sudo tedge cert show    #You can then check the content of that certificate.
    Should Contain    ${output}    Device certificate: /etc/tedge/device-certs/tedge-certificate.pem
    Should Contain    ${output}    Subject: CN=${DEVICE_SN}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Issuer: CN=${DEVICE_SN}, O=Thin Edge, OU=Test Device
    Should Contain    ${output}    Valid from:
    Should Contain    ${output}    Valid up to:
    Should Contain    ${output}    Thumbprint:

tedge cert upload c8y command
    Execute Command    sudo env C8YPASS\='${C8Y_CONFIG.password}' tedge cert upload c8y --user ${C8Y_CONFIG.username}
    Sleep    3s    # Wait for cert to be processed/distributed to all cores (in Cumulocity IoT)

Connect the device
    ${output}=    Execute Command    sudo tedge connect c8y    #You can then check the content of that certificate.
    Sleep    3s
    Should Contain    ${output}    Checking if systemd is available.
    Should Contain    ${output}    Checking if configuration for requested bridge already exists.
    Should Contain    ${output}    Validating the bridge certificates.
    Should Contain    ${output}    Creating the device in Cumulocity cloud.
    Should Contain    ${output}    Saving configuration for requested bridge.
    Should Contain    ${output}    Restarting mosquitto service.
    Should Contain    ${output}    Awaiting mosquitto to start. This may take up to 5 seconds.
    Should Contain    ${output}    Enabling mosquitto service on reboots.
    Should Contain    ${output}    Successfully created bridge connection!
    Should Contain    ${output}    Sending packets to check connection. This may take up to 2 seconds.
    Should Contain    ${output}    Connection check is successful.
    Should Contain    ${output}    Checking if tedge-mapper is installed.
    Should Contain    ${output}    Starting tedge-mapper-c8y service.
    Should Contain    ${output}    Persisting tedge-mapper-c8y on reboot.
    Should Contain    ${output}    tedge-mapper-c8y service successfully started and enabled!
    Should Contain    ${output}    Enabling software management.
    Should Contain    ${output}    Checking if tedge-agent is installed.
    Should Contain    ${output}    Starting tedge-agent service.
    Should Contain    ${output}    Persisting tedge-agent on reboot.
    Should Contain    ${output}    tedge-agent service successfully started and enabled!

Sending your first telemetry data
    Execute Command    tedge mqtt pub c8y/s/us 211,20    #Set the URL of your Cumulocity IoT tenant

Download the measurements report file
    Device Should Exist    ${DEVICE_SN}
    ${measurements}=    Device Should Have Measurements    minimum=1    maximum=1

Monitor the device
    # Install collectd
    Execute Command    sudo apt-get install libmosquitto1 -y
    Execute Command    sudo apt-get install collectd-core -y
    # Configure collectd
    Execute Command    sudo cp /etc/tedge/contrib/collectd/collectd.conf /etc/collectd/collectd.conf; sudo systemctl restart collectd
    #Enable Collectd
    Execute Command    sudo systemctl start tedge-mapper-collectd && sudo systemctl enable tedge-mapper-collectd
    ${measure}    Device Should Have Measurements    minimum=1


*** Keywords ***

Uninstall tedge with purge
    Execute Command    wget https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/uninstall-thin-edge_io.sh
    Execute Command    chmod a+x uninstall-thin-edge_io.sh
    Execute Command    ./uninstall-thin-edge_io.sh purge

Clear previous downloaded files if any
    Execute Command    rm -f *.deb; rm -f uninstall-thin-edge_io.sh

Install thin-edge.io
    Execute Command    curl -fsSL https://raw.githubusercontent.com/thin-edge/thin-edge.io/main/get-thin-edge_io.sh | sudo sh -s
