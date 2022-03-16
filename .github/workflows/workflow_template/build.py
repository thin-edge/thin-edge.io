
templates = ['A','B','C','D', 'Azure', 'ALL']

# Hint:
# Azure is currently unused
# Template ALL is to compare with the offsite workflow:
#     meld system-test-workflow_ALL.yml system-test-offsite.yml

from string import Template
import os

filename="system-test-workflow_T.yml"
filenamet="system-test-workflow_{}.yml"

path = "../"

branch = "continuous_integration"
#branch = "main"

class MyTemplate(Template):
    delimiter = '%'

with open(filename) as f:
    c = f.read()
    t = MyTemplate(c)

    for k in templates:
        with open(os.path.join(path,filenamet.format(k)), "w") as o:
            print(k, k.lower(), filenamet.format(k))
            o.write(t.substitute(T=k, t=k.lower(), b=branch))

