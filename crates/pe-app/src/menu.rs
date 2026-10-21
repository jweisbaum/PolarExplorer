//! The native menu, in the interface language (spec.md 3.5).
//!
//! Tauri's default menu names its items in English whatever the interface
//! says, so the whole menu is built here from one table and rebuilt when the
//! language changes. Two items are ours rather than the platform's: Quit,
//! which has to go through the unsaved-changes guard (spec.md 3.3), and
//! PolarExplorer Help, which opens the help window.

use tauri::menu::{
    AboutMetadata, HELP_SUBMENU_ID, Menu, MenuItem, PredefinedMenuItem, Submenu, WINDOW_SUBMENU_ID,
};

/// The id of our Quit item.
pub const QUIT_ID: &str = "pe-quit";
/// The id of the Help window item.
pub const HELP_ID: &str = "pe-help";

/// Every label the menu shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// "About PolarExplorer".
    About,
    /// The macOS Services submenu.
    Services,
    /// "Hide PolarExplorer".
    Hide,
    /// "Hide Others".
    HideOthers,
    /// "Show All".
    ShowAll,
    /// "Quit PolarExplorer".
    Quit,
    /// The File menu (Windows and Linux, where it holds Quit).
    File,
    /// The Edit menu.
    Edit,
    /// "Undo".
    Undo,
    /// "Redo".
    Redo,
    /// "Cut".
    Cut,
    /// "Copy".
    Copy,
    /// "Paste".
    Paste,
    /// "Select All".
    SelectAll,
    /// The View menu.
    View,
    /// "Enter Full Screen".
    Fullscreen,
    /// The Window menu.
    Window,
    /// "Minimize".
    Minimize,
    /// "Zoom" (maximise).
    Maximize,
    /// The Help menu.
    Help,
    /// "PolarExplorer Help".
    AppHelp,
    /// The ORC catalogue's provenance in About (spec.md 5.1), a template
    /// filled by [`about_orc`].
    OrcCatalogue,
    /// Data providers credited in the About panel.
    DataCredits,
}

/// Every label, for the test that each has a translation.
pub const ALL: &[Label] = &[
    Label::About,
    Label::Services,
    Label::Hide,
    Label::HideOthers,
    Label::ShowAll,
    Label::Quit,
    Label::File,
    Label::Edit,
    Label::Undo,
    Label::Redo,
    Label::Cut,
    Label::Copy,
    Label::Paste,
    Label::SelectAll,
    Label::View,
    Label::Fullscreen,
    Label::Window,
    Label::Minimize,
    Label::Maximize,
    Label::Help,
    Label::AppHelp,
    Label::OrcCatalogue,
    Label::DataCredits,
];

/// A menu label in `language` (one of `settings::LANGUAGES`), English for
/// anything else. The wording follows each platform's own menus in that
/// language, so ours reads like every other application's.
pub fn text(language: &str, label: Label) -> &'static str {
    use Label as L;
    match (language, label) {
        ("fr", L::About) => "À propos de PolarExplorer",
        ("fr", L::Services) => "Services",
        ("fr", L::Hide) => "Masquer PolarExplorer",
        ("fr", L::HideOthers) => "Masquer les autres",
        ("fr", L::ShowAll) => "Tout afficher",
        ("fr", L::Quit) => "Quitter PolarExplorer",
        ("fr", L::File) => "Fichier",
        ("fr", L::Edit) => "Édition",
        ("fr", L::Undo) => "Annuler",
        ("fr", L::Redo) => "Rétablir",
        ("fr", L::Cut) => "Couper",
        ("fr", L::Copy) => "Copier",
        ("fr", L::Paste) => "Coller",
        ("fr", L::SelectAll) => "Tout sélectionner",
        ("fr", L::View) => "Présentation",
        ("fr", L::Fullscreen) => "Passer en plein écran",
        ("fr", L::Window) => "Fenêtre",
        ("fr", L::Minimize) => "Réduire",
        ("fr", L::Maximize) => "Agrandir",
        ("fr", L::Help) => "Aide",
        ("fr", L::AppHelp) => "Aide de PolarExplorer",
        ("fr", L::OrcCatalogue) => {
            "Catalogue ORC : {records} certificats de jieter/orc-data, commit {commit} du {date}, construit le {built}."
        }
        ("fr", L::DataCredits) => {
            "Données : jieter/orc-data (MIT) ; ERA5 d’ECMWF / Copernicus Climate Change Service, via ARCO-ERA5 ; E.U. Copernicus Marine Service Information ; fond de carte Natural Earth (domaine public).\n\nLe guide et les mentions des sources sont fournis dans le dossier documentation de l’application."
        }

        ("de", L::About) => "Über PolarExplorer",
        ("de", L::Services) => "Dienste",
        ("de", L::Hide) => "PolarExplorer ausblenden",
        ("de", L::HideOthers) => "Andere ausblenden",
        ("de", L::ShowAll) => "Alle einblenden",
        ("de", L::Quit) => "PolarExplorer beenden",
        ("de", L::File) => "Datei",
        ("de", L::Edit) => "Bearbeiten",
        ("de", L::Undo) => "Widerrufen",
        ("de", L::Redo) => "Wiederholen",
        ("de", L::Cut) => "Ausschneiden",
        ("de", L::Copy) => "Kopieren",
        ("de", L::Paste) => "Einfügen",
        ("de", L::SelectAll) => "Alles auswählen",
        ("de", L::View) => "Darstellung",
        ("de", L::Fullscreen) => "Vollbildmodus aktivieren",
        ("de", L::Window) => "Fenster",
        ("de", L::Minimize) => "Minimieren",
        ("de", L::Maximize) => "Zoomen",
        ("de", L::Help) => "Hilfe",
        ("de", L::AppHelp) => "PolarExplorer-Hilfe",
        ("de", L::OrcCatalogue) => {
            "ORC-Katalog: {records} Messbriefe aus jieter/orc-data, Commit {commit} vom {date}, erstellt am {built}."
        }
        ("de", L::DataCredits) => {
            "Daten: jieter/orc-data (MIT); ERA5 von ECMWF / Copernicus Climate Change Service, über ARCO-ERA5; E.U. Copernicus Marine Service Information; Kartengrundlage von Natural Earth (gemeinfrei).\n\nDas Handbuch und die Quellenangaben liegen im Ordner documentation der Anwendung."
        }

        ("es", L::About) => "Acerca de PolarExplorer",
        ("es", L::Services) => "Servicios",
        ("es", L::Hide) => "Ocultar PolarExplorer",
        ("es", L::HideOthers) => "Ocultar otros",
        ("es", L::ShowAll) => "Mostrar todo",
        ("es", L::Quit) => "Salir de PolarExplorer",
        ("es", L::File) => "Archivo",
        ("es", L::Edit) => "Edición",
        ("es", L::Undo) => "Deshacer",
        ("es", L::Redo) => "Rehacer",
        ("es", L::Cut) => "Cortar",
        ("es", L::Copy) => "Copiar",
        ("es", L::Paste) => "Pegar",
        ("es", L::SelectAll) => "Seleccionar todo",
        ("es", L::View) => "Visualización",
        ("es", L::Fullscreen) => "Entrar en pantalla completa",
        ("es", L::Window) => "Ventana",
        ("es", L::Minimize) => "Minimizar",
        ("es", L::Maximize) => "Zoom",
        ("es", L::Help) => "Ayuda",
        ("es", L::AppHelp) => "Ayuda de PolarExplorer",
        ("es", L::OrcCatalogue) => {
            "Catálogo ORC: {records} certificados de jieter/orc-data, commit {commit} del {date}, compilado el {built}."
        }
        ("es", L::DataCredits) => {
            "Datos: jieter/orc-data (MIT); ERA5 de ECMWF / Copernicus Climate Change Service, a través de ARCO-ERA5; E.U. Copernicus Marine Service Information; mapa base de Natural Earth (dominio público).\n\nLa guía del usuario y los avisos de las fuentes de datos se incluyen en la carpeta documentation de la aplicación."
        }

        ("it", L::About) => "Informazioni su PolarExplorer",
        ("it", L::Services) => "Servizi",
        ("it", L::Hide) => "Nascondi PolarExplorer",
        ("it", L::HideOthers) => "Nascondi altre",
        ("it", L::ShowAll) => "Mostra tutte",
        ("it", L::Quit) => "Esci da PolarExplorer",
        ("it", L::File) => "File",
        ("it", L::Edit) => "Composizione",
        ("it", L::Undo) => "Annulla",
        ("it", L::Redo) => "Ripristina",
        ("it", L::Cut) => "Taglia",
        ("it", L::Copy) => "Copia",
        ("it", L::Paste) => "Incolla",
        ("it", L::SelectAll) => "Seleziona tutto",
        ("it", L::View) => "Vista",
        ("it", L::Fullscreen) => "Attiva modalità a tutto schermo",
        ("it", L::Window) => "Finestra",
        ("it", L::Minimize) => "Contrai",
        ("it", L::Maximize) => "Zoom",
        ("it", L::Help) => "Aiuto",
        ("it", L::AppHelp) => "Aiuto di PolarExplorer",
        ("it", L::OrcCatalogue) => {
            "Catalogo ORC: {records} certificati da jieter/orc-data, commit {commit} del {date}, compilato il {built}."
        }
        ("it", L::DataCredits) => {
            "Dati: jieter/orc-data (MIT); ERA5 di ECMWF / Copernicus Climate Change Service, tramite ARCO-ERA5; E.U. Copernicus Marine Service Information; mappa di base Natural Earth (pubblico dominio).\n\nLa guida per l’utente e le note sulle fonti dei dati si trovano nella cartella documentation dell’applicazione."
        }

        ("nl", L::About) => "Over PolarExplorer",
        ("nl", L::Services) => "Voorzieningen",
        ("nl", L::Hide) => "Verberg PolarExplorer",
        ("nl", L::HideOthers) => "Verberg andere",
        ("nl", L::ShowAll) => "Toon alles",
        ("nl", L::Quit) => "Stop PolarExplorer",
        ("nl", L::File) => "Archief",
        ("nl", L::Edit) => "Wijzig",
        ("nl", L::Undo) => "Herstel",
        ("nl", L::Redo) => "Opnieuw",
        ("nl", L::Cut) => "Knip",
        ("nl", L::Copy) => "Kopieer",
        ("nl", L::Paste) => "Plak",
        ("nl", L::SelectAll) => "Selecteer alles",
        ("nl", L::View) => "Weergave",
        ("nl", L::Fullscreen) => "Schakel schermvullende weergave in",
        ("nl", L::Window) => "Venster",
        ("nl", L::Minimize) => "Minimaliseer",
        ("nl", L::Maximize) => "Zoom",
        ("nl", L::Help) => "Help",
        ("nl", L::AppHelp) => "PolarExplorer-help",
        ("nl", L::OrcCatalogue) => {
            "ORC-catalogus: {records} meetbrieven uit jieter/orc-data, commit {commit} van {date}, gebouwd op {built}."
        }
        ("nl", L::DataCredits) => {
            "Gegevens: jieter/orc-data (MIT); ERA5 van ECMWF / Copernicus Climate Change Service, via ARCO-ERA5; E.U. Copernicus Marine Service Information; basiskaart van Natural Earth (publiek domein).\n\nDe gebruikershandleiding en de vermeldingen van de gegevensbronnen staan in de map documentation van de app."
        }

        ("zh", L::About) => "关于 PolarExplorer",
        ("zh", L::Services) => "服务",
        ("zh", L::Hide) => "隐藏 PolarExplorer",
        ("zh", L::HideOthers) => "隐藏其他",
        ("zh", L::ShowAll) => "全部显示",
        ("zh", L::Quit) => "退出 PolarExplorer",
        ("zh", L::File) => "文件",
        ("zh", L::Edit) => "编辑",
        ("zh", L::Undo) => "撤销",
        ("zh", L::Redo) => "重做",
        ("zh", L::Cut) => "剪切",
        ("zh", L::Copy) => "拷贝",
        ("zh", L::Paste) => "粘贴",
        ("zh", L::SelectAll) => "全选",
        ("zh", L::View) => "显示",
        ("zh", L::Fullscreen) => "进入全屏幕",
        ("zh", L::Window) => "窗口",
        ("zh", L::Minimize) => "最小化",
        ("zh", L::Maximize) => "缩放",
        ("zh", L::Help) => "帮助",
        ("zh", L::AppHelp) => "PolarExplorer 帮助",
        ("zh", L::OrcCatalogue) => {
            "ORC 目录：来自 jieter/orc-data 的 {records} 份证书，提交 {commit}（{date}），构建于 {built}。"
        }
        ("zh", L::DataCredits) => {
            "数据：jieter/orc-data (MIT)；ECMWF / Copernicus Climate Change Service 的 ERA5，经由 ARCO-ERA5；E.U. Copernicus Marine Service Information；Natural Earth 底图（公有领域）。\n\n用户指南和数据来源声明位于应用程序的 documentation 文件夹中。"
        }

        ("ja", L::About) => "PolarExplorer について",
        ("ja", L::Services) => "サービス",
        ("ja", L::Hide) => "PolarExplorer を隠す",
        ("ja", L::HideOthers) => "ほかを隠す",
        ("ja", L::ShowAll) => "すべてを表示",
        ("ja", L::Quit) => "PolarExplorer を終了",
        ("ja", L::File) => "ファイル",
        ("ja", L::Edit) => "編集",
        ("ja", L::Undo) => "取り消す",
        ("ja", L::Redo) => "やり直す",
        ("ja", L::Cut) => "カット",
        ("ja", L::Copy) => "コピー",
        ("ja", L::Paste) => "ペースト",
        ("ja", L::SelectAll) => "すべてを選択",
        ("ja", L::View) => "表示",
        ("ja", L::Fullscreen) => "フルスクリーンにする",
        ("ja", L::Window) => "ウインドウ",
        ("ja", L::Minimize) => "しまう",
        ("ja", L::Maximize) => "拡大／縮小",
        ("ja", L::Help) => "ヘルプ",
        ("ja", L::AppHelp) => "PolarExplorer ヘルプ",
        ("ja", L::OrcCatalogue) => {
            "ORC カタログ：jieter/orc-data の証書 {records} 件、コミット {commit}（{date}）、ビルド {built}。"
        }
        ("ja", L::DataCredits) => {
            "データ：jieter/orc-data (MIT)、ECMWF / Copernicus Climate Change Service の ERA5（ARCO-ERA5 経由）、E.U. Copernicus Marine Service Information、Natural Earth のベースマップ（パブリックドメイン）。\n\nユーザーガイドとデータソースの表記は、アプリケーションの documentation フォルダにあります。"
        }

        ("ar", L::About) => "حول PolarExplorer",
        ("ar", L::Services) => "الخدمات",
        ("ar", L::Hide) => "إخفاء PolarExplorer",
        ("ar", L::HideOthers) => "إخفاء الآخرين",
        ("ar", L::ShowAll) => "إظهار الكل",
        ("ar", L::Quit) => "إنهاء PolarExplorer",
        ("ar", L::File) => "ملف",
        ("ar", L::Edit) => "تحرير",
        ("ar", L::Undo) => "تراجع",
        ("ar", L::Redo) => "إعادة",
        ("ar", L::Cut) => "قص",
        ("ar", L::Copy) => "نسخ",
        ("ar", L::Paste) => "لصق",
        ("ar", L::SelectAll) => "تحديد الكل",
        ("ar", L::View) => "عرض",
        ("ar", L::Fullscreen) => "الدخول إلى ملء الشاشة",
        ("ar", L::Window) => "نافذة",
        ("ar", L::Minimize) => "تصغير",
        ("ar", L::Maximize) => "تكبير/تصغير",
        ("ar", L::Help) => "مساعدة",
        ("ar", L::AppHelp) => "مساعدة PolarExplorer",
        ("ar", L::OrcCatalogue) => {
            "كتالوج ORC: ‏{records} شهادة من jieter/orc-data، الإيداع {commit} بتاريخ {date}، بُني في {built}."
        }
        ("ar", L::DataCredits) => {
            "البيانات: jieter/orc-data (MIT)؛ ERA5 من ECMWF / Copernicus Climate Change Service عبر ARCO-ERA5؛ E.U. Copernicus Marine Service Information؛ خريطة الأساس من Natural Earth (ملكية عامة).\n\nدليل المستخدم وإشعارات مصادر البيانات موجودة في مجلد documentation الخاص بالتطبيق."
        }

        (_, L::About) => "About PolarExplorer",
        (_, L::Services) => "Services",
        (_, L::Hide) => "Hide PolarExplorer",
        (_, L::HideOthers) => "Hide Others",
        (_, L::ShowAll) => "Show All",
        (_, L::Quit) => "Quit PolarExplorer",
        (_, L::File) => "File",
        (_, L::Edit) => "Edit",
        (_, L::Undo) => "Undo",
        (_, L::Redo) => "Redo",
        (_, L::Cut) => "Cut",
        (_, L::Copy) => "Copy",
        (_, L::Paste) => "Paste",
        (_, L::SelectAll) => "Select All",
        (_, L::View) => "View",
        (_, L::Fullscreen) => "Enter Full Screen",
        (_, L::Window) => "Window",
        (_, L::Minimize) => "Minimize",
        (_, L::Maximize) => "Zoom",
        (_, L::Help) => "Help",
        (_, L::AppHelp) => "PolarExplorer Help",
        (_, L::OrcCatalogue) => {
            "ORC catalogue: {records} certificates from jieter/orc-data, commit {commit} of {date}, built {built}."
        }
        (_, L::DataCredits) => {
            "Data: jieter/orc-data (MIT); ERA5 from ECMWF / Copernicus Climate Change Service, via ARCO-ERA5; E.U. Copernicus Marine Service Information; Natural Earth basemap (public domain).\n\nThe user guide and data source notices are included in the application’s documentation folder."
        }
    }
}

/// The ORC catalogue's provenance for About, in `language`: how many
/// certificates, the orc-data commit and its date, and the build date. Reads
/// only the catalogue's header. `None` if the embedded catalogue is damaged.
pub fn about_orc(language: &str) -> Option<String> {
    let provenance = pe_orc::provenance().ok()?;
    let commit: String = provenance.commit.chars().take(10).collect();
    Some(
        text(language, Label::OrcCatalogue)
            .replace("{records}", &provenance.records.to_string())
            .replace("{commit}", &commit)
            .replace("{date}", &provenance.commit_date)
            .replace("{built}", &provenance.build_date),
    )
}

/// Keep the data credits even if a damaged catalogue has no provenance.
pub fn about_credits(language: &str) -> String {
    let credits = text(language, Label::DataCredits);
    match about_orc(language) {
        Some(provenance) => format!("{provenance}\n\n{credits}"),
        None => credits.to_owned(),
    }
}

/// Builds the menu in `language`.
pub fn build<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    language: &str,
) -> tauri::Result<Menu<R>> {
    let l = |label| Some(text(language, label));
    let info = app.package_info();
    // macOS shows `credits` in its About panel, the others `comments`.
    let credits = about_credits(language);
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        comments: Some(credits.clone()),
        credits: Some(credits),
        ..AboutMetadata::default()
    };
    let quit = MenuItem::with_id(
        app,
        QUIT_ID,
        text(language, Label::Quit),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let help = MenuItem::with_id(
        app,
        HELP_ID,
        text(language, Label::AppHelp),
        true,
        Some("F1"),
    )?;

    #[cfg(target_os = "macos")]
    let app_menu = Submenu::with_items(
        app,
        info.name.clone(),
        true,
        &[
            &PredefinedMenuItem::about(app, l(Label::About), Some(about.clone()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, l(Label::Services))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, l(Label::Hide))?,
            &PredefinedMenuItem::hide_others(app, l(Label::HideOthers))?,
            &PredefinedMenuItem::show_all(app, l(Label::ShowAll))?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    // No Close Window item: its fixed Cmd/Ctrl-W accelerator would take the
    // chord spec.md 3.2 gives to closing the project. The window's own close
    // button still closes it, through the same guard as Quit.
    #[cfg(not(target_os = "macos"))]
    let file_menu = Submenu::with_items(app, text(language, Label::File), true, &[&quit])?;

    let edit_menu = Submenu::with_items(
        app,
        text(language, Label::Edit),
        true,
        &[
            &PredefinedMenuItem::undo(app, l(Label::Undo))?,
            &PredefinedMenuItem::redo(app, l(Label::Redo))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, l(Label::Cut))?,
            &PredefinedMenuItem::copy(app, l(Label::Copy))?,
            &PredefinedMenuItem::paste(app, l(Label::Paste))?,
            &PredefinedMenuItem::select_all(app, l(Label::SelectAll))?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    let view_menu = Submenu::with_items(
        app,
        text(language, Label::View),
        true,
        &[&PredefinedMenuItem::fullscreen(app, l(Label::Fullscreen))?],
    )?;

    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        text(language, Label::Window),
        true,
        &[
            &PredefinedMenuItem::minimize(app, l(Label::Minimize))?,
            &PredefinedMenuItem::maximize(app, l(Label::Maximize))?,
        ],
    )?;

    let help_menu = Submenu::with_id_and_items(
        app,
        HELP_SUBMENU_ID,
        text(language, Label::Help),
        true,
        &[
            &help,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::separator(app)?,
            #[cfg(not(target_os = "macos"))]
            &PredefinedMenuItem::about(app, l(Label::About), Some(about))?,
        ],
    )?;
    #[cfg(target_os = "macos")]
    let _ = about;

    Menu::with_items(
        app,
        &[
            #[cfg(target_os = "macos")]
            &app_menu,
            #[cfg(not(target_os = "macos"))]
            &file_menu,
            &edit_menu,
            #[cfg(target_os = "macos")]
            &view_menu,
            &window_menu,
            &help_menu,
        ],
    )
}

/// Builds the menu in `language` and makes it the application's. A failure
/// leaves the previous menu, which is still a working menu.
pub fn install<R: tauri::Runtime>(app: &tauri::AppHandle<R>, language: &str) {
    if let Ok(menu) = build(app, language) {
        let _ = app.set_menu(menu);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::LANGUAGES;

    #[test]
    fn every_label_exists_in_every_language_and_names_the_app_where_english_does() {
        for &language in LANGUAGES {
            for &label in ALL {
                let english = text("en", label);
                let translated = text(language, label);
                assert!(!translated.trim().is_empty(), "{language} {label:?}");
                assert_eq!(
                    english.contains("PolarExplorer"),
                    translated.contains("PolarExplorer"),
                    "{language} {label:?}"
                );
            }
        }
    }

    /// The menu really changes language: apart from words a platform's menus
    /// share with English in that language, every label differs from English.
    #[test]
    fn switching_language_changes_the_labels() {
        for language in crate::settings::LANGUAGES
            .iter()
            .copied()
            .filter(|&l| l != "en")
        {
            let same: Vec<Label> = ALL
                .iter()
                .copied()
                .filter(|&label| text(language, label) == text("en", label))
                .collect();
            // Words each platform's menus share with English in that language.
            let allowed: &[Label] = match language {
                "fr" => &[Label::Services],
                "es" => &[Label::Maximize],
                "it" => &[Label::File, Label::Maximize],
                "nl" => &[Label::Maximize, Label::Help],
                _ => &[],
            };
            assert_eq!(same, allowed, "{language}");
        }
        assert_eq!(text("tlh", Label::Quit), "Quit PolarExplorer");
    }

    /// About names the catalogue's source commit and dates (spec.md 5.1), in
    /// every language, with every placeholder filled.
    #[test]
    fn about_names_the_orc_catalogue_provenance() {
        let provenance = pe_orc::provenance().unwrap();
        for &language in LANGUAGES {
            let about = about_credits(language);
            assert!(about.contains(&provenance.commit[..10]), "{about}");
            assert!(about.contains(&provenance.commit_date), "{about}");
            assert!(about.contains(&provenance.build_date), "{about}");
            assert!(about.contains(&provenance.records.to_string()), "{about}");
            assert!(!about.contains('{'), "{about}");
            for source in [
                "jieter/orc-data (MIT)",
                "ECMWF",
                "Copernicus Climate Change Service",
                "ARCO-ERA5",
                "Copernicus Marine",
                "Natural Earth",
            ] {
                assert!(about.contains(source), "{language}: missing {source}");
            }
        }
    }
}
