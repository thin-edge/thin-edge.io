#!/bin/python3

"""Parse final xml and return an error if there are failures.
"""

import sys
from xml.dom.minidom import parse

dom = parse(sys.argv[1])

errors = 0
failures = 0

for nodes in dom.childNodes:
    l = nodes.attributes.length

    for node in range(l):
        attr = nodes.attributes.item(node)

        print(f" {attr.name} : {attr.value}")
        if attr.name == "failures":
            failures = int(attr.value)
        if attr.name == "errors":
            errors = int(attr.value)

print(f"Recorded {errors} errors and {failures} failures in {sys.argv[1]}")

if errors == 0 and failures == 0:
    print("Passed, no errors.")
    sys.exit(0)
else:
    print("Failed, there are errors in the test run.")
    sys.exit(1)
