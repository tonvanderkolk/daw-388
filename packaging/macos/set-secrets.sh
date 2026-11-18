#!/bin/sh
# Put the five signing secrets into the GitHub repository (see SIGNING.md).
#
#   packaging/macos/set-secrets.sh <certificate.p12> <issuer-id> [<AuthKey_XXXX.p8>]
#
# Asks for the .p12 password. Without a .p8 the newest key in ~/.appstoreconnect/private_keys
# is used. Nothing is printed or written to disk.
set -eu
p12=$1
issuer=$2
p8=${3:-$(ls -t "$HOME"/.appstoreconnect/private_keys/AuthKey_*.p8 | head -1)}
key_id=$(basename "$p8" .p8 | sed 's/^AuthKey_//')
printf 'Password of %s: ' "$p12"
stty -echo
read -r password
stty echo
echo
base64 -i "$p12" | gh secret set MACOS_CERT_P12
printf '%s' "$password" | gh secret set MACOS_CERT_PASSWORD
gh secret set NOTARY_KEY_P8 <"$p8"
printf '%s' "$key_id" | gh secret set NOTARY_KEY_ID
printf '%s' "$issuer" | gh secret set NOTARY_ISSUER
gh secret list
