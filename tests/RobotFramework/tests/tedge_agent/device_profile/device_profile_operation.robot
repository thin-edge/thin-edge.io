*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity
Library             OperatingSystem

Force Tags          theme:tedge_agent
Suite Setup         Custom Setup
Test Setup          Custom Test Setup
Test Teardown       Get Logs

*** Test Cases ***

Triger custom device profile operation
    Execute Command    /etc/tedge/operations/device_profile.sh create_test_operation te/device/main///cmd/device_profile/robot-123
    ${cmd_messages}    Should Have MQTT Messages    te/device/main///cmd/device_profile/robot-123    message_pattern=.*successful.*   maximum=1    timeout=30
    Execute Command    tedge mqtt pub --retain te/device/main///cmd/device_profile/robot-123 ''

*** Keywords ***

Custom Test Setup
    Execute Command    cmd=echo 'tedge ALL = (ALL) NOPASSWD: /usr/bin/tedge, /usr/bin/systemctl, /etc/tedge/sm-plugins/[a-zA-Z0-9]*, /bin/sync, /sbin/init, /sbin/shutdown, /usr/bin/on_shutdown.sh, /usr/bin/tedge-write /etc/*' > /etc/sudoers.d/tedge

Custom Setup
    ${DEVICE_SN}=    Setup
    Set Suite Variable    $DEVICE_SN
    Device Should Exist                      ${DEVICE_SN}
    Copy Configuration Files
    Execute Command    apt-get update && apt-get install -y jq jo
    # setup repos so that packages can be installed from them
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/tedge-main/setup.deb.sh' | sudo -E bash
    Execute Command    curl -1sLf 'https://dl.cloudsmith.io/public/thinedge/community/setup.deb.sh' | sudo -E bash
    Restart Service    tedge-agent

Copy Configuration Files
    ThinEdgeIO.Transfer To Device    ${CURDIR}/device_profile.toml       /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/firmware_update.toml      /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/device_profile.sh         /etc/tedge/operations/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/tedge_operator_helper.sh         /etc/tedge/operations/
