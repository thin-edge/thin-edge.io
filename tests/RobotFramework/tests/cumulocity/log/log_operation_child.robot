*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             DateTime
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:log


*** Test Cases ***
Successful log operation
    ${end_timestamp}=    get current date    result_format=%Y-%m-%dT%H:%M:%S+0000
    ${operation}=    Cumulocity.Create Operation
    ...    description=Log file request
    ...    fragments={"c8y_LogfileRequest":{"dateFrom":"1970-01-01T00:00:00+0000","dateTo":"${end_timestamp}","logFile":"example","searchText":"first","maximumLines":10}}
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb
    Execute Command    sudo dpkg -i packages/tedge-log-plugin*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883

    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge-log-plugin.toml    /etc/tedge/plugins/tedge-log-plugin.toml
    ThinEdgeIO.Transfer To Device    ${CURDIR}/example.log    /var/log/example/
    Execute Command    chown root:root /etc/tedge/plugins/tedge-log-plugin.toml /var/log/example/example.log
    ThinEdgeIO.Stop Service    tedge-log-plugin
    Execute Command    cmd=sed -i 's|ExecStart=.*|ExecStart=/usr/bin/tedge-log-plugin --root te --device "device/${CHILD_SN}//"|g' /lib/systemd/system/tedge-log-plugin.service && sudo systemctl daemon-reload
    ThinEdgeIO.Start Service    tedge-log-plugin
    ThinEdgeIO.Service Health Status Should Be Up    tedge-log-plugin

Custom Setup
    # Parent
    ${parent_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $PARENT_SN    ${parent_sn}
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true

    ${parent_ip}=    Get IP Address
    Set Suite Variable    $PARENT_IP    ${parent_ip}
    Execute Command    sudo tedge config set c8y.enable.log_management true
    Execute Command    sudo tedge config set mqtt.external.bind.address ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.external.bind.port 1883

    ThinEdgeIO.Connect Mapper    c8y
    ThinEdgeIO.Service Health Status Should Be Up    tedge-mapper-c8y

    # Child
    ${child_sn}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $CHILD_SN    ${child_sn}
    Register Child Device    parent_name=${PARENT_SN}    child_name=${CHILD_SN}    supported_operations=c8y_LogfileRequest
    Cumulocity.Device Should Exist    ${CHILD_SN}

    Setup Child Device
