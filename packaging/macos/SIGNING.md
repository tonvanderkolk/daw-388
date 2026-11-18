# Ondertekenen en notariseren (macOS)

`bundle.sh` ondertekent de app met een Developer ID-certificaat en de hardened runtime
(`entitlements.plist`: alleen de microfoon) en laat de DMG notariseren. In GitHub Actions gebeurt
dat bij elke release zodra de vijf secrets hieronder bestaan; zonder secrets blijft het ad hoc.

## 1. Het certificaat (eenmalig, geldig vijf jaar)

1. Xcode, Settings, Accounts: meld je aan met je Apple ID van het Developer-account.
2. Manage Certificates..., +, **Developer ID Application**.
3. Sleutelhangertoegang, Mijn certificaten: rechtsklik op "Developer ID Application: ...",
   Exporteer, als `.p12` met een sterk wachtwoord.
4. Base64 ervan maken (komt op het klembord):

   ```bash
   base64 -i ~/Desktop/Certificaten.p12 | pbcopy
   ```

## 2. De sleutel voor notariseren

1. App Store Connect, Users and Access, Integrations, App Store Connect API, Team Keys.
2. Maak een sleutel met de rol **Developer**. Download het `.p8`-bestand (dat kan maar één keer).
3. Noteer de **Key ID** (bij de sleutel) en de **Issuer ID** (boven de lijst).

## 3. De secrets in GitHub

Repository, Settings, Secrets and variables, Actions, New repository secret:

| Secret                | Inhoud                                      |
|-----------------------|---------------------------------------------|
| `MACOS_CERT_P12`      | de base64-tekst van stap 1.4                |
| `MACOS_CERT_PASSWORD` | het wachtwoord van de `.p12`                |
| `NOTARY_KEY_P8`       | de hele inhoud van het `.p8`-bestand        |
| `NOTARY_KEY_ID`       | de Key ID                                   |
| `NOTARY_ISSUER`       | de Issuer ID                                |

Bewaar de `.p12` en de `.p8` daarna veilig buiten de repository.

## 4. Proberen

Actions, de workflow, Run workflow (handmatig). In de log van de macOS-job staat dan
`Accepted` van `notarytool` en `source=Notarized Developer ID` van `spctl`.

## Lokaal

```bash
SIGN_IDENTITY="Developer ID Application: <naam> (<team-id>)" \
NOTARY_KEY=~/keys/AuthKey_XXXX.p8 NOTARY_KEY_ID=XXXX NOTARY_ISSUER=<issuer-id> \
packaging/macos/bundle.sh 0.0.0 dist target/release/daw388
```
