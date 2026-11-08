#!/bin/sh
# /mnt/us/documents/Kauwlendar.sh

if pgrep -f "/mnt/us/kauwlendar/kauwlendar" > /dev/null; then
    killall kauwlendar
    start powerd 2>/dev/null
    start framework 2>/dev/null
    eips 2 2 "=== Stopping Dashboard ==="
    sleep 3
else
    stop framework 2>/dev/null
    stop powerd 2>/dev/null
    /mnt/us/kauwlendar/kauwlendar &
fi