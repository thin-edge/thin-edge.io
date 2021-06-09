
import time
import urllib
import hmac
import hashlib
import base64

import subprocess
import time


import requests
import json

import os
import sys

def publish_az(amount):
    """Publish to Azure topic"""

    for i in range(amount):
        message = f'{{"cafe": {i} }}'
        cmd = ["/usr/bin/tedge","mqtt", "pub", "az/messages/events/", message]
        subprocess.run(cmd)
        print ("Published message: ", message)
        time.sleep(0.05)

# Function taken from :
# https://docs.microsoft.com/en-us/rest/api/eventhub/generate-sas-token
# TODO : Care about license for this part
def get_auth_token(sb_name, eh_name, sas_name, sas_value):
    """
    Returns an authorization token dictionary
    for making calls to Event Hubs REST API.
    """
    uri = urllib.parse.quote_plus("https://{}.servicebus.windows.net/{}" \
                                  .format(sb_name, eh_name))
    sas = sas_value.encode('utf-8')
    expiry = str(int(time.time() + 10000))
    string_to_sign = (uri + '\n' + expiry).encode('utf-8')
    signed_hmac_sha256 = hmac.HMAC(sas, string_to_sign, hashlib.sha256)
    signature = urllib.parse.quote(base64.b64encode(signed_hmac_sha256.digest()))
    return  {"sb_name": sb_name,
             "eh_name": eh_name,
             "token":'SharedAccessSignature sr={}&sig={}&se={}&skn={}' \
                     .format(uri, signature, expiry, sas_name)
            }

def retrieve_queue_az(sas_policy_name, service_bus_name, queue_name):
    """Get the published messages back from a service bus queue"""

    if "SASPOLICYKEY" in os.environ:
        sas_policy_primary_key = os.environ["SASPOLICYKEY"]
    else:
        print("Error environment variable SASPOLICYKEY not set")
        sys.exit(1)

    tokendict = get_auth_token( service_bus_name, queue_name, sas_policy_name, sas_policy_primary_key)

    token = tokendict["token"]
    #print("Token", token)

    # See also
    # https://docs.microsoft.com/en-us/rest/api/servicebus/receive-and-delete-message-destructive-read


    # Do it manuylly with curl:
    # curl --request DELETE     --url "http{s}://thinedgebus.servicebus.windows.net/testqueue/messages/head" \
    #     --header "Accept: application/json"     --header "Content-Type: application/json;charset=utf-8"   \
    #   --header "Authorization: $SASTOKEN"     --verbose

    url = "https://thinedgebus.servicebus.windows.net/testqueue/messages/head"

    headers = { "Accept": "application/json",
               "Content-Type": "application/json;charset=utf-8",
               "Authorization": token}


    while True:
        req = requests.delete(url, headers=headers)

        if req.status_code == 200:
            #print(req)
            text = req.text
            #print(text)
            #print(req.headers)
            props = json.loads( req.headers["BrokerProperties"] )
            #print("Properties", props)
            number = props["SequenceNumber"]
            #print("SequenceNumber", number)
            time = props["EnqueuedTimeUtc"]
            #print("Time", time)

            try:
                data = json.loads(text)
                value = data["cafe"]
            except:
                print("Parsing Error", text)
                decoded = None

            print(f"Got message {number} from {time} message is {text} value: {value}")

        elif(req.status_code == 204):
            print("Queue Empty:  HTTP status: ", req.status_code)
            break
        elif(req.status_code == 401):
            print("Token Expired:  HTTP status: ", req.status_code)
            break
        else:
            print(req)
            #print(req.headers)
            print("Error HTTP status: ", req.status_code)
            raise SystemError


if __name__ == "__main__":

    amount = 30
    sas_policy_name = "sas_policy"
    service_bus_name = "thinedgebus"
    queue_name = "testqueue"

    publish_az(amount)
    retrieve_queue_az(sas_policy_name, service_bus_name, queue_name)

