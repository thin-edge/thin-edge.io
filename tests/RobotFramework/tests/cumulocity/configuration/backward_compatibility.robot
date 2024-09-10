*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Test Teardown       Get Logs    name=${DEVICE_SN}

Test Tags           theme:configuration    theme:installation


*** Test Cases ***
Migrate Legacy Configuration Files
    ${DEVICE_SN}=    Setup    skip_bootstrap=${True}
    Set Suite Variable    $DEVICE_SN

    # Copy old c8y-configuration-plugin's config file before bootstrapping
    ThinEdgeIO.Execute Command    rm -f /etc/tedge/plugins/*
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y-configuration-plugin.toml    /etc/tedge/c8y/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json    /etc/

    # Bootstrap the tedge-agent so that it picks up the old c8y-configuration-plugin.toml
    Execute Command    ./bootstrap.sh
    Cumulocity.Device Should Exist    ${DEVICE_SN}
    ${operation}=    Cumulocity.Get Configuration    TEST_CONFIG
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}
