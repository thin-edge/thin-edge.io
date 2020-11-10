DEVICE=$1

if [ -z "$DEVICE" ]
then
    echo usage: $0 device-identifier
    echo
    echo Upload the device certificate \$device-identifier to c8y.
    exit 1
fi
if [ ! -f $DEVICE.crt ]
then
    echo Certificate file $DEVICE.crt not found.
    exit 1
fi

. get-credentials.sh

CERT_FILE=$DEVICE.crt
CERT=$(cat $CERT_FILE | tr -d '\n')
DATA=$(cat <<EOF
{ "name": "$DEVICE",
  "certInPemFormat":"$CERT",
  "autoRegistrationEnabled": true,
  "status":"ENABLED"}
EOF
)

curl --request POST --silent \
  --url https://$TENANT.$C8Y/tenant/tenants/$TENANT/trusted-certificates/ \
  --header "authorization: Basic $HASH" \
  --header 'Content-Type: application/json' \
  --data-raw "$DATA"
