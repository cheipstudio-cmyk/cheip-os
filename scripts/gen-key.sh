#!/bin/sh
# Genera la coppia di chiavi che firma i pacchetti OTA. Da lanciare UNA volta.
# La pubblica va nel repo (keys/), la privata SOLO nel secret GitHub CHEIP_APK_KEY.
set -eu
cd "$(dirname "$0")/.."
[ -f keys/cheip-os.rsa.pub ] && { echo "keys/cheip-os.rsa.pub esiste già, non la rigenero"; exit 1; }
umask 077
openssl genrsa -out cheip-os.rsa 4096
openssl rsa -in cheip-os.rsa -pubout -out keys/cheip-os.rsa.pub
echo
echo "Fatto. Ora:"
echo "  1. gh secret set CHEIP_APK_KEY < cheip-os.rsa"
echo "  2. git add keys/cheip-os.rsa.pub && git commit"
echo "  3. conserva cheip-os.rsa offline e cancellalo da qui"
