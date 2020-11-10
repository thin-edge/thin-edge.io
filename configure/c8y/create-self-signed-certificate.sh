DEVICE=$1

if [ -z "$DEVICE" ]
then
    echo usage: $0 device-identifier
    echo
    echo Generates a self signed certificate for the device
    echo using the device identifier as its common name.
    exit 1
fi

# see https://www.mkssoftware.com/docs/man1/openssl_req.1.asp

CONFIG="
[ req ]
default_bits            = 2048
distinguished_name = dist_name
x509_extensions = v3_ca
output_password = nopass
prompt = no

[ dist_name ]
commonName = $DEVICE
organizationName = 'SAG C8Y'
organizationalUnitName	= 'Thin Edge'

[ v3_ca ]
basicConstraints = CA:true
"

openssl req -config <(echo "$CONFIG") -new -nodes -x509 -days 365 -extensions v3_ca -keyout $DEVICE.key -out $DEVICE.crt
