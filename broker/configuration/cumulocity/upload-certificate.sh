#!/usr/bin/env bash
DEVICE=$1
CERT_FILE=$2
C8Y=$3
TENANT=$4
USER=$5

if [ -z "$DEVICE" -o -z "$CERT_FILE" -o -z "$C8Y" -o -z "$TENANT" -o -z "$USER" -o "$#" -ne 5 ]
then
    echo "usage: $0 DEVICE_ID CERT_FILE C8Y_URL TENANT USER"
    echo
    echo "Upload the certificate CERT_FILE to c8y."
    exit 1
fi

if [ ! -f "$CERT_FILE" ]
then
    echo "File not found: $CERT_FILE"
    exit 1
fi

if ! (file "$CERT_FILE" | grep -q PEM)
then
    echo "[ERROR] The file $CERT_FILE is not a certificate: $(file $CERT_FILE)"
    exit 1
fi

echo -n "$USER PASSWORD:"
stty -echo
read PASSWORD
stty echo
echo
HASH=$(echo -n "$TENANT/$USER:$PASSWORD" | base64)

### Upload request

CERT=$(cat $CERT_FILE | tr -d '\n')
DATA=$(cat <<EOF
{ "name": "$DEVICE",
  "certInPemFormat":"$CERT",
  "autoRegistrationEnabled": true,
  "status":"ENABLED"}
EOF
)

if curl --request POST --silent \
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

CERT_ID=$(cat $CERT_FILE | grep -v CERTIFICATE | tr -d '\n')

if (curl --request GET --silent \
  --url https://$TENANT.$C8Y/tenant/tenants/$TENANT/trusted-certificates/ \
  --header "authorization: Basic $HASH" \
  --header 'Content-Type: application/json' | grep -q "$CERT_ID")
then
   echo "[OK] the device certificate is trusted by c8y"
else
   echo "[ERROR] the device certificate is not trusted by c8y"
fi
