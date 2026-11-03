# Snoopr — Document de conception

- **Date** : 2026-11-03
- **Statut** : brouillon, à valider par le porteur du projet
- **Maquette de référence** : [`2026-11-03-snoopr-maquette.html`](2026-11-03-snoopr-maquette.html). Ouvre-la dans un navigateur : elle est cliquable, avec des données inventées.

Légende : **[décidé]** = choisi par le porteur du projet. **[mon choix]** = décidé pendant la conception, modifiable. Tout le reste est décrit comme attendu.

---

## 1. Intention

Le disque est plein et on veut récupérer de la place vite. Snoopr scanne un disque ou un dossier et montre sous forme de treemap où partent les gigas. On peut ensuite supprimer les gros éléments qui ne servent plus, sans risquer de casser Windows.

Le but n'est pas d'explorer pour le plaisir. C'est de passer de « disque plein » à « j'ai libéré 80 Go » en quelques minutes.

C'est une alternative open source à SpaceSniffer, avec une interface moderne et un treemap plus lisible.

## 2. Objectifs de la v1

1. Scanner un disque ou un dossier rapidement, avec un treemap qui se remplit **pendant** le scan.
2. Afficher un treemap à **dossiers imbriqués** (style B de la maquette) qui reste fluide.
3. Naviguer : clic pour sélectionner, double-clic pour entrer dans un dossier, ← ou fil d'Ariane pour remonter, raccourcis clavier.
4. **Repérer ce qui ne sert plus** : griser ce qui a été modifié récemment (seuils : 6 mois, 1 an, 2 ans).
5. **Supprimer depuis l'appli** avec un panier, une confirmation, et le choix entre la corbeille et la suppression définitive.
6. Montrer clairement que **la corbeille occupe encore de la place**, et permettre de la vider.
7. **Garde-fous** : bloquer la suppression des éléments système et expliquer la bonne méthode à la place. Avertir pour les cas à risque (jeux Steam, OneDrive…).
8. Ouvrir un élément dans l'Explorateur Windows.
9. Être livré en un seul `.exe` portable pour Windows 10/11 x64.

## 3. Hors périmètre de la v1

- Versions Linux et macOS **[décidé : Windows d'abord]**. Le code du cœur doit rester portable quand ça ne coûte rien, mais rien n'est testé ni livré hors de Windows.
- Lecture directe de la MFT NTFS (le mode « WinDirStat/WizTree rapide »). C'est un bon candidat pour la v2.
- Mise à jour en direct du treemap quand des fichiers changent après le scan.
- Champ de recherche et filtres par nom ou par taille. Il a été retiré de la maquette pour garder la barre légère.
- Recherche de doublons, rapports, export.
- Thème clair.
- Installeur, mise à jour automatique, intégration au menu contextuel de l'Explorateur.
- Scan de plusieurs disques en même temps.

## 4. Anti-objectifs (ce qui serait un échec même si « ça marche »)

- Supprimer quoi que ce soit sans confirmation explicite dans la fenêtre de confirmation.
- Permettre de casser Windows depuis l'interface.
- Supprimer **définitivement** un élément alors que l'utilisateur a choisi la corbeille, par exemple parce qu'il est trop gros pour la corbeille, sans le lui dire et sans lui redemander.
- Figer l'interface pendant un scan ou une suppression.
- Envoyer des données sur le réseau (télémétrie, statistiques…).
- Devenir un « nettoyeur magique » qui décide à la place de l'utilisateur. Snoopr montre et aide à choisir, l'utilisateur décide.

## 5. Contraintes

- **Langage : Rust [décidé].**
- **Open source [décidé].** Licence : MIT OR Apache-2.0 **[mon choix]**, la double licence habituelle en Rust.
- Windows 10 et 11 x64. L'appli doit fonctionner **sans droits administrateur**, et le mode admin reste optionnel.
- Aucune dépendance à installer à côté de l'exe.
- Gestion des chemins longs (> 260 caractères).

## 6. Interface

### 6.1 Disposition générale [décidé : version « petite barre en haut » de la maquette]

La fenêtre a trois zones, du haut vers le bas :

1. **Une seule barre fine** (~44 px) avec des boutons.
2. **Le treemap**, sur toute la largeur et toute la hauteur restante. Il n'y a **pas de panneau latéral**.
3. **Une barre d'état** (~30 px).

Le style est sombre et épuré (fond ~`#0f1116`), avec des coins arrondis. La police est celle du système (Segoe UI Variable). La maquette fait référence pour les couleurs, les tailles et les espacements. Les petites différences sont au choix du constructeur.

### 6.2 Barre du haut (de gauche à droite)

| Élément | Comportement |
|---|---|
| Logo « Snoopr » | Décoratif. |
| Menu **Disque** (ex. « C: 398 Go / 476 Go ▾ ») | Liste les disques fixes avec leur occupation, plus « Choisir un dossier… » (sélecteur de dossier Windows). Choisir une entrée lance le scan correspondant. |
| **Rescanner** | Relance le scan de la cible actuelle. Pendant un scan, il devient **Arrêter**. |
| **←** | Remonte d'un niveau. Il est désactivé à la racine. |
| **Fil d'Ariane** (ex. `C: › Users › Lenny`) | Chaque segment est cliquable et permet de remonter directement. |
| Menu **Ancienneté** | Choix entre « Tout afficher », « Pas modifié depuis 6 mois », « 1 an » et « 2 ans ». Le libellé du bouton reflète le choix (voir 6.6). |
| **Ouvrir** | Ouvre l'Explorateur avec l'élément sélectionné en surbrillance. |
| **Ajouter au panier** | Le bouton change selon la sélection : « Ajouter au panier », « Retirer du panier », « Vider la corbeille » (si la corbeille est sélectionnée). Il est désactivé pour la racine, les éléments bloqués, ceux déjà inclus via un dossier parent dans le panier, et ceux déjà dans la corbeille. |
| **Panier** avec un compteur | Ouvre un menu déroulant. Il liste les éléments (nom, taille, ✕ pour retirer) et propose un bouton « Supprimer N éléments · X Go » qui ouvre la confirmation. Quand le panier est vide, un court texte explique que rien n'est effacé sans confirmation. |

### 6.3 Treemap [décidé : style « dossiers imbriqués »]

- **Disposition** : *squarified treemap* (rectangles les plus carrés possibles), trié par taille.
- **Dossiers** : un conteneur arrondi (rayon ~6 px) avec un **bandeau** de ~16 px qui affiche le nom et la taille. Les enfants sont dessinés à l'intérieur, avec un retrait de ~3 px. Le bandeau n'apparaît que si le conteneur est assez grand (repère : ≥ 50 × 36 px).
- **Fichiers** : des blocs arrondis (rayon ~4 px) avec un léger dégradé.
- **Espacement** : environ 1,5 px entre les blocs.
- **Couleurs** : chaque enfant direct de la racine **scannée** a sa propre teinte, et cette teinte reste la même quand on zoome. La luminosité des conteneurs augmente avec la profondeur. Les éléments système bloqués (Windows, etc.) sont désaturés pour s'effacer visuellement **[mon choix]**. La palette exacte est au choix du constructeur.
- **Libellés** : nom et taille si le bloc fait au moins ~58 × 32 px, le nom seul (plus petit) si le bloc fait au moins ~42 × 17 px, et rien en dessous. Quand le filtre d'ancienneté est actif, la deuxième ligne devient « taille · ancienneté » (ex. « 70 Go · 1 an »).
- **Petits fichiers** : les fichiers trop petits pour être visibles sont regroupés, dans chaque dossier, en un seul bloc « N petits fichiers » **[mon choix]**. Le seuil est au choix du constructeur.
- **Profondeur** : on dessine aussi profondément que les blocs restent visibles.
- **Pendant le scan** : le treemap se met à jour régulièrement (repère : environ 4 fois par seconde) sans clignoter de façon gênante.
- **Survol** : le bloc s'éclaircit et une **bulle** suit la souris. Elle affiche le nom, la taille, le pourcentage du disque, « Modifié il y a… », le nombre d'éléments pour un dossier et l'éventuel avertissement de garde-fou. Pour un dossier, elle ajoute « Double-clic pour entrer ».
- **Sélection** : contour blanc de 2 px. Les éléments du panier ont un contour ambre en pointillés.
- **Zoom** : un double-clic sur un dossier en fait la racine de l'affichage. Une courte transition animée serait appréciable, mais elle n'est pas obligatoire.

### 6.4 Barre d'état

- **À gauche** :
  - si rien n'est sélectionné (racine) : `C:\ · 398 Go utilisés sur 476 Go · Scan terminé en 3,8 s · 1 284 312 fichiers` ;
  - pendant un scan : la progression (nombre de fichiers, dossier en cours, temps écoulé) ;
  - sinon : `nom · taille · modifié il y a … · chemin complet`, suivi de l'avertissement de garde-fou s'il y en a un (🔒 en rouge pour un blocage, ⚠ en ambre pour un avertissement).
- **À droite** :
  - `✓ X Go libérés` (cumul de la session) ;
  - s'il y a quelque chose dans la corbeille : `Corbeille : X Go encore sur le disque` avec un bouton **Vider la corbeille**.

### 6.5 Navigation et clavier

- Clic : sélectionner. Clic dans une zone vide : sélectionner le dossier affiché.
- Double-clic sur un dossier : entrer dedans.
- **Retour arrière** : remonter. **Suppr** : ajouter au panier ou en retirer. **Échap** : fermer les menus et la fenêtre de confirmation.
- **Entrée** sur un dossier sélectionné : entrer dedans **[mon choix]**.

### 6.6 Ancienneté (« ce qui ne sert plus ») [décidé]

- L'ancienneté d'un fichier vient de la **plus récente de ses dates de modification et de création** **[mon choix]**.
  - La date de dernier accès n'est pas utilisée, car elle n'est pas fiable sous Windows.
  - Copier, extraire une archive ou restaurer une sauvegarde conserve l'ancienne date de modification. En revanche, la date de création est remise à zéro.
  - Prendre la plus récente des deux évite donc de présenter comme « inutilisé » un contenu qui vient d'arriver. En cas de doute, l'erreur va dans le sens prudent.
- L'ancienneté d'un dossier est celle du fichier **le plus récent** à l'intérieur.
- Quand un seuil est choisi, les fichiers modifiés plus récemment que ce seuil sont **grisés** (très transparents et désaturés). Les candidats à la suppression restent seuls en couleur.

### 6.7 Démarrage [mon choix]

- Au lancement, la zone centrale affiche la liste des disques sous forme de grandes cartes, avec la lettre, le nom et une barre d'occupation. Un clic lance le scan.
- `snoopr.exe <chemin>` lance directement le scan de ce chemin.

## 7. Suppression

### 7.1 Parcours [décidé : suppression dans l'appli, avec panier]

1. L'utilisateur ajoute des éléments au panier. Si on ajoute un dossier, ses descendants déjà présents dans le panier en sont retirés.
2. **Panier → Supprimer** ouvre une fenêtre de confirmation qui contient :
   - le titre « Supprimer N éléments ? » ;
   - la taille totale ;
   - la liste des chemins avec leur taille ;
   - un encadré qui explique que la corbeille garde les fichiers sur le disque et que la suppression définitive est irréversible.
3. Trois boutons : **Annuler**, **Supprimer définitivement** et **Mettre à la corbeille** (action par défaut).
4. Pendant la suppression, l'interface reste réactive et une progression s'affiche.
5. Ensuite :
   - l'arbre est mis à jour **sans rescanner** (tailles des parents recalculées) et l'occupation du disque est relue ;
   - si l'élément affiché ou sélectionné a été supprimé, la vue revient à la racine.

### 7.2 Corbeille

- Le dossier `$Recycle.Bin` du disque apparaît dans le treemap comme un bloc **« Corbeille »**, avec une teinte rouge. Il reflète ce que le scan peut lire, et les éléments qu'on vient d'y envoyer s'y ajoutent.
- Le contenu de la corbeille ne peut pas être supprimé élément par élément. La seule action possible est **Vider la corbeille**, disponible depuis la barre d'état ou en sélectionnant le bloc. Elle vide la corbeille de ce disque, après une confirmation.
- **Obligatoire** : si un élément ne peut pas aller à la corbeille (trop gros, ou disque sans corbeille), on ne le supprime **jamais** définitivement sans l'annoncer.
  - Ces éléments sont repérés **avant** de commencer.
  - L'utilisateur reçoit **une seule question** pour tous ces éléments : les supprimer définitivement, ou les ignorer.
  - « Ignorer » ne retire que ces éléments-là, le reste du panier part normalement à la corbeille **[mon choix]**.

### 7.3 Échecs [mon choix]

- **Mode corbeille** : chaque élément du panier part **en un seul bloc**, comme dans l'Explorateur.
  - Si un fichier à l'intérieur est verrouillé (par exemple utilisé par un programme ouvert), tout cet élément échoue et reste en place intact.
  - Les autres éléments du panier ne sont pas affectés.
- **Mode définitif** : suppression **fichier par fichier, au mieux**.
  - L'attribut « lecture seule » est retiré automatiquement, comme le fait l'Explorateur, puisque l'utilisateur a confirmé.
  - Ce qui ne peut pas être supprimé (fichier en cours d'utilisation, accès refusé) reste en place.
  - Les dossiers concernés restent donc partiellement remplis.
- **Après coup** :
  - une fenêtre récapitule les échecs, avec l'élément et la raison en clair (ex. « utilisé par un autre programme ») ;
  - l'arbre reflète exactement ce qui a été supprimé, y compris les dossiers partiellement vidés.

## 8. Garde-fous [décidé]

Les règles s'appliquent à l'élément et à tous ses descendants.

**Une liste fermée en v1 [mon choix].** Les règles ci-dessous sont la liste complète pour la v1, et c'est contre elle qu'on vérifie la promesse « rien de bloqué n'est supprimable ». Il n'y a pas de détection application par application via le registre. Ajouter des règles plus tard doit rester simple : une table de règles plutôt que du code dispersé.

Les règles sont de deux sortes :
- **Chemins système** : résolus avec les API Windows (dossiers connus) et non écrits en dur avec « C: ».
- **Règles par nom** : elles s'appliquent à **tous les disques** (ex. un `Program Files` ou un `WindowsApps` sur D:).

En cas de conflit entre règles, la plus stricte gagne.

**Bloqué** : bouton désactivé, message 🔒 qui explique la bonne méthode.

| Élément | Message / conseil |
|---|---|
| Racine d'un disque | Ne peut pas être supprimée. |
| Dossier Windows (`%SystemRoot%`) | Passe par le Nettoyage de disque ou Paramètres › Stockage. |
| `Program Files`, `Program Files (x86)` à la racine de **n'importe quel disque** | Désinstalle le programme via Paramètres › Applications. |
| `WindowsApps`, sur n'importe quel disque | Applications du Microsoft Store : désinstalle-les via Paramètres › Applications. |
| `ProgramData` | Données partagées des applications : bloqué par prudence. |
| `pagefile.sys`, `swapfile.sys` | Taille réglable dans Paramètres système avancés › Mémoire virtuelle. |
| `hiberfil.sys` | Désactivable avec `powercfg /h off` (en admin). |
| `System Volume Information`, `Recovery`, `$WinREAgent`, `Boot`, `bootmgr` | Fichiers système. |
| `Windows.old`, `$Windows.~BT`, `$Windows.~WS` | Ancienne installation de Windows : supprime-la via Paramètres › Stockage › Fichiers temporaires. |
| `Users` lui-même, la racine de chaque profil (`Users\<nom>`, `Public`), et **tout** `Users\Default` | Les sous-dossiers d'un profil restent supprimables, pas le profil entier. |
| `AppData` lui-même (le dossier entier) | Contient les réglages de toutes tes applications. |
| `AppData\Roaming\Microsoft`, `AppData\Local\Microsoft`, `AppData\Local\Packages` | Données de Windows (menu Démarrer, recherche, mots de passe enregistrés…) : les supprimer casse ta session. |
| Contenu de la corbeille, élément par élément | Seul « Vider la corbeille » est possible. |

**Avertissement** : suppression autorisée, message ⚠ dans la bulle, la barre d'état et la fenêtre de confirmation.

| Élément | Message |
|---|---|
| Jeux Steam (`…\steamapps\common\…`), Epic Games (`…\Epic Games\…`), Xbox (`XboxGames`), sur n'importe quel disque | Mieux vaut le désinstaller depuis le lanceur du jeu. |
| Dossiers synchronisés avec le cloud : OneDrive (variables d'environnement), Dropbox, Google Drive, iCloud Drive (emplacements par défaut) | La suppression est **aussi faite dans le cloud**. |
| Sous-dossiers de `AppData` non bloqués | Peut effacer des réglages d'applications. Les dossiers de cache et `Temp` sont sans risque. |
| Disques virtuels `*.vhdx` / `*.vhd` (WSL, Docker, machines virtuelles) | Peut contenir une distribution Linux, Docker ou une machine virtuelle entière. |
| Fichiers Outlook `*.pst` / `*.ost` | Peut contenir tes e-mails. |

## 9. Scan

- **Taille comptée : taille réellement occupée sur le disque**, et non la taille « logique » **[mon choix]**, puisque le but est de récupérer de la place. Ainsi, les fichiers compressés NTFS et les fichiers OneDrive « en ligne seulement » comptent pour ce qu'ils occupent vraiment.
- **Liens** : on ne suit pas les points de jonction ni les liens symboliques, pour éviter les doubles comptes et les boucles. Les liens physiques (*hard links*) peuvent être comptés plusieurs fois en v1, ce qui est une limite acceptée. Ils concernent surtout `Windows\WinSxS`, qui est bloqué de toute façon.
- **Espace non analysé** : à la racine d'un disque, un bloc gris hachuré « Non analysé » représente `occupation du disque − total scanné` (ramené à 0 s'il est négatif). Il couvre les dossiers inaccessibles et l'espace réservé par le système (points de restauration, etc.). Sa bulle explique pourquoi **[mon choix]**.
- **Accès refusé** : sans droits admin, certains dossiers sont illisibles. On les compte, on l'indique dans la bulle du bloc « Non analysé » et dans la barre d'état, et on propose **« Relancer en administrateur »**.
- Le scan tourne **en parallèle sur plusieurs threads**, hors du thread de l'interface, et il est annulable.
- **Mémoire** : un arbre compact, avec un repère d'environ 100 octets par entrée. 2 millions de fichiers doivent tenir confortablement en mémoire.

## 10. Choix techniques [mon choix, le porteur m'a laissé l'interface]

- **Interface** : `eframe` / `egui`, avec le treemap dessiné à la main (painter) et un thème sombre personnalisé pour coller à la maquette. Pour les icônes, une police d'icônes compatible egui fera l'affaire.
- **API Windows** : crate `windows`. Pour l'énumération, une API qui renvoie en une passe par dossier la taille allouée, la date de modification et les attributs. `GetFileInformationByHandleEx` avec `FileIdBothDirectoryInfo` en est une piste. Le choix exact revient au constructeur.
- **Structure** : un workspace Cargo avec deux crates :
  - `snoopr-core` : scan, arbre, ancienneté, règles de garde-fous, suppression. Il est sans interface et testable.
  - `snoopr-app` : interface egui.
- **Tests** : des tests unitaires du cœur (disposition du treemap, agrégation des tailles et des dates, règles de garde-fous, suppression sur des dossiers temporaires). Il n'y a pas de tests automatisés de l'interface.
- **Livraison** : un `.exe` x64 portable produit par GitHub Actions et publié dans les *Releases*.

## 11. Critères de réussite

- Sur le vrai disque C: du porteur du projet, on passe du lancement à « X Go libérés » en moins de 5 minutes.
- Le treemap commence à apparaître en moins d'une seconde après le début du scan. Le scan complet d'un disque SSD typique (~500 Go, 1 à 2 millions de fichiers) reste du même ordre que SpaceSniffer sur la même machine. C'est un objectif à mesurer, pas un chiffre contractuel.
- Le survol et le zoom restent fluides, autour de 60 images par seconde, pendant et après le scan.
- Aucun élément bloqué n'est supprimable depuis l'interface, et rien n'est supprimé sans passer par la fenêtre de confirmation.

## 12. Laissé au constructeur

- Les crates précises (énumération, corbeille, icônes) et l'organisation interne du code.
- La palette exacte, les seuils de taille des libellés et le seuil de regroupement des petits fichiers.
- L'animation de zoom (souhaitable, pas obligatoire).
- La formulation exacte des textes, en restant dans l'esprit de la maquette.
- Les détails de la CI et de la publication.

## 13. Langue de l'interface [mon choix, laissé par le porteur]

- **Langues** : français **et** anglais dès la v1, puisque le projet est open source et vise un public plus large que les francophones.
- **Choix de la langue** : on suit automatiquement la langue d'affichage de Windows. C'est le français si Windows est en français, l'anglais dans tous les autres cas. Il n'y a pas de réglage dans l'appli, donc rien à sauvegarder.
- **Formats** : ils suivent la langue.
  - Français : « Go / To », virgule décimale, espace pour les milliers.
  - Anglais : « GB / TB », point décimal, virgule pour les milliers.
  - Les pluriels sont gérés dans les deux langues.
- **Textes** : tous centralisés, pour pouvoir ajouter une langue sans toucher au reste du code.
- **Dépôt** : README et notes de version en anglais, avec un court résumé en français.
