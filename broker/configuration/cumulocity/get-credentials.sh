if [ -f .credentials ]
then
    . .credentials
else
    echo -n "C8Y:"
    read C8Y

    echo -n "TENANT:"
    read TENANT

    echo -n "USER:"
    read USER

    echo -n "PASSWORD:"
    stty -echo
    read PASSWORD
    stty echo

    HASH=$(echo -n $TENANT/$USER:$PASSWORD | base64)

    cat >.credentials<<EOF
C8Y=$C8Y
TENANT=$TENANT
USER=$USER
HASH=$HASH
EOF
fi

