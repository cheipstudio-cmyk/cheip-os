#!/bin/sh
# lk1st on Lumia has no device DTB bundled, so "fdtdir" can't be resolved:
# point extlinux.conf at the right DTB explicitly. Usage: fix-extlinux.sh disk.img dtb-name
set -eu
IMG="$1"; DTB="$2"
START=$(sfdisk -d "$IMG" | awk -F'[=,]' '/^.*1 :/ {gsub(/ /,"",$2); print $2; exit}')
SIZE=$(sfdisk -d "$IMG" | awk -F'[=,]' '/^.*1 :/ {gsub(/ /,"",$4); print $4; exit}')
TMP=$(mktemp -d)
dd if="$IMG" of="$TMP/boot" bs=512 skip="$START" count="$SIZE" status=none
debugfs -R "cat /extlinux/extlinux.conf" "$TMP/boot" 2>/dev/null > "$TMP/ext.conf"
sed -i -e "s|^\(\s*\)fdtdir .*|\1fdt /$DTB|" -e "s|append quiet |append console=tty0 lk2nd.pass-simplefb=autorefresh,relocate,xrgb8888 |" "$TMP/ext.conf"
grep -q "fdt /$DTB" "$TMP/ext.conf"
debugfs -w -R "rm /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -w -R "write $TMP/ext.conf /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -R "cat /extlinux/extlinux.conf" "$TMP/boot" 2>/dev/null
dd if="$TMP/boot" of="$IMG" bs=512 seek="$START" conv=notrunc status=none
rm -rf "$TMP"
