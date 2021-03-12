#!/usr/bin/sh


# a simple checker function
check() {
    if [ $? -ne 0 ]; then
        echo "Error: Exiting due to previous error"
        exit 1;
    fi
}

set -x

pip3 install pysys

echo "Dumping Environment"
echo "C8YPASS $C8YPASS"
echo "C8YUSERNAME $C8YUSERNAME"
echo "C8YTENNANT $C8YTENNANT"
echo "C8YDEVICE $C8YDEVICE"
echo "C8YDEVICEID $C8YDEVICEID"
echo "C8YTIMEZONE $C8YTIMEZONE"
echo "TEBASEDIR $TEBASEDIR"
echo "EXAMPLEDIR $EXAMPLEDIR"

cd tests/PySys/
check

~/.local/bin/pysys.py run -v DEBUG
check
