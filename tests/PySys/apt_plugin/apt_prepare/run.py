import sys
import os
import time

sys.path.append("apt_plugin")
from environment_apt_plugin import AptPlugin

"""
Validate apt plugin prepare
(As far as currently possible)

When we store the modification time of the apt-cache
When we call prepare
When we store the modification time of the apt-cache
Then we compare the timestamps, to make sure that the cache was updated
"""


class AptPluginPrepare(AptPlugin):
    def setup(self):
        super().setup()
        self.addCleanupFunction(self.cleanup_prepare)

    def execute(self):
        self.mtime_old = os.stat("/var/cache/apt/pkgcache.bin").st_mtime
        self.plugin_cmd("prepare", "outp_prepare", 0)
        self.mtime_new = os.stat("/var/cache/apt/pkgcache.bin").st_mtime
        self.now = time.time()

    def validate(self):
        # make sure that the timestamp has changed
        self.assertThat("old != new", old=self.mtime_old, new=self.mtime_new)
        # make sure the cache was updated in the last N seconds
        # N=100 : Took 91s to update at mythic beasts
        # N=150 : Took 123s to update at mythic beasts (Why???)
        self.assertThat("(new +150) >= now", new=self.mtime_new, now=self.now)

    def cleanup_prepare(self):
        pass
