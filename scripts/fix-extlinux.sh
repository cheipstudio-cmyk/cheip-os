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
sed -i -e "s|^\(\s*\)fdtdir .*|\1fdt /$DTB|" -e "s|append quiet |append console=tty0 clk_ignore_unused pd_ignore_unused cma=128M |" "$TMP/ext.conf"
grep -q "fdt /$DTB" "$TMP/ext.conf"
debugfs -w -R "rm /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -w -R "write $TMP/ext.conf /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -R "cat /extlinux/extlinux.conf" "$TMP/boot" 2>/dev/null
# All four cores: the Lumia UEFI parks CPU1-3 at the parking protocol mailboxes
# 0x301000 + n * 0x1000 (ACPI MADT of the MSM8x26 Lumias). Keep that memory
# untouched and boot the cores through it (kernel/0001-ARM-qcom-parking-protocol-SMP.patch)
debugfs -R "dump /$DTB $TMP/dtb" "$TMP/boot" >/dev/null 2>&1
patch_dtb() {
	# Only replace the DTB once the new one exists (errexit is off inside "||")
	python3 "$(dirname "$0")/dtb-lumia.py" "$TMP/dtb" "$TMP/dtb.new" && [ -s "$TMP/dtb.new" ] || return 1
	debugfs -w -R "rm /$DTB" "$TMP/boot" >/dev/null 2>&1
	debugfs -w -R "write $TMP/dtb.new /$DTB" "$TMP/boot" >/dev/null 2>&1
}
if [ -s "$TMP/dtb" ]; then
	# Never block the image: without this the phone still boots on one core
	( patch_dtb ) || echo "DTB patch (SMP, Wi-Fi) failed" >&2
else
	echo "DTB /$DTB not found in the boot partition" >&2
fi
dd if="$TMP/boot" of="$IMG" bs=512 seek="$START" conv=notrunc status=none
rm -rf "$TMP"
