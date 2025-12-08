*** Settings ***
Resource    ../pkcs11_common.resource


*** Keywords ***
Pass PIN in the request using pin-value
    [Documentation]    Tests if the PIN can be changed for the request by assuming current one is correct and setting a
    ...    different one to see if we get an error about pin being incorrect.

    ${key_uri}=    Execute Command    tedge config get device.key_uri    strip=True    ignore_exit_code=True
    # FIXME: this breaks if currently set URI already has query attributes, but currently that's not the case (other tests don't set it)
    Execute Command    cmd=tedge config set device.key_uri "pkcs11:token=tedge;object=tedge?pin-value=incorrect"
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Restore previous key_uri    ${key_uri}

Pass PIN in the request using device.key_pin
    Execute Command    tedge config set device.key_pin incorrect
    Tedge Reconnect Should Fail With    The specified PIN is incorrect

    [Teardown]    Execute Command    tedge config unset device.key_pin

Restore previous key_uri
    [Arguments]    ${key_uri}
    IF    $key_uri
        Execute Command    tedge config set device.key_uri "${key_uri}"
    ELSE
        Execute Command    tedge config unset device.key_uri
    END
