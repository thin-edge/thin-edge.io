*** Settings ***
Resource    ../../resources/common.resource
Library    Cumulocity
Library    ThinEdgeIO

Test Tags    theme:configuration
# Test Setup    Custom Setup
Test Teardown    Get Logs

*** Test Cases ***

Startup with invalid system.toml
    [Tags]    known-issue
    Skip    msg=Known issue. tedge does not startup if the system.toml file contains invalid toml content
    Setup    skip_bootstrap=True
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    ${DEVICE_SN}=    Execute Command           tedge config get device.id
    Execute Command           cmd=echo -n "[new-section]\ntest = invalid" > /etc/tedge/system.toml
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-install || true
    Device Should Exist       ${DEVICE_SN}

Startup with invalid tedge.toml
    [Tags]    known-issue
    Skip    msg=Known issue. tedge does not startup if the tedge.toml file contains invalid toml content
    Setup    skip_bootstrap=True
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-connect || true
    ${DEVICE_SN}=    Execute Command           tedge config get device.id
    Execute Command           cmd=echo -n "[new-section]\ntest = invalid" > /etc/tedge/tedge.toml
    Execute Command           test -f ./bootstrap.sh && ./bootstrap.sh --no-install || true
    Device Should Exist       ${DEVICE_SN}
