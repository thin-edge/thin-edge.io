*** Settings ***
Resource        ../../resources/common.resource
Library         ThinEdgeIO
Library         Cumulocity

Suite Setup     Custom Setup

Test Tags       theme:software    theme:plugins    adapter:docker


*** Test Cases ***
Install package with localization
    [Template]    Install package with localization
    ja_JP.UTF-8
    fr_FR.UTF-8
    de_DE.UTF-8


*** Keywords ***
Custom Setup
    ${DEVICE_SN}=    Setup
    Device Should Exist    ${DEVICE_SN}

    # remove docker logic which blocks language settings and reinstall apt/dpkg
    Execute Command    cmd=rm -f /etc/apt/apt.conf.d/docker-no-languages /etc/dpkg/dpkg.cfg.d/docker
    Execute Command    cmd=apt-get update && apt-get install -y --no-install-recommends locales
    Execute Command    cmd=apt-get install --reinstall -y apt dpkg

Install package with localization
    [Arguments]    ${LANG}
    Execute Command    cmd=sed -i -e 's/# ${LANG} UTF-8/${LANG} UTF-8/' /etc/locale.gen && locale-gen
    Transfer To Device    src=${CURDIR}/sampledeb_1.0.0_all.deb    dst=/opt/
    Execute Command
    ...    cmd=sudo env LC_ALL=${LANG} LANGUAGE= /etc/tedge/sm-plugins/apt install sampledeb --module-version 1.0.0 --file /opt/sampledeb_1.0.0_all.deb

    [Teardown]    Run Keywords    Execute Command    cmd=apt-get remove -y sampledeb ||:    AND    Get Logs
