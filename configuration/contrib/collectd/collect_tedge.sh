#!/bin/bash

# TODO Move time intensive parts outside of the loop

HOSTNAME="${COLLECTD_HOSTNAME:-localhost}"
INTERVAL="${COLLECTD_INTERVAL:-60}"

while sleep "$INTERVAL"; do

    STATM_MOSQ=$(cat /proc/$(pgrep -x mosquitto)/statm)
    STATM_MOSQ_ARRAY=($STATM_MOSQ)

#       /proc/[pid]/statm
#              Provides information about memory usage, measured in pages.  The
#              columns are:
#
#                  size       (1) total program size
#                             (same as VmSize in /proc/[pid]/status)
#                  resident   (2) resident set size
#                             (inaccurate; same as VmRSS in /proc/[pid]/status)
#                  shared     (3) number of resident shared pages
#                             (i.e., backed by a file)
#                             (inaccurate; same as RssFile+RssShmem in
#                             /proc/[pid]/status)
#                  text       (4) text (code)
#                  lib        (5) library (unused since Linux 2.6; always 0)
#                  data       (6) data + stack
#                  dt         (7) dirty pages (unused since Linux 2.6; always 0)

#       /proc/[pid]/stat
#
#              (14) utime  %lu
#                     Amount  of  time  that this process has been scheduled in
#                     user  mode,  measured   in   clock   ticks   (divide   by
#                     sysconf(_SC_CLK_TCK)).    This   includes   guest   time,
#                     guest_time (time spent running a virtual CPU, see below),
#                     so that applications that are not aware of the guest time
#                     field do not lose that time from their calculations.
#
#              (15) stime  %lu
#                     Amount of time that this process has  been  scheduled  in
#                     kernel   mode,   measured   in  clock  ticks  (divide  by
#                     sysconf(_SC_CLK_TCK)).
#
                     
    STAT_MOSQ=$(cat /proc/$(pgrep -x mosquitto)/stat)
    STAT_MOSQ_ARRAY=($STAT_MOSQ)

    # Alternative
    #pgrep -f -x "/usr/bin/tedge_mapper c8y"
    MPID_TMP=$(systemctl show -p MainPID tedge-mapper-c8y.service)
    MPID=${MPID_TMP#MainPID=}

    STATM_MAPPER=$(cat /proc/$MPID/statm)
    STATM_MAPPER_ARRAY=($STATM_MAPPER)

    STAT_MAPPER=$(cat /proc/$MPID/stat)
    STAT_MAPPER_ARRAY=($STAT_MAPPER)
    
    # echo ${STAT_MAPPER_ARRAY[@]}
    

    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-size\" interval=$INTERVAL N:${STATM_MOSQ_ARRAY[0]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-resident\" interval=$INTERVAL N:${STATM_MOSQ_ARRAY[1]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-shared\" interval=$INTERVAL N:${STATM_MOSQ_ARRAY[2]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-text\" interval=$INTERVAL N:${STATM_MOSQ_ARRAY[3]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-data\" interval=$INTERVAL N:${STATM_MOSQ_ARRAY[5]}"

    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-utime\" interval=$INTERVAL N:${STAT_MOSQ_ARRAY[13]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mosquitto-stime\" interval=$INTERVAL N:${STAT_MOSQ_ARRAY[14]}"

    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-size\" interval=$INTERVAL N:${STATM_MAPPER_ARRAY[0]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-resident\" interval=$INTERVAL N:${STATM_MAPPER_ARRAY[1]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-shared\" interval=$INTERVAL N:${STATM_MAPPER_ARRAY[2]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-text\" interval=$INTERVAL N:${STATM_MAPPER_ARRAY[3]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-data\" interval=$INTERVAL N:${STATM_MAPPER_ARRAY[5]}"

    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-utime\" interval=$INTERVAL N:${STAT_MAPPER_ARRAY[13]}"
    echo "PUTVAL \"$HOSTNAME/exec/gauge-mapper-c8y-stime\" interval=$INTERVAL N:${STAT_MAPPER_ARRAY[14]}"


done

