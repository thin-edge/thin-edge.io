
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

def publish_az():
    for i in range(20):
        message = f'{{"temperature": {i} }}'
        #message = '{"temperature": %i}'%i
        cmd = ["/usr/bin/tedge","mqtt", "pub", "az/messages/events/", message]
        subprocess.run(cmd)
        print (i)
        time.sleep(0.1)

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

def retrieve_az():

    sas_policy_primary_key = os.environ["SASPOLICYKEY"]

    sas_policy_name = "sas_policy"
    service_bus_name = "thinedgebus"
    queue_name = "testqueue"

    tokendict = get_auth_token( service_bus_name, queue_name, sas_policy_name, sas_policy_primary_key)

    token = tokendict["token"]
    print("Token", token)

    # See also
    # https://docs.microsoft.com/en-us/rest/api/servicebus/receive-and-delete-message-destructive-read


    # Curl
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
            print(text)
            #print(req.headers)
            props = json.loads( req.headers["BrokerProperties"] )
            #print("Properties", props)
            number = props["SequenceNumber"]
            print("SequenceNumber", number)
            time = props["EnqueuedTimeUtc"]
            print("Time", time)

            try:
                data = json.loads(text)
                temp = data["temperature"]
            except:
                print("Parsing Error", text)
                temp = None

            print(f"Got {number} from {time} message {text} temp {temp}")

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

publish_az()
retrieve_az()

