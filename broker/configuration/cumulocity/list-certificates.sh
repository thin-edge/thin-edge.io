. get-credentials.sh

curl --request GET --silent \
  --url https://$TENANT.$C8Y/tenant/tenants/$TENANT/trusted-certificates/ \
  --header "authorization: Basic $HASH" \
  --header 'Content-Type: application/json' \
