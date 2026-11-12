#!/bin/sh
# Name: Kauwlendar
# Author: Arisness
# Description: Calendario E-Ink
# Icon: /mnt/us/kauwlendar/icon.png
#
# /mnt/us/documents/Kauwlendar.sh
# Script lanzador para KPM / Scriptlets en Kindle
# Silenciar toda salida estándar para que SH_Integration no dibuje consola de logs en pantalla
exec >/tmp/kauwlendar.log 2>&1

cleanup() {
    # Reanudar la interfaz gráfica de Kindle sin reiniciar el framework
    killall -CONT blanket cvm awesome mesquite 2>/dev/null
    lipc-set-prop com.lab126.powerd preventScreenSaver 0 2>/dev/null
    fbink -q -c -f 2>/dev/null || /usr/sbin/eips -c 2>/dev/null || eips -c 2>/dev/null
}

# Si ya está corriendo, detenerlo y restaurar la pantalla
if pgrep -f "/mnt/us/kauwlendar/kauwlendar" > /dev/null; then
    killall kauwlendar 2>/dev/null
    cleanup
    exit 0
fi

# Asegurar restauración de la interfaz ante cualquier señal o salida
trap cleanup INT TERM EXIT

cd /mnt/us/kauwlendar 2>/dev/null
/mnt/us/kauwlendar/kauwlendar </dev/null >/tmp/kauwlendar.log 2>&1

cleanup
exit 0
