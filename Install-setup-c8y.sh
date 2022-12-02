#!/bin/sh

echo 'This script lets you setup a thinEdge connection to cumulocity IoT. We asume that the installatio was successful and all the necrsarry services are running.
Just follow the instructions.....'

read  -p 'Enter the Cumulocity tenant url e.g. [your-tenant.cumulocity.com]: ' tenant_url
tenant_url=${tenant_url:-your-tenant.cumulocity.com}
tedge config set c8y.url $tenant_url


read -p 'Enter the path to your root certificates [/etc/ssl/certs]: ' path_root_cert
path_root_cert=${path_root_cert:-/etc/ssl/certs}
tedge config set c8y.root.cert.path $/etc/ssl/certs


echo "Your current configuration:"
tedge config list

read -p 'Enter your device-id [my-device]: ' device-id
device-id=${device:-my-device}
tedge cert create --device-id $device-id

echo 'Created Certificate:'
sudo tedge cert show


read -p 'Enter Cumulocity user [your@email.com]: ' c8y_Username
c8y_Username=${c8y_Username:-your@email.com}

#read -ps 'Enter Cumulocity password [secret]: ' c8y_Password
#c8y_Password=${c8y_Username:-secret}

echo "Using $c8y_Username to authenticate...."
tedge cert upload c8y --user $c8y_Username

echo "Connecting...."
tedge connect c8y

read -r -p "Would you like to send a test measurement to your new device?? [y/N] " response
if [[ "$response" =~ ^([yY][eE][sS]|[yY])$ ]]
then
    echo "Creating a Temperature Measurement...."
    tedge mqtt pub c8y/s/us 211,20
else
    echo "Ok bye, enjoy thingEdge.io!!!"
fi
