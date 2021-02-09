#!/usr/bin/env bash
DEVICE=$1
CERT_PATH=$2
C8Y=$3
TENANT=$4
USER=$5

if [ -z "$DEVICE" -o -z "$CERT_PATH" -o -z "$C8Y" -o -z "$TENANT" -o -z "$USER" -o "$#" -ne 5 ]
then
    echo "usage: $0 DEVICE_ID CERT_PATH C8Y_URL TENANT USER"
    echo
    echo "Upload the certificate CERT_PATH to c8y."
    exit 1
fi

if [ ! -f "$CERT_PATH" ]
then
    echo "File not found: $CERT_PATH"
    exit 1
fi

if ! (file "$CERT_PATH" | grep -q PEM)
then
    echo "[ERROR] The file $CERT_PATH is not a certificate: $(file $CERT_PATH)"
    exit 1
fi

echo -n "$USER PASSWORD:"
stty -echo
read PASSWORD
stty echo
echo
HASH=$(echo -n "$TENANT/$USER:$PASSWORD" | base64)

### Upload request

CERT=$(cat $CERT_PATH | tr -d '\n')
DATA=$(cat <<EOF
{ "name": "$DEVICE",
  "certInPemFormat":"$CERT",
  "autoRegistrationEnabled": true,
  "status":"ENABLED"}
EOF
)

if curl --request POST \
  --url https://$TENANT.$C8Y/tenant/tenants/$TENANT/trusted-certificates/ \
  --header "authorization: Basic $HASH" \
  --header 'Content-Type: application/json' \
  --data-raw "$DATA"
then
   echo "[OK] the device certificate has been uploaded to c8y"
else
   echo "[ERROR] the device certificate has not been uploaded to c8y"
fi

### Test request

CERT_ID=$(cat $CERT_PATH | grep -v CERTIFICATE | tr -d '\n')

if (curl --request GET --silent \
  --url https://$TENANT.$C8Y/tenant/tenants/$TENANT/trusted-certificates/ \
  --header "authorization: Basic $HASH" \
  --header 'Content-Type: application/json' | grep -q "$CERT_ID")
then
   echo "[OK] the device certificate is trusted by c8y"
else
   echo "[ERROR] the device certificate is not trusted by c8y"
fi
