# Cheip OS

Sistema operativo leggero per i Lumia con Snapdragon 400 e 1 GB di RAM:
**Lumia 630, 640, 640 XL, 735, 830**. Base postmarketOS con kernel mainline,
shell propria in Rust + Slint, aggiornamenti OTA firmati.

![Schermate](screens/00-panoramica.png)

## Stato 0.2

| Parte | Stato |
|---|---|
| Launcher stile ChromeOS | griglia di icone tonde senza testo, dock con ricerca, pallini di notifica |
| Shell | blocco, controlli rapidi a icone, app aperte, apertura app che si espande dall'icona |
| Telefono | tastiera, recenti, rubrica, chiamate in uscita e in arrivo via ModemManager |
| Messaggi | conversazioni, invio e ricezione SMS via ModemManager, salvati in locale |
| Browser | modalità lettura: pagine ridotte a testo, titoli e link nativi, schede, preferiti, ricerca |
| File | navigazione nella home e sulle SD, anteprima testo e immagini, nuova cartella, eliminazione |
| Impostazioni | Wi-Fi con scansione e password, Bluetooth, rete mobile, display, suoni, batteria, archiviazione, Android, data e ora, aggiornamenti, info |
| Calendario, Note | come 0.1 |
| Fotocamera | interfaccia pronta, i sensori dei Lumia non hanno driver in mainline |
| Modem | il codice c'è, ma sui Lumia in mainline il modem non è attivo: chiamate e SMS reali partono quando lo sarà |
| Rendering | software su simpledrm oggi, GLES automatico quando c'è `/dev/dri/renderD128` |
| OTA | manifest + repository apk firmato su Cloudflare R2, installazione dalla shell |
| App Android | rilevate da Waydroid con la loro icona e lanciate, ma senza finestra finché la shell non è compositor |
| Immagine per il telefono | CI con pmbootstrap, **mai provata su hardware** |

## Sviluppo sul PC

```sh
cd shell
cargo run                      # finestra 360x640
cargo test --no-default-features --features screens --lib
cargo run --no-default-features --features screens --bin cheip-screens -- ../screens
```

L'ultimo comando renderizza tutte le schermate a 720x1280 in PNG, senza display.

## Struttura

```
shell/ui/          interfaccia Slint: screens, apps, phone, messages, browser, files, settings
shell/src/         controller e ctl_* (telefono, messaggi, web, file, impostazioni),
                   modem (mmcli), web (lettura), files, system, store, OTA
system/            avvio della sessione, servizio OpenRC, helper root, udev, doas
packaging/         APKBUILD di cheip-shell e del pacchetto UI postmarketos-ui-cheip
.github/workflows/ shell.yml (test + screenshot), release.yml (immagine + OTA)
```

## Prima configurazione (una volta)

1. Chiave di firma OTA: `scripts/gen-key.sh`, poi `gh secret set CHEIP_APK_KEY < cheip-os.rsa`
   e commit di `keys/cheip-os.rsa.pub`. La privata non va mai nel repo.
2. Bucket R2 servito su `updates.cheipstudio.org`, poi i secret
   `R2_BUCKET`, `R2_ACCOUNT_ID`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY`.
   Senza questi la release si fa lo stesso, solo senza upload OTA.
3. Dominio diverso? Cambialo in `system/update.json` e
   `packaging/postmarketos-ui-cheip/postmarketos-ui-cheip.post-install`.

## Release

```sh
git tag -a v0.1.1 -m "Animazioni più rapide, fix calendario"
git push origin v0.1.1
```

Il messaggio del tag diventa la nota dell'aggiornamento mostrata sul telefono.
La CI produce l'immagine `.img.xz`, i pacchetti armv7, il repository firmato e
`manifest.json`. I telefoni lo vedono da Controlli o Impostazioni, tasto Controlla.

## Installazione sul telefono

Il bootloader va sbloccato e serve una catena di avvio che carichi un kernel
Linux al posto di Windows. Il porting usato è quello generico MSM8226 di
postmarketOS (`qcom-msm8226`): segui la pagina wiki del tuo modello per il boot,
poi scrivi l'immagine della release. Il primo obiettivo è vedere la shell con il
renderer software: se arriva quella, il resto è sviluppo.

## Prossime milestone

- **0.3** la shell diventa compositor Wayland (Smithay): finestre Android via Waydroid
  con la stessa animazione di apertura delle app di sistema, e browser con motore WPE WebKit.
- **0.4** GPU: Adreno 305 con freedreno, display MDP5/DSI e driver pannello per modello.
  È il passo che porta ai 60 fps fissi.
- **0.5** OTA A/B con RAUC: sistema in sola lettura, rollback automatico se il boot fallisce.
- Fotocamera con libcamera, solo se arrivano driver per i sensori.

## Licenze

Codice GPL-3.0-or-later. Font Inter e Space Grotesk sotto SIL Open Font License 1.1.
