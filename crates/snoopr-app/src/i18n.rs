//! All user-visible text and number formats, in French and English.

use snoopr_core::delete::FailureReason;
use snoopr_core::filetype::FileType;
use snoopr_core::rules::{Block, Warning};

/// UI language, following Windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// French when the locale tag starts with `fr`, English otherwise.
    pub fn from_locale(tag: &str) -> Lang {
        if tag.to_ascii_lowercase().starts_with("fr") {
            Lang::Fr
        } else {
            Lang::En
        }
    }

    /// Language of the running Windows session; English if unknown.
    pub fn detect() -> Lang {
        sys_locale::get_locale().map_or(Lang::En, |t| Lang::from_locale(&t))
    }

    pub fn s(self) -> &'static Strings {
        match self {
            Lang::Fr => &FR,
            Lang::En => &EN,
        }
    }
}

pub struct Strings {
    pub choose_folder: &'static str,
    pub rescan: &'static str,
    pub stop: &'static str,
    pub up: &'static str,
    pub age_button_all: &'static str,
    pub age_menu_title: &'static str,
    pub age_all: &'static str,
    pub age_6m: &'static str,
    pub age_1y: &'static str,
    pub age_2y: &'static str,
    pub open: &'static str,
    pub add: &'static str,
    pub remove: &'static str,
    pub empty_bin: &'static str,
    pub blocked: &'static str,
    pub covered: &'static str,
    pub in_bin: &'static str,
    pub basket: &'static str,
    pub basket_title: &'static str,
    pub basket_empty: &'static str,
    pub wait_scan: &'static str,
    pub recycle_bin_name: &'static str,
    pub unscanned_name: &'static str,
    pub unscanned_explain: &'static str,
    pub double_click: &'static str,
    pub bin_info: &'static str,
    pub cancel: &'static str,
    pub delete_permanently: &'static str,
    pub move_to_bin: &'static str,
    pub skip: &'static str,
    pub ok: &'static str,
    pub too_big_title: &'static str,
    pub too_big_body: &'static str,
    pub failures_title: &'static str,
    pub empty_bin_title: &'static str,
    pub emptying_bin: &'static str,
    pub relaunch_admin: &'static str,
    pub start_title: &'static str,
    pub percent_of_disk: &'static str,
}

pub static FR: Strings = Strings {
    choose_folder: "Choisir un dossier…",
    rescan: "Rescanner",
    stop: "Arrêter",
    up: "Remonter (Retour arrière)",
    age_button_all: "Ancienneté : tout",
    age_menu_title: "Griser ce qui a servi récemment",
    age_all: "Tout afficher",
    age_6m: "Pas modifié depuis 6 mois",
    age_1y: "Pas modifié depuis 1 an",
    age_2y: "Pas modifié depuis 2 ans",
    open: "Ouvrir",
    add: "Ajouter au panier",
    remove: "Retirer du panier",
    empty_bin: "Vider la corbeille",
    blocked: "Suppression bloquée",
    covered: "Déjà dans le panier",
    in_bin: "Déjà dans la corbeille",
    basket: "Panier",
    basket_title: "Panier de nettoyage",
    basket_empty: "Le panier est vide. Sélectionne un bloc puis « Ajouter au panier » (ou Suppr). Rien n’est effacé tant que tu n’as pas confirmé.",
    wait_scan: "Attends la fin du scan pour supprimer.",
    recycle_bin_name: "Corbeille",
    unscanned_name: "Non analysé",
    unscanned_explain: "Dossiers inaccessibles et espace réservé par le système (points de restauration…).",
    double_click: "Double-clic pour entrer",
    bin_info: "La corbeille garde les fichiers sur le disque : la place n’est vraiment libérée qu’une fois la corbeille vidée. « Supprimer définitivement » libère la place tout de suite, mais on ne peut plus revenir en arrière.",
    cancel: "Annuler",
    delete_permanently: "Supprimer définitivement",
    move_to_bin: "Mettre à la corbeille",
    skip: "Ignorer",
    ok: "OK",
    too_big_title: "Trop gros pour la corbeille",
    too_big_body: "Ces éléments ne peuvent pas aller à la corbeille. Les supprimer définitivement, ou les ignorer ?",
    failures_title: "Certains éléments n’ont pas pu être supprimés",
    empty_bin_title: "Vider la corbeille ?",
    emptying_bin: "Vidage de la corbeille…",
    relaunch_admin: "Relancer en administrateur",
    start_title: "Quel disque veux-tu analyser ?",
    percent_of_disk: "du disque",
};

pub static EN: Strings = Strings {
    choose_folder: "Choose a folder…",
    rescan: "Rescan",
    stop: "Stop",
    up: "Up (Backspace)",
    age_button_all: "Age: all",
    age_menu_title: "Dim recently used items",
    age_all: "Show all",
    age_6m: "Not modified for 6 months",
    age_1y: "Not modified for 1 year",
    age_2y: "Not modified for 2 years",
    open: "Open",
    add: "Add to basket",
    remove: "Remove from basket",
    empty_bin: "Empty Recycle Bin",
    blocked: "Deletion blocked",
    covered: "Already in basket",
    in_bin: "Already in Recycle Bin",
    basket: "Basket",
    basket_title: "Cleanup basket",
    basket_empty: "The basket is empty. Select a block, then “Add to basket” (or Del). Nothing is deleted until you confirm.",
    wait_scan: "Wait for the scan to finish before deleting.",
    recycle_bin_name: "Recycle Bin",
    unscanned_name: "Not scanned",
    unscanned_explain: "Inaccessible folders and space reserved by the system (restore points…).",
    double_click: "Double-click to open",
    bin_info: "The Recycle Bin keeps files on disk: space is only freed once it is emptied. “Delete permanently” frees space right away, but cannot be undone.",
    cancel: "Cancel",
    delete_permanently: "Delete permanently",
    move_to_bin: "Move to Recycle Bin",
    skip: "Skip",
    ok: "OK",
    too_big_title: "Too big for the Recycle Bin",
    too_big_body: "These items can’t go to the Recycle Bin. Delete them permanently, or skip them?",
    failures_title: "Some items couldn’t be deleted",
    empty_bin_title: "Empty the Recycle Bin?",
    emptying_bin: "Emptying the Recycle Bin…",
    relaunch_admin: "Restart as administrator",
    start_title: "Which drive do you want to scan?",
    percent_of_disk: "of disk",
};

fn is_fr(l: Lang) -> bool {
    l == Lang::Fr
}

/// Picks the French or English variant.
fn pick<T>(l: Lang, fr: T, en: T) -> T {
    if is_fr(l) { fr } else { en }
}

fn group(l: Lang, n: u64) -> String {
    let sep = pick(l, '\u{202f}', ',');
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

fn decimal(l: Lang, v: f64) -> String {
    let s = format!("{v:.1}");
    if is_fr(l) { s.replace('.', ",") } else { s }
}

pub fn format_size(l: Lang, bytes: u64) -> String {
    let units: [&str; 5] = pick(
        l,
        ["o", "Ko", "Mo", "Go", "To"],
        ["B", "KB", "MB", "GB", "TB"],
    );
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    // Rounded for display, 1023.6 Mo would read "1024 Mo": that is 1,0 Go.
    if v.round() >= 1024.0 && u < units.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    let rounded = (v * 10.0).round() / 10.0;
    let num = if u > 0 && rounded < 10.0 {
        decimal(l, rounded)
    } else {
        format!("{}", v.round() as u64)
    };
    let sep = pick(l, '\u{a0}', ' ');
    format!("{num}{sep}{}", units[u])
}

pub fn format_count(l: Lang, n: u64) -> String {
    group(l, n)
}

pub fn format_percent(l: Lang, p: f64) -> String {
    let num = decimal(l, p);
    if is_fr(l) {
        format!("{num}\u{a0}%")
    } else {
        format!("{num}%")
    }
}

pub fn format_age(l: Lang, months: u32) -> String {
    if months == 0 {
        return pick(l, "ce mois-ci", "this month").to_string();
    }
    if months < 12 {
        return if is_fr(l) {
            format!("{months} mois")
        } else if months == 1 {
            "1 month".to_string()
        } else {
            format!("{months} months")
        };
    }
    let years = months / 12;
    let s = if years > 1 { "s" } else { "" };
    if is_fr(l) {
        format!("{years} an{s}")
    } else {
        format!("{years} year{s}")
    }
}

pub fn modified_ago(l: Lang, months: u32) -> String {
    match (is_fr(l), months) {
        (true, 0) => "modifié ce mois-ci".to_string(),
        (false, 0) => "modified this month".to_string(),
        (true, m) => format!("modifié il y a {}", format_age(l, m)),
        (false, m) => format!("modified {} ago", format_age(l, m)),
    }
}

pub fn items_count(l: Lang, n: u64) -> String {
    let c = group(l, n);
    if is_fr(l) {
        format!("{c} élément{}", if n > 1 { "s" } else { "" })
    } else {
        format!("{c} item{}", if n == 1 { "" } else { "s" })
    }
}

pub fn small_files(l: Lang, n: u64) -> String {
    let c = group(l, n);
    if is_fr(l) {
        let s = if n > 1 { "s" } else { "" };
        format!("{c} petit{s} fichier{s}")
    } else {
        format!("{c} small file{}", if n == 1 { "" } else { "s" })
    }
}

pub fn delete_button(l: Lang, n: usize, size: u64) -> String {
    let items = items_count(l, n as u64);
    let size = format_size(l, size);
    if is_fr(l) {
        format!("Supprimer {items} · {size}")
    } else {
        format!("Delete {items} · {size}")
    }
}

pub fn confirm_title(l: Lang, n: usize) -> String {
    let items = items_count(l, n as u64);
    if is_fr(l) {
        format!("Supprimer {items} ?")
    } else {
        format!("Delete {items}?")
    }
}

pub fn used_of(l: Lang, used: u64, total: u64) -> String {
    let (u, t) = (format_size(l, used), format_size(l, total));
    if is_fr(l) {
        format!("{u} utilisés sur {t}")
    } else {
        format!("{u} used of {t}")
    }
}

pub fn free_of(l: Lang, free: u64, total: u64) -> String {
    let (f, t) = (format_size(l, free), format_size(l, total));
    if is_fr(l) {
        format!("{f} libres sur {t}")
    } else {
        format!("{f} free of {t}")
    }
}

/// Whether `n` takes the plural: from 2 in French, for anything but 1 in English.
fn plural(l: Lang, n: u64) -> bool {
    if is_fr(l) { n > 1 } else { n != 1 }
}

/// « 1 fichier », « 2 fichiers » ; “1 file”, “2 files”.
fn files_count(l: Lang, n: u64) -> String {
    let (c, s) = (format_count(l, n), if plural(l, n) { "s" } else { "" });
    if is_fr(l) {
        format!("{c} fichier{s}")
    } else {
        format!("{c} file{s}")
    }
}

pub fn scan_done(l: Lang, secs: f64, files: u64) -> String {
    let (s, n) = (decimal(l, secs), files_count(l, files));
    if is_fr(l) {
        format!("Scan terminé en {s} s · {n}")
    } else {
        format!("Scan finished in {s} s · {n}")
    }
}

pub fn scanning(l: Lang, files: u64, secs: u64) -> String {
    let n = files_count(l, files);
    if is_fr(l) {
        format!("Scan en cours… {n} · {secs} s")
    } else {
        format!("Scanning… {n} · {secs} s")
    }
}

pub fn denied_dirs(l: Lang, n: u64) -> String {
    let (c, s) = (format_count(l, n), if plural(l, n) { "s" } else { "" });
    if is_fr(l) {
        format!("{c} dossier{s} inaccessible{s}")
    } else {
        format!("{c} inaccessible folder{s}")
    }
}

pub fn freed(l: Lang, bytes: u64) -> String {
    let x = format_size(l, bytes);
    if is_fr(l) {
        format!("✓ {x} libérés")
    } else {
        format!("✓ {x} freed")
    }
}

pub fn bin_still(l: Lang, bytes: u64) -> String {
    let x = format_size(l, bytes);
    if is_fr(l) {
        format!("Corbeille : {x} encore sur le disque")
    } else {
        format!("Recycle Bin: {x} still on disk")
    }
}

pub fn deleting(l: Lang, done: usize, total: usize) -> String {
    if is_fr(l) {
        format!("Suppression… {done}/{total}")
    } else {
        format!("Deleting… {done}/{total}")
    }
}

pub fn empty_bin_body(l: Lang, bytes: u64, drive: &str) -> String {
    let x = format_size(l, bytes);
    if is_fr(l) {
        format!("Les {x} de la corbeille de {drive} seront définitivement supprimés.")
    } else {
        format!("The {x} in the {drive} Recycle Bin will be permanently deleted.")
    }
}

pub fn block_message(l: Lang, b: Block) -> &'static str {
    match b {
        Block::DriveRoot => pick(
            l,
            "Racine du disque : ne peut pas être supprimée.",
            "Drive root: cannot be deleted.",
        ),
        Block::WindowsDir => pick(
            l,
            "Dossier système de Windows : passe par le Nettoyage de disque ou Paramètres › Stockage.",
            "Windows system folder: use Disk Cleanup or Settings › Storage.",
        ),
        Block::ProgramFiles => pick(
            l,
            "Programme installé : désinstalle-le via Paramètres › Applications.",
            "Installed program: uninstall it from Settings › Apps.",
        ),
        Block::WindowsApps => pick(
            l,
            "Applications du Microsoft Store : désinstalle-les via Paramètres › Applications.",
            "Microsoft Store apps: uninstall them from Settings › Apps.",
        ),
        Block::ProgramData => pick(
            l,
            "Données partagées des applications : bloqué par prudence.",
            "Shared application data: blocked to be safe.",
        ),
        Block::PageFile => pick(
            l,
            "Fichier d’échange de Windows : sa taille se règle dans Paramètres système avancés › Mémoire virtuelle.",
            "Windows page file: resize it in Advanced system settings › Virtual memory.",
        ),
        Block::Hiberfil => pick(
            l,
            "Fichier d’hibernation : désactivable avec « powercfg /h off » (en admin).",
            "Hibernation file: disable it with “powercfg /h off” (as admin).",
        ),
        Block::SystemFiles => pick(l, "Fichiers système de Windows.", "Windows system files."),
        Block::OldWindows => pick(
            l,
            "Ancienne installation de Windows : supprime-la via Paramètres › Stockage › Fichiers temporaires.",
            "Previous Windows installation: remove it from Settings › Storage › Temporary files.",
        ),
        Block::UsersRoot | Block::ProfileRoot => pick(
            l,
            "Les sous-dossiers d’un profil restent supprimables, pas le profil entier.",
            "A profile’s subfolders can be deleted, but not the whole profile.",
        ),
        Block::AppDataRoot => pick(
            l,
            "Contient les réglages de toutes tes applications.",
            "Holds the settings of all your apps.",
        ),
        Block::WindowsUserData => pick(
            l,
            "Données de Windows (menu Démarrer, recherche, mots de passe enregistrés…) : les supprimer casse ta session.",
            "Windows data (Start menu, search, saved passwords…): deleting it breaks your session.",
        ),
        Block::RecycleBinContent => pick(
            l,
            "Utilise « Vider la corbeille ».",
            "Use “Empty Recycle Bin”.",
        ),
    }
}

pub fn warning_message(l: Lang, w: Warning) -> &'static str {
    match w {
        Warning::GameLauncher => pick(
            l,
            "Jeu : mieux vaut le désinstaller depuis son lanceur (Steam, Epic, Xbox).",
            "Game: better uninstall it from its launcher (Steam, Epic, Xbox).",
        ),
        Warning::CloudSync => pick(
            l,
            "Dossier synchronisé : la suppression est aussi faite dans le cloud.",
            "Synced folder: it will also be deleted from the cloud.",
        ),
        Warning::AppData => pick(
            l,
            "Peut effacer des réglages d’applications. Les caches et Temp sont sans risque.",
            "May erase app settings. Caches and Temp are safe.",
        ),
        Warning::VirtualDisk => pick(
            l,
            "Disque virtuel : peut contenir une distribution Linux, Docker ou une machine virtuelle entière.",
            "Virtual disk: may hold a whole Linux distro, Docker or virtual machine.",
        ),
        Warning::OutlookData => pick(l, "Peut contenir tes e-mails.", "May contain your e-mails."),
    }
}

/// Name of a content type, as in the legend.
pub fn type_name(l: Lang, t: FileType) -> &'static str {
    match t {
        FileType::Game => pick(l, "Jeux", "Games"),
        FileType::Media => pick(l, "Vidéos & images", "Videos & images"),
        FileType::Archive => pick(l, "Archives & installateurs", "Archives & installers"),
        FileType::VirtualDisk => pick(l, "Disques virtuels", "Virtual disks"),
        FileType::Cache => pick(l, "Cache & temporaire", "Cache & temp files"),
        FileType::Program => pick(l, "Programmes", "Programs"),
        FileType::System => pick(l, "Système", "System"),
        FileType::Other => pick(l, "Autres", "Other"),
    }
}

/// Tooltip line of a file: "Type : Jeux".
pub fn type_line(l: Lang, t: FileType) -> String {
    let name = type_name(l, t);
    if is_fr(l) {
        format!("Type\u{a0}: {name}")
    } else {
        format!("Type: {name}")
    }
}

/// Tooltip line of a folder: its dominant type and that type's share
/// (0..=1), as a whole percentage: "Surtout : Jeux (62 %)".
pub fn mostly_line(l: Lang, t: FileType, share: f32) -> String {
    let name = type_name(l, t);
    let pct = (f64::from(share) * 100.0).round().clamp(0.0, 100.0) as u32;
    if is_fr(l) {
        format!("Surtout\u{a0}: {name} ({pct}\u{a0}%)")
    } else {
        format!("Mostly: {name} ({pct}%)")
    }
}

pub fn failure_reason(l: Lang, r: &FailureReason) -> String {
    match r {
        FailureReason::InUse => pick(
            l,
            "utilisé par un autre programme",
            "in use by another program",
        )
        .to_string(),
        FailureReason::AccessDenied => pick(l, "accès refusé", "access denied").to_string(),
        FailureReason::Cancelled => pick(l, "annulé", "cancelled").to_string(),
        FailureReason::Other(s) => s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NB: char = '\u{a0}';
    const NNB: char = '\u{202f}';

    #[test]
    fn locale() {
        assert_eq!(Lang::from_locale("fr-FR"), Lang::Fr);
        assert_eq!(Lang::from_locale("fr"), Lang::Fr);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("de-DE"), Lang::En);
    }
    #[test]
    fn sizes() {
        const G: u64 = 1 << 30;
        assert_eq!(format_size(Lang::Fr, 0), format!("0{NB}o"));
        assert_eq!(format_size(Lang::Fr, 1023), format!("1023{NB}o"));
        assert_eq!(format_size(Lang::Fr, 1536), format!("1,5{NB}Ko"));
        assert_eq!(format_size(Lang::Fr, 398 * G), format!("398{NB}Go"));
        assert_eq!(format_size(Lang::Fr, 1536 * G), format!("1,5{NB}To"));
        assert_eq!(format_size(Lang::En, 1536), "1.5 KB");
        assert_eq!(format_size(Lang::En, 398 * G), "398 GB");
    }
    #[test]
    fn size_rounding_rolls_over_to_the_next_unit() {
        const M: f64 = 1_048_576.0;
        let bytes = |mib: f64| (mib * M).round() as u64;
        assert_eq!(format_size(Lang::Fr, bytes(1023.6)), format!("1,0{NB}Go"));
        assert_eq!(format_size(Lang::En, bytes(1023.6)), "1.0 GB");
        assert_eq!(format_size(Lang::Fr, bytes(1023.4)), format!("1023{NB}Mo"));
        assert_eq!(format_size(Lang::Fr, 1_048_167), format!("1,0{NB}Mo"));
        assert_eq!(format_size(Lang::En, 1023), "1023 B");
        // No unit above TB.
        assert_eq!(format_size(Lang::Fr, 1024 << 40), format!("1024{NB}To"));
        assert_eq!(format_size(Lang::En, bytes(1023.6 * M)), "1024 TB");
    }
    #[test]
    fn counts_are_singular_or_plural() {
        assert_eq!(
            scan_done(Lang::Fr, 1.0, 1),
            "Scan terminé en 1,0 s · 1 fichier"
        );
        assert_eq!(
            scan_done(Lang::Fr, 1.0, 2),
            "Scan terminé en 1,0 s · 2 fichiers"
        );
        assert_eq!(
            scan_done(Lang::En, 1.0, 1),
            "Scan finished in 1.0 s · 1 file"
        );
        assert_eq!(
            scan_done(Lang::En, 1.0, 2),
            "Scan finished in 1.0 s · 2 files"
        );
        assert_eq!(scanning(Lang::Fr, 1, 7), "Scan en cours… 1 fichier · 7 s");
        assert_eq!(scanning(Lang::Fr, 2, 7), "Scan en cours… 2 fichiers · 7 s");
        assert_eq!(scanning(Lang::En, 1, 7), "Scanning… 1 file · 7 s");
        assert_eq!(scanning(Lang::En, 2, 7), "Scanning… 2 files · 7 s");
        assert_eq!(denied_dirs(Lang::Fr, 1), "1 dossier inaccessible");
        assert_eq!(denied_dirs(Lang::Fr, 2), "2 dossiers inaccessibles");
        assert_eq!(denied_dirs(Lang::En, 1), "1 inaccessible folder");
        assert_eq!(denied_dirs(Lang::En, 2), "2 inaccessible folders");
        // French counts 0 in the singular, English in the plural.
        assert_eq!(scanning(Lang::Fr, 0, 0), "Scan en cours… 0 fichier · 0 s");
        assert_eq!(scanning(Lang::En, 0, 0), "Scanning… 0 files · 0 s");
    }
    #[test]
    fn counts_and_percent() {
        assert_eq!(
            format_count(Lang::Fr, 1_284_312),
            format!("1{NNB}284{NNB}312")
        );
        assert_eq!(format_count(Lang::En, 1_284_312), "1,284,312");
        assert_eq!(format_percent(Lang::Fr, 83.6), format!("83,6{NB}%"));
        assert_eq!(format_percent(Lang::En, 83.6), "83.6%");
    }
    #[test]
    fn ages_and_plurals() {
        assert_eq!(format_age(Lang::Fr, 0), "ce mois-ci");
        assert_eq!(format_age(Lang::Fr, 3), "3 mois");
        assert_eq!(format_age(Lang::Fr, 16), "1 an");
        assert_eq!(format_age(Lang::Fr, 30), "2 ans");
        assert_eq!(format_age(Lang::En, 1), "1 month");
        assert_eq!(format_age(Lang::En, 30), "2 years");
        assert_eq!(modified_ago(Lang::Fr, 16), "modifié il y a 1 an");
        assert_eq!(modified_ago(Lang::En, 0), "modified this month");
        assert_eq!(items_count(Lang::Fr, 1), "1 élément");
        assert_eq!(small_files(Lang::En, 37), "37 small files");
        assert_eq!(confirm_title(Lang::Fr, 2), "Supprimer 2 éléments ?");
        assert_eq!(
            delete_button(Lang::En, 2, 80 << 30),
            "Delete 2 items · 80 GB"
        );
    }
    #[test]
    fn parameterised() {
        const G: u64 = 1 << 30;
        assert_eq!(
            used_of(Lang::Fr, 398 * G, 476 * G),
            format!("398{NB}Go utilisés sur 476{NB}Go")
        );
        assert_eq!(used_of(Lang::En, 398 * G, 476 * G), "398 GB used of 476 GB");
        assert_eq!(
            free_of(Lang::Fr, 78 * G, 476 * G),
            format!("78{NB}Go libres sur 476{NB}Go")
        );
        assert_eq!(free_of(Lang::En, 78 * G, 476 * G), "78 GB free of 476 GB");
        assert_eq!(
            scan_done(Lang::Fr, 3.84, 1_284),
            format!("Scan terminé en 3,8 s · 1{NNB}284 fichiers")
        );
        assert_eq!(
            scan_done(Lang::En, 3.84, 1_284),
            "Scan finished in 3.8 s · 1,284 files"
        );
        assert_eq!(
            scanning(Lang::Fr, 1_284, 7),
            format!("Scan en cours… 1{NNB}284 fichiers · 7 s")
        );
        assert_eq!(scanning(Lang::En, 1_284, 7), "Scanning… 1,284 files · 7 s");
        assert_eq!(denied_dirs(Lang::Fr, 12), "12 dossiers inaccessibles");
        assert_eq!(denied_dirs(Lang::En, 12), "12 inaccessible folders");
        assert_eq!(freed(Lang::Fr, 2 * G), format!("✓ 2,0{NB}Go libérés"));
        assert_eq!(freed(Lang::En, 2 * G), "✓ 2.0 GB freed");
        assert_eq!(
            bin_still(Lang::Fr, 50 * G),
            format!("Corbeille : 50{NB}Go encore sur le disque")
        );
        assert_eq!(
            bin_still(Lang::En, 50 * G),
            "Recycle Bin: 50 GB still on disk"
        );
        assert_eq!(deleting(Lang::Fr, 2, 5), "Suppression… 2/5");
        assert_eq!(deleting(Lang::En, 2, 5), "Deleting… 2/5");
        assert_eq!(
            empty_bin_body(Lang::Fr, 3 * G, "C:"),
            format!("Les 3,0{NB}Go de la corbeille de C: seront définitivement supprimés.")
        );
        assert_eq!(
            empty_bin_body(Lang::En, 3 * G, "C:"),
            "The 3.0 GB in the C: Recycle Bin will be permanently deleted."
        );
    }
    #[test]
    fn messages() {
        assert_eq!(
            block_message(Lang::Fr, Block::DriveRoot),
            "Racine du disque : ne peut pas être supprimée."
        );
        assert_eq!(
            block_message(Lang::En, Block::RecycleBinContent),
            "Use “Empty Recycle Bin”."
        );
        assert_eq!(
            warning_message(Lang::Fr, Warning::OutlookData),
            "Peut contenir tes e-mails."
        );
        assert_eq!(
            warning_message(Lang::En, Warning::CloudSync),
            "Synced folder: it will also be deleted from the cloud."
        );
        assert_eq!(
            failure_reason(Lang::Fr, &FailureReason::InUse),
            "utilisé par un autre programme"
        );
        assert_eq!(
            failure_reason(Lang::En, &FailureReason::AccessDenied),
            "access denied"
        );
        assert_eq!(
            failure_reason(Lang::En, &FailureReason::Cancelled),
            "cancelled"
        );
        assert_eq!(
            failure_reason(Lang::Fr, &FailureReason::Other("x".into())),
            "x"
        );
        assert_eq!(Lang::Fr.s().cancel, "Annuler");
        assert_eq!(Lang::En.s().choose_folder, "Choose a folder…");
    }
    #[test]
    fn file_types() {
        use FileType::*;
        let fr = [
            (Game, "Jeux"),
            (Media, "Vidéos & images"),
            (Archive, "Archives & installateurs"),
            (VirtualDisk, "Disques virtuels"),
            (Cache, "Cache & temporaire"),
            (Program, "Programmes"),
            (System, "Système"),
            (Other, "Autres"),
        ];
        let en = [
            (Game, "Games"),
            (Media, "Videos & images"),
            (Archive, "Archives & installers"),
            (VirtualDisk, "Virtual disks"),
            (Cache, "Cache & temp files"),
            (Program, "Programs"),
            (System, "System"),
            (Other, "Other"),
        ];
        for (t, name) in fr {
            assert_eq!(type_name(Lang::Fr, t), name);
        }
        for (t, name) in en {
            assert_eq!(type_name(Lang::En, t), name);
        }
        assert_eq!(type_line(Lang::Fr, Game), "Type\u{a0}: Jeux");
        assert_eq!(type_line(Lang::En, Game), "Type: Games");
        assert_eq!(mostly_line(Lang::En, Game, 0.62), "Mostly: Games (62%)");
        assert_eq!(
            mostly_line(Lang::Fr, Game, 0.62),
            format!("Surtout{NB}: Jeux (62{NB}%)")
        );
        assert_eq!(
            mostly_line(Lang::En, Archive, 0.996),
            "Mostly: Archives & installers (100%)"
        );
    }
}
