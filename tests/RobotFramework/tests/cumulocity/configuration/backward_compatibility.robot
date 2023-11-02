*** Settings ***
Resource            ../../../resources/common.resource
Library             ThinEdgeIO
Library             Cumulocity

Suite Teardown      Get Logs    name=${PARENT_SN}

Test Tags          theme:configuration    theme:childdevices

*** Test Cases ***                DEVICE        EXTERNALID                        CONFIG_TYPE       DEVICE_FILE                  FILE                                PERMISSION    OWNERSHIP

Get Configuration from Device
    ${parent_sn}=    Setup    skip_bootstrap=True
    Set Suite Variable    $PARENT_SN    ${parent_sn}

    # Copy old c8y-configuration-plugin's config file before bootstrapping
    ThinEdgeIO.Transfer To Device    ${CURDIR}/c8y-configuration-plugin.toml    /etc/tedge/c8y/
    ThinEdgeIO.Transfer To Device    ${CURDIR}/config1.json         /etc/

    # Bootstrap the tedge-config-plugin so that it picks up the old c8y-configuration-plugin.toml
    Execute Command    ./bootstrap.sh
    ${operation}=    Cumulocity.Get Configuration    TEST_CONFIG
    ${operation}=    Operation Should Be SUCCESSFUL    ${operation}    timeout=120
