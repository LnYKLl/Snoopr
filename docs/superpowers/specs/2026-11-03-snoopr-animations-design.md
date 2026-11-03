# Snoopr : clic droit, animations et treemap fluide

**Date :** 2026-11-03
**Complète :** `2026-11-03-snoopr-design.md`. Ce document l'emporte en cas de conflit (§6.3 zoom, §6.5 touche Suppr).

**Origine :** l'utilisateur a demandé :
- « on pourrait ajouter des animations dans l'application ? et aussi rendre fluide quand le tree se modifie faire un beau truc » ;
- « j'aurais pensé qu'on pourrait clic droit pour supprimer ? faut que ce soit noob friendly ».

Il a ensuite demandé de « se lancer sans questions ». Tout ce qui suit est donc **mon choix**, sauf ces trois demandes : le clic droit pour supprimer, les animations, et un treemap fluide quand l'arbre change.

## 1. Intention

Quelqu'un qui n'y connaît rien doit pouvoir faire de la place sans rien lire :
1. il voit un gros bloc ;
2. il fait un clic droit dessus et choisit « Supprimer… » ;
3. il confirme ;
4. le bloc disparaît en douceur, les voisins prennent sa place et « ✓ 12,3 Go libérés » s'affiche.

Rien ne doit sauter d'un coup à l'écran : chaque changement du treemap doit se lire comme un mouvement.

**Objectifs :**
- supprimer un élément en deux clics (clic droit, puis la confirmation) ;
- des transitions fluides du treemap (zoom, suppression, scan, rescans) ;
- quelques micro-animations qui rendent l'appli vivante.

**Hors objectifs :**
- du son, des confettis ou des particules ;
- un panneau de réglages ;
- une nouvelle dépendance ;
- un changement des garde-fous.

**Anti-objectifs**, à refuser même si « ça marche » :
- une animation qui retarde l'effet d'un clic ou bloque une saisie ;
- des saccades pendant le scan d'un gros disque ;
- une information fausse, même pendant une transition. La bulle et la barre d'état décrivent toujours l'état réel.

## 2. Clic droit

### 2.1 Comportement

- Un clic droit sur un bloc **sélectionne** ce bloc, comme dans l'Explorateur, puis ouvre un menu sous la souris.
- Un clic droit dans une zone vide vise le dossier affiché.
- Un bloc « N petits fichiers » vise son dossier parent, comme le clic gauche.

### 2.2 Contenu du menu (de haut en bas)

1. **En-tête** : le nom et la taille de l'élément visé, en petites capitales comme les autres menus.
2. **« Entrer dans le dossier »** : seulement pour un dossier qui n'est pas déjà le dossier affiché.
3. **« Afficher dans l'Explorateur »**.
4. Un séparateur.
5. **« Ajouter au panier »** ou **« Retirer du panier »**, selon `primary_action` (Add ou Remove). La ligne est absente dans les autres cas.
6. **« Supprimer… »**, en rouge, avec l'icône de corbeille. Elle ouvre la confirmation de §7.1 **pour cet élément seul**. Le panier n'est pas touché.
   - Sur la Corbeille, cette ligne devient **« Vider la corbeille… »**, qui ouvre la confirmation de §7.2.
   - Quand la suppression est impossible, la ligne est grisée et une ligne d'explication en gris suit :

     | Cas | Explication affichée |
     |---|---|
     | Élément bloqué | Le message du garde-fou (`block_message`), précédé de 🔒. |
     | Déjà couvert par un dossier du panier | « Déjà dans le panier avec « Parent » ». |
     | Dans la corbeille | « Déjà dans la corbeille ». |
     | Scan ou suppression en cours | « Disponible à la fin du scan » (ou « …de la suppression »). |
     | Racine, ou bloc « Non analysé » | Ligne grisée, sans explication. |

### 2.3 Confirmation d'un seul élément

- C'est la fenêtre de §7.1. Le titre devient « Supprimer « Videos » ? » (EN : “Delete “Videos”?”). La liste ne contient que cet élément.
- Les boutons, le cas « trop gros pour la corbeille », le délai anti-double-clic et la vérification par la session avant tout accès au disque restent identiques.
- La vérification d'un élément seul est **exactement** celle d'un panier qui ne contiendrait que lui.

### 2.4 Touche Suppr (remplace §6.5)

- **Suppr** ouvre la confirmation pour l'élément sélectionné, comme dans l'Explorateur.
- Si l'élément sélectionné est la Corbeille, Suppr ouvre la confirmation pour la vider.
- Suppr ne fait rien quand :
  - la sélection est le dossier affiché lui-même, car rien n'est visiblement sélectionné ;
  - ou la suppression est impossible.
- Ajouter au panier ou en retirer se fait avec le bouton de la barre du haut ou avec le menu.

### 2.5 Indice dans la bulle

La dernière ligne de la bulle devient :

| Cas | Texte |
|---|---|
| Dossier supprimable | « Double-clic pour entrer · clic droit pour supprimer » |
| Dossier non supprimable | « Double-clic pour entrer » |
| Fichier supprimable | « Clic droit pour supprimer » |
| Autres cas | Pas d'indice. |

Ici, « supprimable » veut dire que `primary_action` vaut Add ou Remove.

Pas d'indice sur un bloc « N petits fichiers » : son clic droit vise le dossier parent tout entier.

## 3. Treemap animé

### 3.1 Reflux (même dossier affiché)

Ce cas couvre les mises à jour du scan, une suppression et un rescan.

- **Durée et courbe** : 260 ms, *ease-out* cubique.
- **Bloc présent avant et après** : il glisse et change de taille, de ce qui est affiché à l'écran vers sa nouvelle place.
- **Bloc qui apparaît** : il part de 85 % de sa taille, centré, avec une opacité de 0, et grandit jusqu'à sa taille finale en devenant opaque.
- **Bloc qui disparaît** (on parle de « fantôme ») :
  - s'il est encore dans l'arbre et qu'un de ses ancêtres est dessiné, il **file vers cet ancêtre** : il rétrécit jusqu'à un point au centre du nouveau rectangle de l'ancêtre, en s'effaçant. C'est le cas d'un élément envoyé à la Corbeille, qui vole dans la Corbeille, ou d'un dossier qui se referme ;
  - sinon (supprimé définitivement, ou sorti de la vue), il **se dissout sur place** : il rétrécit vers son propre centre en s'effaçant.

### 3.2 Zoom (« caméra »)

- **Durée et courbe** : 340 ms, *ease-in-out* cubique.
- **Entrer** dans le dossier F :
  - le nouveau treemap part du rectangle que F occupait à l'écran et s'agrandit jusqu'à remplir la zone ;
  - l'ancien treemap s'agrandit avec lui en s'effaçant.
  - Si F n'était pas dessiné, on prend son ancêtre dessiné le plus profond.
- **Remonter**, ou aller vers un ancêtre par le fil d'Ariane :
  - l'ancien treemap rétrécit jusqu'au rectangle de l'ancien dossier dans le nouveau treemap (ou celui de son ancêtre dessiné le plus profond), en s'effaçant ;
  - le nouveau treemap arrive « depuis l'intérieur » de ce rectangle.
- **Autre changement de racine**, ou nouvelle session : fondu enchaîné de 200 ms.

### 3.3 Règles communes

- Le **redimensionnement** de la fenêtre n'est pas animé : l'affichage passe directement à la nouvelle disposition.
- Une transition **interrompue** repart de ce qui est affiché, sans saut.
- Le **test de survol et de clic** utilise toujours la disposition finale. Un clic n'attend jamais la fin d'une animation.
- Au-delà de **6 000 blocs** (blocs plus fantômes), on n'anime pas, pour garder la fluidité.
- Les libellés suivent le rectangle affiché et l'opacité du bloc.
- **Survol** : l'éclaircissement apparaît et disparaît en fondu sur 120 ms.
- **Sélection** : le contour blanc glisse de l'ancien bloc sélectionné vers le nouveau en 160 ms. Il suit aussi le bloc pendant un reflux.

## 4. Autres animations

- **Bulle « libéré »** :
  - Quand l'espace libéré de la session augmente (suppression, vidage de la corbeille, rescan), une pastille « ✓ 12,3 Go libérés » apparaît en bas au centre du treemap. Elle monte de 12 px en apparaissant, sur 200 ms.
  - Elle affiche **ce qui vient d'être libéré**, et le nombre compte à partir de 0 sur 700 ms (*ease-out*).
  - Elle reste visible 3,5 s, puis disparaît en fondu sur 400 ms.
  - Si on libère encore de l'espace pendant qu'elle est affichée, le total visé augmente, le décompte reprend depuis la valeur affichée et le minuteur repart.
- **Ligne d'activité** : une ligne de 2 px sous la barre du haut.
  - Pendant un scan, un segment lumineux (30 % de la largeur, couleur d'accent) la balaie de gauche à droite toutes les 1,2 s.
  - Pendant une suppression, elle se remplit selon la progression (faits / total), en douceur.
  - Elle s'efface en 300 ms quand l'activité finit.
- **Écran de démarrage** :
  - les barres d'occupation des disques se remplissent de 0 à leur valeur en 600 ms (*ease-out*), avec un décalage de 60 ms d'une carte à la suivante ;
  - la bordure des cartes passe en fondu au survol, sur 120 ms.
- **Menus et fenêtres** : egui les fait déjà apparaître en fondu, on garde ce comportement. Le délai anti-double-clic des fenêtres ne change pas.
- **Animations désactivées** : si Windows a « Afficher les animations dans Windows » désactivé (`SPI_GETCLIENTAREAANIMATION` = FALSE), toutes les durées passent à 0. La pastille « libéré » s'affiche alors sans mouvement, avec la valeur finale.

## 5. Contraintes

- Aucune nouvelle dépendance. Toujours egui/eframe 0.37 et Rust 1.95.
- Quand rien n'est animé, aucun redessin supplémentaire.
- Pendant une animation, on demande un redessin à chaque image.
- Les règles de suppression, `build_items` et `next_step` gardent leurs garanties. Une suppression ne part toujours que d'un bouton de confirmation armé.

## 6. Laissé au constructeur

- La structure exacte des données d'animation.
- Le dessin précis de la pastille et de la ligne d'activité, dans le style existant (`theme.rs`).
- Les textes anglais, construits sur le modèle des textes existants.
