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
sed -i -e "s|^\(\s*\)fdtdir .*|\1fdt /$DTB|" -e "s|append quiet |append console=tty0 clk_ignore_unused pd_ignore_unused |" "$TMP/ext.conf"
grep -q "fdt /$DTB" "$TMP/ext.conf"
debugfs -w -R "rm /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -w -R "write $TMP/ext.conf /extlinux/extlinux.conf" "$TMP/boot" >/dev/null 2>&1
debugfs -R "cat /extlinux/extlinux.conf" "$TMP/boot" 2>/dev/null
# All four cores: the Lumia UEFI parks CPU1-3 at the parking protocol mailboxes
# 0x301000 + n * 0x1000 (ACPI MADT of the MSM8x26 Lumias). Keep that memory
# untouched and boot the cores through it (kernel/0001-ARM-qcom-parking-protocol-SMP.patch)
debugfs -R "dump /$DTB $TMP/dtb" "$TMP/boot" >/dev/null 2>&1
patch_dtb() {
	fdtput -c "$TMP/dtb" /reserved-memory/mppark@300000
	fdtput -t x "$TMP/dtb" /reserved-memory/mppark@300000 reg 300000 40000
	fdtput -t s "$TMP/dtb" /reserved-memory/mppark@300000 no-map ""
	for n in 0 1 2 3; do
		fdtput -t s "$TMP/dtb" /cpus/cpu@$n enable-method microsoft,parking-protocol
		fdtput -t x "$TMP/dtb" /cpus/cpu@$n cpu-release-addr "$(printf %x $((0x301000 + n * 0x1000)))"
	done
	fdtget "$TMP/dtb" /cpus/cpu@1 enable-method
	debugfs -w -R "rm /$DTB" "$TMP/boot" >/dev/null 2>&1
	debugfs -w -R "write $TMP/dtb /$DTB" "$TMP/boot" >/dev/null 2>&1
}
if [ -s "$TMP/dtb" ]; then
	# Never block the image: without this the phone still boots on one core
	( patch_dtb ) || echo "DTB patch for the parking protocol failed" >&2
else
	echo "DTB /$DTB not found in the boot partition" >&2
fi
dd if="$TMP/boot" of="$IMG" bs=512 seek="$START" conv=notrunc status=none
rm -rf "$TMP"
