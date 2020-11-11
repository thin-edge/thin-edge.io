. get-credentials.sh

curl --request POST --silent \
  --url $TENANT.$C8Y/tenant/options \
  --header 'accept: application/*' \
  --header 'cache-control: no-cache' \
  --header 'content-type: application/json' \
  --header "authorization: Basic $HASH" \
  --data '{
"category": "oauth.internal.token",
"value": "true",
"key": "trusted-certificates.enabled"
}'
