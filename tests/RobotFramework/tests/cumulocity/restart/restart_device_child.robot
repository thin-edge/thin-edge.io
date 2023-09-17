*** Settings ***
Resource            ../../../resources/common.resource
Library             Cumulocity
Library             ThinEdgeIO

Suite Setup         Custom Setup
Test Teardown       Get Logs

Test Tags           theme:c8y    theme:troubleshooting    theme:childdevices


*** Test Cases ***
Support restarting the device
    Cumulocity.Should Contain Supported Operations    c8y_Restart
    ${operation}=    Cumulocity.Restart Device
    Operation Should Be SUCCESSFUL    ${operation}    timeout=180


*** Keywords ***
Setup Child Device
    ThinEdgeIO.Set Device Context    ${CHILD_SN}
    Execute Command    sudo dpkg -i packages/tedge_*.deb

    Execute Command    sudo tedge config set mqtt.client.host ${PARENT_IP}
    Execute Command    sudo tedge config set mqtt.client.port 1883
    Execute Command    sudo tedge config set mqtt.topic_root te
    Execute Command    sudo tedge config set mqtt.device_topic_id "device/${CHILD_SN}//"

    # Install plugin after the default settings have been updated to prevent it from starting up as the main plugin
    Execute Command    sudo dpkg -i packages/tedge-agent*.deb
    Execute Command    sudo systemctl enable tedge-agent
    Execute Command    sudo systemctl start tedge-agent

    Transfer To Device    ${CURDIR}/*.sh    /usr/bin/
    Transfer To Device    ${CURDIR}/*.service    /etc/systemd/system/
    Execute Command    apt-get install -y systemd-sysv && chmod a+x /usr/bin/*.sh && chmod 644 /etc/systemd/system/*.service && systemctl enable on_startup.service
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh' > /etc/sudoers.d/tedge
    Execute Command    cmd=sed -i 's|reboot =.*|reboot = ["/usr/bin/on_shutdown.sh"]|g' /etc/tedge/system.toml

    # WORKAROUND: Uncomment next line once https://github.com/thin-edge/thin-edge.io/issues/2253 has been resolved
    # ThinEdgeIO.Service Health Status Should Be Up    tedge-agent    device=${CHILD_SN}

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
    Setup Child Device
    Cumulocity.Device Should Exist    ${CHILD_SN}
