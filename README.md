# Snoopr

[![CI](https://github.com/LnYKLl/Snoopr/actions/workflows/ci.yml/badge.svg)](https://github.com/LnYKLl/Snoopr/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/LnYKLl/Snoopr)](https://github.com/LnYKLl/Snoopr/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

**See where your disk space really goes, then free it up safely.**

Snoopr is a free, open-source disk-usage visualizer for Windows, in the spirit of SpaceSniffer. It scans a drive in seconds and draws it as a treemap: every file is a block sized by the space it takes, coloured by what it is. Spot the big things you no longer use, right-click, delete. Guard-rails keep Windows and your programs out of harm's way.

**[⬇ Download snoopr.exe](https://github.com/LnYKLl/Snoopr/releases/latest/download/snoopr.exe)** · Windows 10/11, 64-bit · single portable file, nothing to install · [Version française plus bas](#français)

![Snoopr screenshot](docs/screenshot.png)

## Features

- **Fast.** A multi-threaded scan that you can cancel. On the author's PC, it reads 1.4 million files in about 5 seconds. The treemap fills in while the scan runs, so you can start looking right away.
- **Readable treemap.** Folders are boxes with their name and size. Files are blocks coloured by type: games, videos and images, archives and installers, virtual disks, cache and temporary files, programs, system, and other. A legend reminds you which colour is which. Double-click a folder to zoom in.
- **What is no longer used.** The age filter greys out everything modified recently, so only the old, forgotten stuff stays in colour.
- **Right-click to delete.** Right-click a block and choose "Delete…". A confirmation shows exactly what will go and offers the Recycle Bin by default.
- **Cleanup basket.** Collect several items, review them, then delete them all in one confirmed step.
- **Smooth animations.** Blocks glide to their new place as the scan progresses and when you delete something. A deleted block fades away, an item sent to the Recycle Bin flies into it, and zooming feels like a camera move. Animations follow the Windows "Animation effects" setting.
- **True sizes.** Snoopr shows the space actually allocated on disk, so NTFS compression and OneDrive online-only files count for what they really use.
- **Honest about what it cannot see.** A hatched "Not scanned" block stands for folders it could not read and space reserved by Windows. You can restart as administrator to see more.
- **French and English**, following the Windows language.
- **Private.** No installation, no administrator rights needed, no network access, no telemetry.

## Getting started

1. **Download** [`snoopr.exe`](https://github.com/LnYKLl/Snoopr/releases/latest/download/snoopr.exe) and double-click it.
   - The file is not code-signed, so Windows SmartScreen may show "Windows protected your PC". Click **More info**, then **Run anyway**.
2. **Pick a drive** (or "Choose a folder…"). The scan starts at once.
3. **Hover** a block to see its name, size, share of the disk, age and type.
4. **Double-click** a folder to go inside. **Backspace** or the back arrow goes up, and the breadcrumb at the top jumps to any parent.
5. **Right-click** a big block you no longer need, then choose **Delete…**.
6. **Confirm.** "Move to Recycle Bin" is the default. "Delete permanently" frees the space right away but cannot be undone.
7. **Empty the Recycle Bin.** Items in the Recycle Bin still take space. Use **Empty Recycle Bin** in the status bar, or right-click the Recycle Bin block, to really free it.

A "✓ 12.3 GB freed" badge counts up what you just freed, and the status bar keeps the total for the session.

### Colours

| Colour | Content |
|---|---|
| Red | Games (everything under `steamapps`, Epic Games and GOG Games libraries, game data files) |
| Purple | Videos and images |
| Orange | Archives and installers (`.zip`, `.7z`, `.iso`, `.msi`…) |
| Teal | Virtual disks (`.vhdx`, `.vmdk`, `.vdi`…) |
| Green | Cache and temporary files |
| Blue | Programs (`.exe`, `.dll`…) |
| Slate grey | System (everything under Windows) |
| Grey | Other |

A small folder is drawn as one solid block, in the colour of what weighs most inside it. Hatched grey means "Not scanned".

### Finding what you no longer use

The clock button in the top bar dims everything modified in the last 6 months, 1 year or 2 years. What stays in colour has not been touched for a long time, and is usually the best thing to clean up. A file's age is the most recent of its modified and created dates, so something you just copied or extracted never looks old.

### Keyboard and mouse

| Action | Effect |
|---|---|
| Click | Select a block (the status bar shows its full path) |
| Double-click, or Enter | Go into the selected folder |
| Right-click | Menu: go into folder, show in File Explorer, add to or remove from the basket, delete |
| Del | Delete the selected item, after confirmation, as in File Explorer |
| Backspace | Go up one folder |
| Esc | Close menus and dialogs |

### Command line

```
snoopr.exe [path]
```

Without an argument, Snoopr shows your drives. With a path, it scans that path directly.

## Safety

Nothing is ever deleted without going through the confirmation window.

- **Guard-rails.** Critical items cannot be deleted. The menu greys out "Delete…" and explains why.
  - **Protected:** drive roots, the Windows folder, Program Files, WindowsApps, ProgramData, `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `System Volume Information`, `Recovery`, user profile roots, `AppData` itself and its Microsoft/Packages folders, `Windows.old`, and a few more.
  - **Name-based rules** (`Program Files`, `WindowsApps`, the page and hibernation files, `System Volume Information`, `Recovery`, `Windows.old`…) apply on every drive.
  - **The Windows folder, `ProgramData` and user-profile rules** apply to the system's own folders, found through the Windows known folders.
- **Warnings** (deletion allowed, but you are told): game launcher folders, cloud-synced folders (the deletion also happens in the cloud), other `AppData` subfolders, `.vhdx`/`.vhd` virtual disks, and Outlook `.pst`/`.ost` files.
- **Recycle Bin by default.** An item too big for the Recycle Bin is never deleted permanently behind your back. You are asked once whether to delete it permanently or skip it.
- **Permanent deletion** is a separate, explicit button. It is built to resist folder-swap tricks (junction races), and it never follows links outside the folder you chose.
- **Clear failures.** Anything that could not be deleted is listed afterwards, with a plain reason ("in use by another program", "access denied"…).
- **No accidental clicks.** A freshly opened confirmation ignores clicks for a moment, so a double-click can never confirm it, and the Enter key never confirms a deletion.

## FAQ

**I deleted things but my free space did not change.**
They are in the Recycle Bin, which keeps them on disk. Empty it from the status bar or by right-clicking the Recycle Bin block. To skip the bin, choose "Delete permanently".

**What is the hatched "Not scanned" block?**
It stands for folders Snoopr was not allowed to read and for space Windows reserves for itself, such as restore points. Click "Restart as administrator" in the status bar to read more of it.

**Why are the sizes different from File Explorer's "Size"?**
Snoopr shows the space files really take on disk ("size on disk"). Compressed files and OneDrive online-only files take less than their nominal size.

**Why can't I delete this folder?**
It is protected by a guard-rail; the right-click menu and the tooltip say why. To remove a program, use Windows Settings › Apps, so that it is uninstalled properly.

**Does Snoopr send anything over the internet?**
No. It makes no network access at all and has no telemetry.

## Building from source

Requirements: Windows 10/11 x64 and Rust 1.95 or later (edition 2024).

```
cargo build --release
```

The executable is `target\release\snoopr-app.exe`. It links the C runtime statically, so it runs without the Visual C++ Redistributable. Tagged releases (`v*`) are built by GitHub Actions and published as `snoopr.exe`.

Before sending a change, run:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The workspace has two crates:
- `crates/snoopr-core`: the scan, the tree, the treemap layout, the guard-rails and the deletion. It contains no UI code.
- `crates/snoopr-app`: the egui/eframe interface.

---

## Français

**Voir où part vraiment l'espace disque, puis faire de la place sans risque.**

Snoopr est un visualiseur d'espace disque gratuit et open source pour Windows, dans l'esprit de SpaceSniffer. Il analyse un disque en quelques secondes et l'affiche en treemap : chaque fichier est un bloc dont la taille correspond à la place qu'il occupe, et dont la couleur dit ce qu'il est. On repère les gros trucs qui ne servent plus, clic droit, on supprime. Des garde-fous protègent Windows et les programmes.

**[⬇ Télécharger snoopr.exe](https://github.com/LnYKLl/Snoopr/releases/latest/download/snoopr.exe)** · Windows 10/11, 64 bits · un seul fichier portable, rien à installer

### Fonctionnalités

- **Rapide.** Le scan utilise plusieurs cœurs et peut être arrêté. Sur le PC de l'auteur, il lit 1,4 million de fichiers en 5 secondes environ. Le treemap se remplit pendant le scan.
- **Treemap lisible.** Les dossiers sont des cadres avec leur nom et leur taille. Les fichiers sont des blocs colorés selon leur type : jeux, vidéos et images, archives et installateurs, disques virtuels, cache et fichiers temporaires, programmes, système, autres. Une légende rappelle les couleurs. Un double-clic entre dans un dossier.
- **Ce qui ne sert plus.** Le filtre d'ancienneté grise tout ce qui a été modifié récemment : seuls les vieux fichiers oubliés restent en couleur.
- **Clic droit pour supprimer.** Clic droit sur un bloc, puis « Supprimer… ». Une confirmation montre exactement ce qui part et propose la corbeille par défaut.
- **Panier de nettoyage.** On rassemble plusieurs éléments, on vérifie, puis on supprime tout en une seule confirmation.
- **Animations fluides.**
  - Les blocs glissent vers leur nouvelle place pendant le scan et après une suppression.
  - Un bloc supprimé s'efface, et un élément mis à la corbeille s'envole dedans.
  - Le zoom ressemble à un mouvement de caméra.
  - Les animations suivent le réglage « Effets d'animation » de Windows.
- **Vraies tailles.** Snoopr affiche la place réellement occupée sur le disque. Les fichiers compressés NTFS et les fichiers OneDrive « en ligne uniquement » comptent pour ce qu'ils occupent vraiment.
- **Honnête sur ce qu'il ne voit pas.** Un bloc hachuré « Non analysé » représente les dossiers illisibles et l'espace réservé par Windows. On peut relancer en administrateur pour en voir plus.
- **Français et anglais**, selon la langue de Windows.
- **Respect de la vie privée.** Pas d'installation, pas besoin des droits administrateur, aucun accès réseau, aucune télémétrie.

### Prise en main

1. **Télécharger** [`snoopr.exe`](https://github.com/LnYKLl/Snoopr/releases/latest/download/snoopr.exe) et double-cliquer dessus.
   - Le fichier n'est pas signé, donc Windows SmartScreen peut afficher « Windows a protégé votre ordinateur ». Cliquer sur **Informations complémentaires**, puis sur **Exécuter quand même**.
2. **Choisir un disque** (ou « Choisir un dossier… »). Le scan démarre tout de suite.
3. **Survoler** un bloc pour voir son nom, sa taille, sa part du disque, son ancienneté et son type.
4. **Double-cliquer** sur un dossier pour entrer dedans. **Retour arrière** ou la flèche permet de remonter, et le fil d'Ariane en haut ramène à n'importe quel dossier parent.
5. **Clic droit** sur un gros bloc qui ne sert plus, puis **Supprimer…**.
6. **Confirmer.** « Mettre à la corbeille » est le choix par défaut. « Supprimer définitivement » libère la place tout de suite, mais c'est irréversible.
7. **Vider la corbeille.** Ce qui est dans la corbeille occupe encore de la place. Le bouton **Vider la corbeille** de la barre d'état, ou un clic droit sur le bloc Corbeille, la libère vraiment.

Une pastille « ✓ 12,3 Go libérés » compte ce qui vient d'être libéré, et la barre d'état garde le total de la session.

### Couleurs

| Couleur | Contenu |
|---|---|
| Rouge | Jeux (tout ce qui est sous `steamapps`, les bibliothèques Epic Games et GOG Games, les fichiers de données de jeux) |
| Violet | Vidéos et images |
| Orange | Archives et installateurs (`.zip`, `.7z`, `.iso`, `.msi`…) |
| Turquoise | Disques virtuels (`.vhdx`, `.vmdk`, `.vdi`…) |
| Vert | Cache et fichiers temporaires |
| Bleu | Programmes (`.exe`, `.dll`…) |
| Gris ardoise | Système (tout ce qui est sous Windows) |
| Gris | Autres |

Un petit dossier est dessiné comme un seul bloc plein, de la couleur de ce qui pèse le plus à l'intérieur. Le gris hachuré signifie « Non analysé ».

### Repérer ce qui ne sert plus

Le bouton horloge de la barre du haut grise tout ce qui a été modifié depuis moins de 6 mois, 1 an ou 2 ans. Ce qui reste en couleur n'a pas été touché depuis longtemps : ce sont souvent les meilleurs candidats au nettoyage.

L'ancienneté d'un fichier est la plus récente de ses dates de modification et de création. Un fichier qu'on vient de copier ou d'extraire ne paraît donc jamais vieux.

### Clavier et souris

| Action | Effet |
|---|---|
| Clic | Sélectionner un bloc (la barre d'état affiche son chemin complet) |
| Double-clic ou Entrée | Entrer dans le dossier sélectionné |
| Clic droit | Menu : entrer dans le dossier, afficher dans l'Explorateur, ajouter au panier ou en retirer, supprimer |
| Suppr | Supprimer l'élément sélectionné, après confirmation, comme dans l'Explorateur |
| Retour arrière | Remonter d'un dossier |
| Échap | Fermer les menus et les fenêtres |

### Ligne de commande

```
snoopr.exe [chemin]
```

Sans argument, Snoopr affiche les disques. Avec un chemin, il analyse directement ce chemin.

### Sécurité

Rien n'est jamais supprimé sans passer par la fenêtre de confirmation.

- **Garde-fous.** Les éléments critiques ne peuvent pas être supprimés : le menu grise « Supprimer… » et explique pourquoi.
  - **Éléments protégés :**
    - les racines de disque, le dossier Windows, Program Files, WindowsApps, ProgramData ;
    - `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`, `System Volume Information`, `Recovery`, `Windows.old` ;
    - les racines des profils utilisateur, `AppData` lui-même et ses dossiers Microsoft et Packages ;
    - et quelques autres.
  - **Règles par nom :** elles s'appliquent sur tous les disques.
  - **Dossier Windows, ProgramData et profils :** ces règles visent les vrais dossiers du système, trouvés via les dossiers connus de Windows.
- **Avertissements** : la suppression reste possible, mais on est prévenu. C'est le cas pour :
  - les dossiers des lanceurs de jeux ;
  - les dossiers synchronisés dans le cloud (la suppression se fait aussi dans le cloud) ;
  - les autres sous-dossiers d'`AppData` ;
  - les disques virtuels `.vhdx` et `.vhd` ;
  - les fichiers Outlook `.pst` et `.ost`.
- **Corbeille par défaut.** Un élément trop gros pour la corbeille n'est jamais supprimé définitivement en douce : Snoopr demande une fois s'il faut le supprimer définitivement ou l'ignorer.
- **Suppression définitive** : un bouton à part, explicite. Elle résiste aux tours de passe-passe de dossiers (attaques par jonction) et ne suit jamais un lien hors du dossier choisi.
- **Échecs expliqués.** Ce qui n'a pas pu être supprimé est listé ensuite, avec une raison claire (« utilisé par un autre programme », « accès refusé »…).
- **Pas de clic accidentel.** Une confirmation qui vient de s'ouvrir ignore les clics pendant un court instant, donc un double-clic ne peut jamais la valider. La touche Entrée ne valide jamais une suppression.

### Questions fréquentes

**J'ai supprimé des choses mais l'espace libre n'a pas bougé.**
Elles sont dans la corbeille, qui les garde sur le disque. Il faut la vider depuis la barre d'état, ou avec un clic droit sur le bloc Corbeille. Pour éviter la corbeille, choisir « Supprimer définitivement ».

**C'est quoi le bloc hachuré « Non analysé » ?**
Ce sont les dossiers que Snoopr n'a pas eu le droit de lire, et l'espace que Windows se réserve (points de restauration…). « Relancer en administrateur », dans la barre d'état, permet d'en lire davantage.

**Pourquoi les tailles diffèrent de la « Taille » de l'Explorateur ?**
Snoopr affiche la place réellement occupée sur le disque (« taille sur le disque »). Les fichiers compressés et les fichiers OneDrive en ligne uniquement occupent moins que leur taille nominale.

**Pourquoi je ne peux pas supprimer ce dossier ?**
Un garde-fou le protège ; le menu du clic droit et la bulle disent pourquoi. Pour enlever un programme, passer par Paramètres › Applications de Windows, afin qu'il soit désinstallé proprement.

**Snoopr envoie-t-il quelque chose sur Internet ?**
Non. Aucun accès réseau, aucune télémétrie.

### Compiler soi-même

Il faut Windows 10/11 x64 et Rust 1.95 ou plus récent (édition 2024).

```
cargo build --release
```

L'exécutable est `target\release\snoopr-app.exe`. Il embarque la bibliothèque C, donc il fonctionne sans le Visual C++ Redistributable. Les versions taguées (`v*`) sont compilées par GitHub Actions et publiées sous le nom `snoopr.exe`.

## License

Dual-licensed under MIT OR Apache-2.0, at your option. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).

Double licence MIT ou Apache-2.0, au choix.
