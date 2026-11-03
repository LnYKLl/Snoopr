# Clic droit, animations et treemap fluide : plan d'implémentation

> **Pour les agents qui exécutent ce plan :** sous-compétence REQUISE : superpowers:subagent-driven-development (recommandée) ou superpowers:executing-plans, pour exécuter ce plan tâche par tâche. Les étapes sont des cases à cocher (`- [ ]`).

**But :** supprimer un élément en deux clics (clic droit, puis confirmation), animer chaque changement du treemap, et ajouter quelques micro-animations (pastille « libéré », ligne d'activité, cartes de démarrage).

**Architecture :**
- La suppression d'un seul élément réutilise toute la chaîne existante (`build_items`, `next_step`, `start_delete`). On lui passe un `Subject` : soit le panier, soit un panier virtuel qui ne contient que cet élément.
- Les animations sont des fonctions pures sur `snoopr_core::layout::Rect` et `Instant`, testées sans egui (`crate::anim`, `treemap::anim`). La peinture se contente de les consulter.
- Le test de survol et de clic utilise toujours la disposition finale.

**Pile technique :** Rust 1.95 (édition 2024), eframe et egui 0.37, windows 0.62. Aucune nouvelle dépendance.

**Spec :** `docs/superpowers/specs/2026-11-03-snoopr-animations-design.md` (elle complète `docs/superpowers/specs/2026-11-03-snoopr-design.md`).

## Contraintes globales

- **Versions et dépendances :** Rust 1.95, édition 2024, egui et eframe 0.37. Aucune nouvelle crate. Seules les fonctionnalités de `windows` déjà activées sont autorisées : `SystemParametersInfoW` est dans `Win32_UI_WindowsAndMessaging`, déjà active.
- **Après chaque tâche**, ces trois commandes doivent passer :
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- **Interdits :**
  - lancer les tests `#[ignore]` ;
  - mettre quoi que ce soit à la vraie corbeille, ou la vider ;
  - scripter des clics sur de vraies données ;
  - pousser (`git push`).
- **Textes :**
  - toute chaîne affichée existe en FR et en EN dans `i18n.rs` ;
  - en français, U+00A0 se place avant `?` `!` `:` `;` et à l'intérieur de « » ;
  - les commentaires du code sont en anglais, comme l'existant.
- **Données de test :** les nouveaux tests n'utilisent jamais le prénom de l'utilisateur. Prendre `alice` (le dépôt est public).
- **Commits :** messages en français (`feat(app): …`), avec les deux lignes de fin :
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01GbosYoLSCch5Xt3R8JkpQ4
  ```
- **Garde-fous :** une suppression ne part toujours que d'un bouton de confirmation armé. La session revérifie chaque élément avant le disque.
- **Durées d'animation (spec §3-4) :**

  | Animation | Durée et courbe |
  |---|---|
  | Reflux | 260 ms, *ease-out* cubique |
  | Zoom | 340 ms, *ease-in-out* cubique |
  | Fondu (apparition, autre racine) | 200 ms |
  | Survol | 120 ms |
  | Glissement de la sélection | 160 ms |
  | Pastille « libéré » | apparition 200 ms (12 px), décompte 700 ms, maintien 3,5 s, fondu 400 ms |
  | Ligne d'activité | balayage 1,2 s, segment de 30 %, effacement 300 ms |
  | Cartes de démarrage | 600 ms, décalage de 60 ms par carte |

  Autres valeurs : un bloc qui apparaît part de 85 % de sa taille, et on n'anime plus au-delà de 6 000 blocs.
- **Animations désactivées dans Windows :** toutes les durées valent 0.

## Points d'attention pour la relecture

1. **Clic droit pendant un scan.** Le menu s'ouvre, « Supprimer… » est grisé avec « Disponible à la fin du scan », et rien n'est supprimé. Test : `menu_model_busy` (Tâche 2).
2. **« Supprimer… » sur un élément qui est aussi dans le panier, avec d'autres éléments.** Seul cet élément est supprimé, et le panier garde les autres. Test : `checked_requests_one_keeps_only_the_subject` (Tâche 1).
3. **Cible du menu détachée de l'arbre**, par exemple après un rescan pendant que le menu est ouvert. Tout est grisé et aucune fenêtre ne s'ouvre. Test : `menu_model_detached_target` (Tâche 2).
4. **Redimensionnement en pleine transition.** On saute à la nouvelle disposition et aucun fantôme ne reste à l'écran. Test : `snap_drops_everything` (Tâche 3).
5. **Double-clic pendant un zoom en cours.** La nouvelle transition repart de ce qui est affiché, sans saut. Test : `interrupted_transition_is_continuous` (Tâche 3).

---

### Tâche 1 : Supprimer un seul élément (`Subject`)

**Fichiers :**
- Modifier `crates/snoopr-app/src/session.rs` (`build_items` et `start_delete`, l. 86-330).
- Modifier `crates/snoopr-app/src/ui/modals.rs` (`Modal`, `Step`, `next_step`, `confirm_delete`, tests).
- Modifier `crates/snoopr-app/src/app.rs` (`request_delete`, `apply_modal`, `apply_top`).
- Modifier `crates/snoopr-app/src/i18n.rs` (ajouter `confirm_one_title`).

**Interfaces :**
- Produit, dans `session.rs` :
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Subject { Basket, One(NodeId) }
  /// The basket a deletion of `subject` is checked against: the real one, or one holding only the item.
  pub fn subject_basket<'a>(tree: &Tree, basket: &'a Basket, subject: Subject) -> Cow<'a, Basket>;
  impl Session {
      pub fn subject_ids(&self, subject: Subject) -> Vec<NodeId>; // Basket → basket items, One(id) → [id]
      pub fn delete_items(&self, subject: Subject, ids: &[NodeId]) -> Vec<DeleteItem>;
      fn checked_requests(&self, subject: Subject, requests: Vec<DeleteRequest>) -> Vec<DeleteRequest>;
      pub fn start_delete(&mut self, subject: Subject, requests: Vec<DeleteRequest>);
  }
  ```
- Produit, dans `modals.rs` : `Modal::ConfirmDelete(Subject)`, `Modal::TooBig { subject: Subject, fit: Vec<NodeId>, too_big: Vec<NodeId> }` et `Step::Delete(Subject, Plan)`.
- Produit, dans `i18n.rs` : `pub fn confirm_one_title(l: Lang, name: &str) -> String`.
- Produit, dans `app.rs` : `fn request_delete(&mut self, ctx: &egui::Context, subject: Subject)`.

**Étapes :**

- [ ] **Étape 1 : écrire les tests qui échouent**, dans `session.rs`, `mod tests`. L'arbre de test `C:\` contient :
  - `Data\movie.mkv` (100), `Data\b.zip` (50) ;
  - `Windows\w.dll` (10) ;
  - `$Recycle.Bin\old.zip` (5) ;
  - plus `set_unscanned(7)`.

  Les tests construisent la session avec `Session::from_tree(t)`.
  ```rust
  #[test]
  fn delete_items_for_one_checks_like_a_one_item_basket() {
      // ids: movie, windows (Dir), root, bin, old (in bin), unscanned
      assert_eq!(ids(s.delete_items(Subject::One(movie), &[movie])), vec![movie]);
      for id in [windows, root, bin, old, unscanned] {
          assert!(s.delete_items(Subject::One(id), &[id]).is_empty(), "{id:?}");
      }
      // An id other than the subject is never kept.
      assert!(s.delete_items(Subject::One(movie), &[b]).is_empty());
      assert!(s.basket.is_empty());
  }
  #[test]
  fn checked_requests_one_keeps_only_the_subject() {
      // basket = [movie, b]; requests for movie and b with Subject::One(movie)
      let out = s.checked_requests(Subject::One(movie), requests_for(&[movie, b]));
      assert_eq!(request_ids(&out), vec![movie]);
      assert_eq!(s.basket.items(), &[movie, b]);
      // Subject::Basket keeps both.
      assert_eq!(request_ids(&s.checked_requests(Subject::Basket, requests_for(&[movie, b]))), vec![movie, b]);
  }
  #[test]
  fn subject_ids_lists_what_the_dialog_shows() {
      assert_eq!(s.subject_ids(Subject::One(b)), vec![b]);
      assert_eq!(s.subject_ids(Subject::Basket), s.basket.items().to_vec());
  }
  ```
  - Dans `modals.rs`, adapter les tests existants à `Modal::ConfirmDelete(Subject::Basket)` et `Step::Delete(Subject::Basket, …)`, puis ajouter :
    ```rust
    #[test]
    fn the_subject_follows_the_dialogs() {
        let one = Subject::One(A);
        assert_eq!(step(&Modal::ConfirmDelete(one), ModalAction::Recycle, RecycleCapacity::MaxBytes(100)),
                   Step::Open(Modal::TooBig { subject: one, fit: vec![A], too_big: vec![B] }));
        let m = Modal::TooBig { subject: one, fit: vec![A], too_big: vec![B] };
        assert_eq!(step(&m, ModalAction::TooBigSkip, RecycleCapacity::Unknown),
                   Step::Delete(one, vec![(RecycleBin, vec![A])]));
    }
    ```
  - Dans `i18n.rs` :
    - `confirm_one_title(Lang::Fr, "Videos") == "Supprimer «\u{a0}Videos\u{a0}»\u{a0}?"` ;
    - `confirm_one_title(Lang::En, "Videos") == "Delete “Videos”?"`.
- [ ] **Étape 2 :** lancer `cargo test -p snoopr-app`. Attendu : ÉCHEC, car `Subject` et `confirm_one_title` ne sont pas définis.
- [ ] **Étape 3 :** implémenter.
  - **`subject_basket`** :
    - `Basket` → `Cow::Borrowed(basket)` ;
    - `One(id)` → un `Basket::default()` auquel on fait `add(tree, id)`.
  - **`build_items`** ne change pas : il reçoit le panier de `subject_basket`.
  - **`start_delete`** appelle `checked_requests` : on y déplace le filtrage qui existe aujourd'hui, avec le panier du sujet.
  - **`confirm_delete(ui, s, subject, rules, lang)`** :
    - la liste vient de `s.delete_items(subject, &s.subject_ids(subject))` ;
    - pour `One` avec 1 élément, le titre est `confirm_one_title(lang, &node_label(tree, id, lang))`, sinon `confirm_title`.
  - **`request_delete(ctx, subject)`** ouvre la fenêtre si `can_start_delete(s.is_busy(), s.delete_items(subject, &s.subject_ids(subject)).len())`. `TopBarAction::RequestDelete` appelle `request_delete(ctx, Subject::Basket)`.
  - **`apply_modal`** lit le sujet dans la fenêtre ouverte (`ConfirmDelete(sub)` ou `TooBig { subject, .. }` ; `Basket` sinon) et l'utilise à la fois pour `items` et pour `start_delete`.
- [ ] **Étape 4 :** lancer `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` et `cargo fmt --all --check`. Attendu : tout passe.
- [ ] **Étape 5 :** commit `feat(app): suppression d'un seul élément sans passer par le panier`.

### Tâche 2 : Menu contextuel, touche Suppr et indice dans la bulle

**Fichiers :**
- Créer `crates/snoopr-app/src/ui/context_menu.rs`.
- Modifier :
  - `crates/snoopr-app/src/ui/mod.rs` ;
  - `crates/snoopr-app/src/ui/widgets.rs` ;
  - `crates/snoopr-app/src/treemap/paint.rs` (`TreemapEvents`, `show`, `tooltip`) ;
  - `crates/snoopr-app/src/app.rs` ;
  - `crates/snoopr-app/src/session.rs` (ajouter `activity`) ;
  - `crates/snoopr-app/src/i18n.rs` ;
  - `README.md`, sections « Running » et « En français ».

**Interfaces :**
- Consomme, de la Tâche 1 : `Subject` et `request_delete(ctx, Subject)`.
- Produit, dans `session.rs` :
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub enum Activity { Idle, Scanning, Deleting { done: usize, total: usize }, EmptyingBin }
  impl Session { pub fn activity(&self) -> Activity; }
  ```
  `Scanning` couvre le scan et les rescans, en cours ou en attente. `Deleting` lit `DeleteJob.progress`. La Tâche 5 consomme `activity`.
- Produit, dans `context_menu.rs` :
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum MenuAction { None, Zoom(NodeId), Reveal(NodeId), AddToBasket(NodeId), RemoveFromBasket(NodeId), Delete(NodeId), EmptyBin }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum Why { Blocked(Block), CoveredBy(NodeId), InRecycleBin, Scanning, Deleting }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum DeleteRow { Delete, EmptyBin, Disabled(Option<Why>) }
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct MenuModel { pub target: NodeId, pub can_zoom: bool, pub reveal: bool, pub in_basket: Option<bool>, pub delete: DeleteRow }
  pub fn menu_model(tree: &Tree, basket: &Basket, rules: &RuleContext, view_root: NodeId, target: NodeId, activity: Activity) -> MenuModel;
  pub fn show(ui: &mut Ui, s: &Session, rules: &RuleContext, lang: Lang, target: NodeId) -> MenuAction;
  ```
- Produit, dans `widgets.rs` :
  - `pub fn danger_row(ui: &mut Ui, icon: Icon, label: &str, enabled: bool) -> Response` : texte `LOCK_TEXT` quand le bouton est actif. Inactif, le texte est `TEXT_MUTED` à moitié transparent, sans survol ni curseur main.
  - `pub fn menu_note(ui: &mut Ui, text: &str)` : texte gris de 11 px, retour à la ligne, retrait de 10 px.
- Produit, dans `paint.rs` : `TreemapEvents` gagne `pub secondary_clicked: Option<NodeId>` (une zone vide donne `Some(view root)`) et `pub response: egui::Response`.
- Produit, dans `app.rs` :
  - le champ `pub menu_target: Option<NodeId>` ;
  - `KeyAction::DeleteSelected`, qui remplace `TogglePrimary` ;
  - `pub fn delete_key_target(tree: &Tree, view_root: NodeId, selected: NodeId) -> Option<DeleteKey>` ;
  - `pub enum DeleteKey { One(NodeId), EmptyBin }`.
- Produit, dans `i18n.rs` :
  - `pub fn hint_line(l: Lang, is_dir: bool, deletable: bool) -> Option<&'static str>` ;
  - `pub fn covered_note(l: Lang, parent: &str) -> String` ;
  - les champs `Strings` : `menu_enter`, `menu_reveal`, `menu_delete`, `menu_empty_bin`, `in_bin_note`, `busy_scan`, `busy_delete`.
  - Textes FR :

    | Champ | Texte |
    |---|---|
    | `menu_enter` | « Entrer dans le dossier » |
    | `menu_reveal` | « Afficher dans l'Explorateur » |
    | `menu_delete` | « Supprimer… » |
    | `menu_empty_bin` | « Vider la corbeille… » |
    | `in_bin_note` | « Déjà dans la corbeille » |
    | `busy_scan` | « Disponible à la fin du scan » |
    | `busy_delete` | « Disponible à la fin de la suppression » |
    | `covered_note` | « Déjà dans le panier avec «\u{a0}X\u{a0}» » |
  - Textes EN :

    | Champ | Texte |
    |---|---|
    | `menu_enter` | “Open folder” |
    | `menu_reveal` | “Show in File Explorer” |
    | `menu_delete` | “Delete…” |
    | `menu_empty_bin` | “Empty Recycle Bin…” |
    | `in_bin_note` | “Already in the Recycle Bin” |
    | `busy_scan` | “Available once the scan ends” |
    | `busy_delete` | “Available once the deletion ends” |
    | `covered_note` | “Already in the basket with “X”” |
  - Indices (`hint_line`) :

    | Cas | FR | EN |
    |---|---|---|
    | Dossier supprimable | « Double-clic pour entrer · clic droit pour supprimer » | “Double-click to open · right-click to delete” |
    | Dossier non supprimable | « Double-clic pour entrer » | “Double-click to open” |
    | Fichier supprimable | « Clic droit pour supprimer » | “Right-click to delete” |
    | Fichier non supprimable | `None` | `None` |
  - Le champ `double_click` disparaît.

**Étapes :**

- [ ] **Étape 1 : écrire les tests qui échouent.**
  - **`context_menu.rs`.** L'arbre de la Tâche 1 sert de base, avec `Data` comme dossier. L'activité vaut `Activity::Idle` sauf mention contraire.
    ```rust
    #[test] fn menu_model_file_and_folder() {
        let m = menu_model(&t, &Basket::default(), &rules, root, movie, Idle);
        assert_eq!(m, MenuModel { target: movie, can_zoom: false, reveal: true, in_basket: Some(false), delete: DeleteRow::Delete });
        let m = menu_model(&t, &Basket::default(), &rules, root, data, Idle);
        assert!(m.can_zoom && m.delete == DeleteRow::Delete);
        // The folder shown is not offered again.
        assert!(!menu_model(&t, &Basket::default(), &rules, data, data, Idle).can_zoom);
    }
    #[test] fn menu_model_reasons() {
        assert_eq!(menu_model(.., windows, Idle).delete, DeleteRow::Disabled(Some(Why::Blocked(Block::WindowsDir))));
        assert_eq!(menu_model(.., bin, Idle).delete, DeleteRow::EmptyBin);
        assert_eq!(menu_model(.., old, Idle).delete, DeleteRow::Disabled(Some(Why::InRecycleBin)));
        assert_eq!(menu_model(.., root, Idle).delete, DeleteRow::Disabled(None));
        assert_eq!(menu_model(.., unscanned, Idle).delete, DeleteRow::Disabled(None));
        // basket = [data]
        let m = menu_model(&t, &basket_with_data, .., movie, Idle);
        assert_eq!((m.in_basket, m.delete), (None, DeleteRow::Disabled(Some(Why::CoveredBy(data)))));
        assert_eq!(menu_model(&t, &basket_with_data, .., data, Idle).in_basket, Some(true));
    }
    #[test] fn menu_model_busy() {
        assert_eq!(menu_model(.., movie, Activity::Scanning).delete, DeleteRow::Disabled(Some(Why::Scanning)));
        assert_eq!(menu_model(.., bin, Activity::Deleting { done: 0, total: 1 }).delete, DeleteRow::Disabled(Some(Why::Deleting)));
        assert_eq!(menu_model(.., movie, Activity::EmptyingBin).delete, DeleteRow::Disabled(Some(Why::Deleting)));
    }
    #[test] fn menu_model_detached_target() {
        // movie detached (via the tree API used by apply_outcomes)
        let m = menu_model(.., movie, Idle);
        assert_eq!(m, MenuModel { target: movie, can_zoom: false, reveal: false, in_basket: None, delete: DeleteRow::Disabled(None) });
    }
    ```
    Pour `Block::WindowsDir`, prendre la variante que `classify` renvoie vraiment pour `C:\Windows` (voir `rules.rs`).
  - **`app.rs`** :
    ```rust
    #[test] fn delete_key_targets() {
        assert_eq!(delete_key_target(&t, root, movie), Some(DeleteKey::One(movie)));
        assert_eq!(delete_key_target(&t, root, bin), Some(DeleteKey::EmptyBin));
        assert_eq!(delete_key_target(&t, data, data), None); // the folder shown itself
        assert_eq!(delete_key_target(&t, root, root), None);
    }
    ```
    Mettre à jour `keys()` : `key_action(Key::Delete, false) == Some(KeyAction::DeleteSelected)`.
  - **`i18n.rs`** : les 4 cas de `hint_line` en FR et en EN, et `covered_note` en FR et en EN.
  - **`session.rs`** : `activity()` vaut `Idle` sur une session `from_tree`, et `Deleting { done: 0, total: 2 }` après avoir posé un `DeleteJob` à la main. Le test réutilise le constructeur `DeleteJob` du module de tests ; s'il n'y en a pas, en ajouter un sous `#[cfg(test)]`.
- [ ] **Étape 2 :** lancer `cargo test -p snoopr-app`. Attendu : ÉCHEC, car les fonctions n'existent pas encore.
- [ ] **Étape 3 :** implémenter.
  - **`menu_model`** :
    - sur une cible non attachée (`!tree.is_attached(target)`), tout est faux, `None` ou `Disabled(None)` ;
    - sinon, partir de `primary_action` :
      - Add ou Remove donne `Delete` et `in_basket` = Some(false) ou Some(true) ;
      - EmptyRecycleBin donne `EmptyBin` ;
      - Disabled(Blocked, CoveredBy ou InRecycleBin) donne `Disabled(Some(Blocked, CoveredBy ou InRecycleBin))` ;
      - Disabled(Root ou Unscanned) donne `Disabled(None)` ;
    - ensuite, une activité autre qu'`Idle` change un `Delete` ou un `EmptyBin` en `Disabled(Some(Scanning))` pour `Scanning`, ou en `Disabled(Some(Deleting))` pour les autres activités ;
    - `can_zoom` = `kind == Dir && target != view_root` ;
    - `reveal` = attaché et pas `Unscanned`.
  - **`show`** dessine le modèle de haut en bas, comme décrit en spec §2.2 :
    - `menu_heading` avec `node_label` · `format_size` ;
    - les lignes `menu_row` ;
    - `menu_divider` ;
    - la ligne du panier ;
    - `danger_row` avec `Icon::Trash` ;
    - `menu_note` pour la raison : 🔒 + `block_message` ; `covered_note(node_label(c))` ; `in_bin_note` ; `busy_*`.

    Un clic renvoie l'action et ferme le menu (`ui.close()`).
  - **`paint.rs`** : un `resp.secondary_clicked()` donne `secondary_clicked` = la cible sous le pointeur, ou la racine affichée.
    - Dans la bulle, la dernière ligne devient `hint_line(lang, is_dir, deletable)`.
    - `deletable` = `primary_action(...)` vaut Add ou Remove. Le calcul se fait dans le bloc de lecture qui calcule déjà `guard`, pour le seul bloc survolé.
    - Pas d'indice pour un groupe de petits fichiers ni pour « Non analysé ».
  - **`app.rs`** :
    - un `secondary_clicked = Some(id)` sélectionne `id` et fixe `menu_target = Some(id)` ;
    - ensuite, si aucune fenêtre n'est ouverte et que `menu_target` vaut `Some(t)`, le menu s'affiche avec `egui::Popup::context_menu(&ev.response).show(|ui| context_menu::show(ui, s, &self.rules, self.lang, t))` ;
    - si le résultat est `None`, le menu est fermé et `menu_target` repasse à `None` ;
    - les actions :
      - `Zoom` donne `s.zoom` ;
      - `Reveal` ouvre l'Explorateur sur l'élément sélectionné. Factoriser `open_selected` en `reveal(id)` ;
      - `Add` et `Remove` passent par le panier ;
      - `Delete(id)` appelle `request_delete(ctx, Subject::One(id))` ;
      - `EmptyBin` appelle `request_empty_bin`.
    - Suppr suit `delete_key_target` : `One` appelle `request_delete(ctx, Subject::One(id))`, et `EmptyBin` appelle `request_empty_bin`.
  - **`README.md`**, dans « Running » et « En français » : une phrase sur le clic droit (« Clic droit sur un bloc → Supprimer… ») et sur la touche Suppr.
- [ ] **Étape 4 :** lancer `cargo test --workspace`, clippy et fmt. Attendu : tout passe.
- [ ] **Étape 5 :** commit `feat(app): menu clic droit pour supprimer, touche Suppr comme dans l'Explorateur`.

### Tâche 3 : Moteur d'animation (pur)

**Fichiers :**
- Créer `crates/snoopr-app/src/anim.rs`, qui contient les courbes, la progression et le réglage de Windows.
- Créer `crates/snoopr-app/src/treemap/anim.rs`, qui contient les transitions du treemap.
- Modifier `crates/snoopr-app/src/system.rs` (ajouter `animations_enabled`), `crates/snoopr-app/src/main.rs` (`mod anim;`) et `crates/snoopr-app/src/treemap/mod.rs`.

**Interfaces :**
- Produit, dans `crate::anim` :
  ```rust
  pub fn ease_out_cubic(t: f32) -> f32;      // t clamped to [0, 1]
  pub fn ease_in_out_cubic(t: f32) -> f32;
  pub fn progress(start: Instant, now: Instant, d: Duration) -> f32; // 0..=1; d == 0 → 1
  pub fn motion_enabled() -> bool;           // system::animations_enabled(), read once (OnceLock)
  ```
- Produit, dans `system.rs` : `pub fn animations_enabled() -> bool`. Il appelle `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, &mut BOOL, 0)` et renvoie `true` en cas d'échec ou hors Windows.
- Produit, dans `treemap::anim`. Ici `R` désigne `snoopr_core::layout::Rect`.
  ```rust
  pub const REFLOW: Duration;  // 260 ms
  pub const ZOOM: Duration;    // 340 ms
  pub const APPEAR: Duration;  // 200 ms
  pub const SPAWN_SCALE: f32 = 0.85;
  pub const MAX_ANIMATED: usize = 6000;
  pub fn lerp_rect(a: R, b: R, t: f32) -> R;
  pub fn map_rect(r: R, from: R, to: R) -> R;   // same relative place of `r` in `from`, inside `to`
  pub fn scale_about_center(r: R, k: f32) -> R; // k = 0 → zero-size rect at the centre
  #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
  pub enum AnimKey { Node(NodeId), Group(NodeId) } // Group = small-files group of that parent
  impl AnimKey { pub fn of(item: &Item) -> AnimKey; }
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub enum Change { Snap, Appear, Reflow, ZoomIn { focus_old: R }, ZoomOut { focus_new: R } }
  pub fn classify_change(tree: &Tree, old: Option<&CachedLayout>, new: &CachedLayout, area: R) -> Change;
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct Shown { pub rect: R, pub alpha: f32 }
  pub struct Animator { /* the running transition, if any */ }
  impl Animator {
      pub fn new(enabled: bool) -> Self;
      pub fn reset(&mut self);
      /// Starts the transition from what is on screen at `now` to `new`. Call before `old` is replaced.
      pub fn on_rebuild(&mut self, tree: &Tree, old: Option<&CachedLayout>, new: &CachedLayout, area: R, now: Instant);
      pub fn shown(&self, key: AnimKey, target: R, now: Instant) -> Shown;
      pub fn ghosts(&self, now: Instant) -> Vec<(&CachedItem, Shown)>;
      pub fn is_animating(&self, now: Instant) -> bool;
  }
  ```
- **Règles de `classify_change`** :

  | Situation | Résultat |
  |---|---|
  | `old` vaut `None` | `Appear` |
  | taille différente | `Snap` |
  | même racine | `Reflow` |
  | l'ancienne racine est un ancêtre de la nouvelle | `ZoomIn`, avec `focus_old` = rectangle, dans `old`, de la nouvelle racine ou de son ancêtre placé le plus profond ; `area` si aucun |
  | la nouvelle racine est un ancêtre de l'ancienne | `ZoomOut`, avec `focus_new` = rectangle, dans `new`, de l'ancienne racine ou de son ancêtre placé le plus profond ; `area` si aucun |
  | autre cas | `Appear` |

  La racine affichée n'est jamais elle-même un élément placé.
- **Règles de `on_rebuild`.** Le « départ » d'un bloc, c'est ce qui est affiché à `now` : `shown(key, rect dans old, now)`.
  - **Pas de transition**, et les fantômes sont effacés, quand :
    - `!enabled` ;
    - ou `Snap` ;
    - ou `new.items.len()` + le nombre de fantômes dépasse `MAX_ANIMATED`.
  - **`Reflow`** (`REFLOW`, `ease_out_cubic`) :
    - une clé présente avant et après part de son départ ;
    - une nouvelle clé part de `Shown { scale_about_center(cible, SPAWN_SCALE), 0.0 }` ;
    - une clé qui disparaît devient un fantôme qui part de son départ :
      - pour `Node(id)` attaché et dont un ancêtre strict `a` est placé dans `new`, le fantôme va vers `Shown { scale_about_center(rect_new(a), 0.0), 0.0 }` ;
      - sinon (y compris `Group`), il va vers `Shown { scale_about_center(départ, 0.0), 0.0 }`.
  - **`ZoomIn { focus_old: f }`** (`ZOOM`, `ease_in_out_cubic`) :
    - chaque nouvelle clé part de `Shown { map_rect(cible, area, f), 0.0 }` ;
    - chaque ancien bloc devient un fantôme qui va de son départ vers `Shown { map_rect(départ, f, area), 0.0 }`.
  - **`ZoomOut { focus_new: f }`** (`ZOOM`, `ease_in_out_cubic`) :
    - chaque nouvelle clé part de `Shown { map_rect(cible, f, area), 0.0 }` ;
    - chaque ancien bloc devient un fantôme qui va de son départ vers `Shown { map_rect(départ, area, f), 0.0 }`.
  - **`Appear`** (`APPEAR`, `ease_out_cubic`) :
    - chaque nouvelle clé part de `Shown { cible, 0.0 }` ;
    - les anciens blocs deviennent des fantômes qui vont de leur départ vers `Shown { départ, 0.0 }`.
  - **Fantômes d'une transition interrompue** : ceux qui sont encore visibles sont repris. Ils partent de leur position affichée et gardent leur destination.
  - **Fin de transition** : une fois la transition finie, `shown` renvoie `Shown { cible, 1.0 }`, et `ghosts` renvoie une liste vide.

**Étapes :**

- [ ] **Étape 1 : écrire les tests qui échouent.**
  - **`anim.rs`** :
    - `ease_out_cubic` : 0 → 0, 1 → 1, 0,5 → 0,875 ; -1 → 0 et 2 → 1 (bornes) ;
    - `ease_in_out_cubic` : 0,5 → 0,5 et 0,25 → 0,0625 ;
    - `progress` : la moitié de la durée donne 0,5, et une durée nulle donne 1,0.
  - **`treemap/anim.rs`** :
    ```rust
    #[test] fn rect_math() {
        let r = R { x: 50., y: 50., w: 50., h: 50. };
        assert_eq!(map_rect(r, R { x: 0., y: 0., w: 100., h: 100. }, R { x: 10., y: 20., w: 50., h: 50. }), R { x: 35., y: 45., w: 25., h: 25. });
        assert_eq!(lerp_rect(R { x: 0., y: 0., w: 10., h: 10. }, R { x: 10., y: 20., w: 30., h: 10. }, 0.5), R { x: 5., y: 10., w: 20., h: 10. });
        assert_eq!(scale_about_center(R { x: 0., y: 0., w: 10., h: 20. }, 0.0), R { x: 5., y: 10., w: 0., h: 0. });
    }
    #[test] fn classify_change_table() { /* Appear, Snap, Reflow, ZoomIn (placed / deepest placed ancestor), ZoomOut, sibling → Appear */ }
    #[test] fn reflow_moves_kept_blocks_and_spawns_new_ones() {
        // shown(key, new_rect, t0) == old rect; at t0 + REFLOW == new rect, alpha 1; halfway strictly between
        // new key at t0: rect == scale_about_center(target, 0.85), alpha 0
    }
    #[test] fn deleted_blocks_dissolve_and_recycled_ones_fly_into_the_bin() {
        // detached node: ghost ends at the centre of its own rect, alpha 0
        // node moved under a placed $Recycle.Bin: ghost ends at the centre of the bin's new rect
    }
    #[test] fn zoom_in_grows_from_the_folder() {
        // at t0 an incoming block is map_rect(target, area, focus_old); at t0 + ZOOM it is the target
    }
    #[test] fn interrupted_transition_is_continuous() {
        // rebuild at t0 + 100 ms: shown() of a kept key just after the rebuild == just before
    }
    #[test] fn snap_drops_everything() {
        // a running reflow, then a rebuild with another size: is_animating false, no ghost, shown == target
    }
    #[test] fn disabled_or_huge_layouts_do_not_animate() {
        // Animator::new(false); and a layout of MAX_ANIMATED + 1 items
    }
    ```
    Les tests construisent leurs `CachedLayout` à la main, avec une fonction utilitaire qui fabrique un `CachedItem` à partir de (`Item`, `Rect`). Les arbres sont de vrais `Tree`, comme dans `model.rs`.
- [ ] **Étape 2 :** lancer `cargo test -p snoopr-app anim`. Attendu : ÉCHEC, car les fonctions n'existent pas.
- [ ] **Étape 3 :** implémenter, en suivant les règles ci-dessus.
  - Une `HashMap<AnimKey, Shown>` contient les départs, et un `Vec` contient les fantômes : chaque fantôme garde un clone de son `CachedItem`, son départ et sa destination.
  - Pour les ancêtres placés, construire une `HashMap<NodeId, R>` de `new`.
- [ ] **Étape 4 :** lancer `cargo test --workspace`, clippy et fmt. Attendu : tout passe. En attendant la Tâche 4, `#[allow(dead_code)]` est permis sur les éléments que la peinture n'utilise pas encore, avec le commentaire `// used by the painter (task 4)`.
- [ ] **Étape 5 :** commit `feat(app): moteur d'animation du treemap`.

### Tâche 4 : Brancher le treemap animé, avec le survol et la sélection

**Fichiers :**
- Modifier `crates/snoopr-app/src/treemap/paint.rs`, `crates/snoopr-app/src/treemap/anim.rs` et `crates/snoopr-app/src/app.rs`.

**Interfaces :**
- Consomme, de la Tâche 3 : `Animator`, `AnimKey`, `Shown`, `anim::{progress, ease_out_cubic, motion_enabled}`.
- Produit, dans `treemap::anim` :
  ```rust
  pub const HOVER: Duration;  // 120 ms
  pub const GLIDE: Duration;  // 160 ms
  /// Hover highlight levels: the hovered key rises to 1, the others fall to 0 and are dropped.
  pub struct HoverFade { /* Vec<(AnimKey, f32)> */ }
  impl HoverFade {
      pub fn new(enabled: bool) -> Self;
      pub fn update(&mut self, hovered: Option<AnimKey>, dt: f32);
      pub fn level(&self, key: AnimKey) -> f32;
      pub fn is_animating(&self) -> bool;
  }
  /// The selection outline: glides from the previous selection to the new one.
  pub struct Glide { /* from: Option<R>, key, start */ }
  impl Glide {
      pub fn new(enabled: bool) -> Self;
      pub fn rect(&mut self, key: Option<AnimKey>, target: R, now: Instant) -> R;
      pub fn is_animating(&self, now: Instant) -> bool;
  }
  /// Everything the treemap animates, owned by the app and reset with the session.
  pub struct TreemapAnim { pub layout: Animator, pub hover: HoverFade, pub glide: Glide, pub last_frame: Option<Instant> }
  impl TreemapAnim { pub fn new(enabled: bool) -> Self; pub fn reset(&mut self); }
  ```
- Produit, dans `paint.rs` : `pub fn show(ui, s, cache, anim: &mut TreemapAnim, rules, lang) -> TreemapEvents`.
- Produit, dans `app.rs` : le champ `pub treemap_anim: TreemapAnim`, créé avec `TreemapAnim::new(anim::motion_enabled())` et remis à zéro dans `start_session`.

**Étapes :**

- [ ] **Étape 1 : écrire les tests qui échouent**, dans `treemap/anim.rs`.
  - **`HoverFade`** :
    - sur une clé survolée, après `update` de 0,06 s, le niveau vaut 0,5, puis 1,0 après encore 0,06 s ;
    - une fois le survol parti, le niveau vaut 0,5 après 0,06 s, puis la clé disparaît (`level` = 0) et `is_animating` passe à faux ;
    - avec `enabled = false`, le niveau vaut directement 1 ou 0.
  - **`Glide`** :
    - le premier appel renvoie la cible ;
    - quand la clé change à t0, la valeur renvoyée à t0 est l'ancien rectangle ; à t0 + `GLIDE`, c'est la nouvelle cible ;
    - à clé égale, si la cible bouge (pendant un reflux), il suit la cible sans délai ;
    - avec `enabled = false`, il renvoie toujours la cible.
- [ ] **Étape 2 :** lancer `cargo test -p snoopr-app anim`. Attendu : ÉCHEC.
- [ ] **Étape 3 :** implémenter les deux types, puis brancher `paint.rs`.
  - **Reconstruction :** quand `needs_rebuild` est vrai, construire `new`, appeler `anim.layout.on_rebuild(&tree, cache.as_ref(), &new, area_local, now)`, puis remplacer `cache`. `area_local` vaut `R { x: 0, y: 0, w: size.0, h: size.1 }`.
  - **Peinture :** d'abord les fantômes (`anim.layout.ghosts(now)`), puis les éléments, chacun à `anim.layout.shown(AnimKey::of(&it.placed.item), it.placed.rect, now)`.
    - Si `alpha < 1`, peindre avec un `Painter` cloné, après `multiply_opacity(alpha)`.
    - `paint_dir` et `paint_leaf` reçoivent le rectangle affiché, et le niveau de survol à la place du booléen `hot`. `lit(c, level)` mélange avec `HOVER_MIX * level`.
  - **Contours :** le contour de sélection est peint à `anim.glide.rect(clé de la sélection, rect affiché, now)`. Les contours du panier sont peints aux rectangles affichés.
  - **Survol et clics :** `hit_test` reste sur `it.placed.rect`. `anim.hover.update(hovered_key, dt)` reçoit un `dt` calculé avec `last_frame` et plafonné à 0,1 s.
  - **Redessin :** si `anim.layout.is_animating(now)`, `anim.hover.is_animating()` ou `anim.glide.is_animating(now)`, appeler `ui.ctx().request_repaint()`.
- [ ] **Étape 4 :** lancer `cargo test --workspace`, clippy et fmt. Attendu : tout passe.
  - Puis lancer `cargo build --release -p snoopr-app`, démarrer `target\release\snoopr-app.exe C:\Windows` et le fermer après 5 s. Attendu : pas de panique dans la sortie. Ne cliquer sur rien.
- [ ] **Étape 5 :** commit `feat(app): treemap animé (reflux, zoom, fantômes, survol, sélection)`.

### Tâche 5 : Pastille « libéré », ligne d'activité, cartes de démarrage

**Fichiers :**
- Créer `crates/snoopr-app/src/ui/toast.rs` et `crates/snoopr-app/src/ui/activity.rs`.
- Modifier `crates/snoopr-app/src/ui/mod.rs`, `crates/snoopr-app/src/ui/start.rs` et `crates/snoopr-app/src/app.rs`.

**Interfaces :**
- Consomme, de la Tâche 2 : `Session::activity()` et `Activity`.
- Consomme, de la Tâche 3 : `anim::{progress, ease_out_cubic, motion_enabled}`.
- Produit, dans `toast.rs` :
  ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct ToastView { pub value: u64, pub alpha: f32, pub dy: f32 } // dy: px below the rest position (12 → 0)
  pub struct FreedToast { /* last_freed, target, from_value, count_start, appear, last_bump, enabled */ }
  impl FreedToast {
      pub fn new(freed: u64, enabled: bool) -> Self;
      /// Called every frame with the session's total freed bytes.
      pub fn update(&mut self, freed: u64, now: Instant);
      pub fn view(&self, now: Instant) -> Option<ToastView>;
      pub fn is_animating(&self, now: Instant) -> bool; // true while visible
  }
  pub fn paint(ui: &Ui, area: egui::Rect, v: ToastView, lang: Lang); // pill centred, 16 px above area.bottom()
  ```
  La pastille dit `freed(lang, v.value)`, sur fond `POPUP_BG` et bordure `BORDER`, avec le ✓ en `OK_TEXT`.
- Produit, dans `activity.rs` :
  ```rust
  pub const SWEEP_PERIOD: f32 = 1.2; pub const SEGMENT: f32 = 0.3; pub const FADE: f32 = 0.3;
  /// Indeterminate segment `[x0, x1]` (fractions of the width) at `t` seconds: travels from -SEGMENT to 1.
  pub fn sweep(t: f64) -> (f32, f32);
  /// Moves `current` towards `target` with a time constant of 0.15 s.
  pub fn approach(current: f32, target: f32, dt: f32) -> f32;
  pub struct ActivityLine { /* fade level, shown progress, last frame */ }
  impl ActivityLine {
      pub fn new(enabled: bool) -> Self;
      pub fn paint(&mut self, ui: &Ui, under: egui::Rect, a: Activity, now: Instant) -> bool; // true while animating
  }
  ```
  - `Scanning` et `EmptyingBin` donnent le segment qui balaie.
  - `Deleting { done, total }` donne un remplissage de `done / total`, adouci par `approach`.
  - `Idle` donne un effacement de `FADE` secondes.
  - La ligne est peinte sur les 2 px du bas de la barre du haut, couleur `ACCENT`.
- Produit, dans `start.rs` : `pub fn bar_fraction(frac: f32, index: usize, elapsed: Duration, enabled: bool) -> f32` et `pub fn show(ui: &mut Ui, lang: Lang, volumes: &[Volume], appeared: Instant) -> Option<PathBuf>`.
- Produit, dans `app.rs` : les champs `freed_toast: FreedToast`, `activity: ActivityLine` et `start_appeared: Instant`.
  - `start_session` recrée `freed_toast` avec la valeur `freed` de la nouvelle session.
  - `start_appeared` est fixé à la création de l'app.

**Étapes :**

- [ ] **Étape 1 : écrire les tests qui échouent.**
  - **`toast.rs`.** On note `t0` l'instant de la hausse ; `enabled = true` sauf mention contraire.
    - Sans hausse de `freed`, `view` renvoie `None`.
    - Après `update(1000, t0)`, en partant de 0 :
      - à `t0`, on a `alpha == 0` et `dy == 12` ;
      - à `t0 + 200 ms`, on a `alpha == 1` et `dy == 0` ;
      - à `t0 + 350 ms`, on a `0 < value < 1000` ;
      - à `t0 + 700 ms`, `value == 1000` ;
      - à `t0 + 3,5 s`, `alpha == 1` ;
      - à `t0 + 3,7 s`, `alpha` vaut environ 0,5 ;
      - à `t0 + 3,9 s`, `view` renvoie `None`.
    - Après une deuxième hausse à `t0 + 1 s` (`update(1500, …)`) :
      - la cible vaut 1500, et la valeur à `t0 + 1 s` est 1000 ;
      - la valeur vaut 1500 à `t0 + 1,7 s`, et la pastille est encore visible à `t0 + 4,4 s`.
    - Avec `enabled = false` : à `t0`, `value == 1000`, `alpha == 1` et `dy == 0` ; `view` renvoie `None` après 3,5 s.
    - Une baisse de `freed` (nouvelle session) n'affiche rien.
  - **`activity.rs`** :
    - `sweep(0.0) == (-0.3, 0.0)` et `sweep(1.2) == sweep(0.0)` ;
    - formule : `phase = (t mod 1.2) / 1.2`, `x0 = -0.3 + 1.3 × phase`, `x1 = x0 + 0.3` ;
    - `sweep(0.6)` vaut environ `(0.35, 0.65)` ;
    - `approach(0, 1, 0.15)` vaut environ 0,632, à 1e-3 près ;
    - `approach(x, x, dt) == x`.
  - **`start.rs`** :
    - `bar_fraction(0.8, 0, 0 ms) == 0` et `bar_fraction(0.8, 0, 600 ms) == 0.8` ;
    - `bar_fraction(0.8, 1, 60 ms) == 0` et `bar_fraction(0.8, 1, 660 ms) == 0.8` ;
    - `bar_fraction(0.8, 3, 0 ms, false) == 0.8`.
- [ ] **Étape 2 :** lancer `cargo test -p snoopr-app`. Attendu : ÉCHEC.
- [ ] **Étape 3 :** implémenter, puis brancher.
  - **Dans `app.rs`** :
    - appeler `freed_toast.update(s.freed, now)` à chaque image ;
    - peindre la pastille au-dessus du treemap (`ctx.layer_painter` sur l'ordre `Foreground`), sous les fenêtres ;
    - peindre `activity.paint` juste après la barre du haut ;
    - demander un redessin quand l'un des deux anime.
  - **Dans `start.rs`** :
    - la barre d'occupation utilise `bar_fraction` ;
    - la bordure et le fond des cartes passent en fondu avec `ctx.animate_bool_with_time(resp.id, resp.hovered(), 0.12)` (0 si les animations sont désactivées) ;
    - demander un redessin tant que les barres se remplissent.
- [ ] **Étape 4 :** lancer `cargo test --workspace`, clippy et fmt. Attendu : tout passe. Puis lancer le build release et démarrer l'exe sans argument (écran de démarrage) pendant 5 s. Attendu : pas de panique.
- [ ] **Étape 5 :** commit `feat(app): pastille « libéré », ligne d'activité et cartes animées`.
