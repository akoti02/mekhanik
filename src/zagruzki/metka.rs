//! Метка «скачано из интернета» — поток NTFS `Zone.Identifier` (перенос `MarkOfWeb.cs`).
//! Её ставят браузеры и мессенджеры; по ней сторож узнаёт загрузку в любой папке, а не только в «Загрузках».
//! Метку сторож никогда не снимает: по ней и сама Windows (SmartScreen) предупредит при запуске.
//! При переносе в карантин на том же диске поток едет с файлом (`MoveFileEx`), сохранять отдельно не нужно.

/// Разобранная метка.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct МеткаСети {
    /// 3 — интернет, 4 — опасные сайты; 0–2 — свой компьютер и локальная сеть.
    pub зона: i32,
    pub хост: Option<String>,
    pub реферер: Option<String>,
}

impl МеткаСети {
    pub fn из_интернета(&self) -> bool {
        self.зона >= 3
    }

    /// Сайт, с которого скачано: «rutracker.org». `about:…` и пустое — не считаем.
    pub fn сайт(&self) -> Option<String> {
        for u in [self.реферер.as_deref(), self.хост.as_deref()].into_iter().flatten() {
            let u = u.trim();
            if u.is_empty() || u.to_ascii_lowercase().starts_with("about:") {
                continue;
            }
            if let Some(h) = хост_из_url(u) {
                return Some(h);
            }
        }
        None
    }
}

/// Хост из URL без крейтов: «https://www.rutracker.org/x» → «rutracker.org».
fn хост_из_url(u: &str) -> Option<String> {
    let после_схемы = u.split_once("://").map(|(_, х)| х).unwrap_or(u);
    let хост = после_схемы.split(['/', '?', '#']).next().unwrap_or("");
    // убрать логин@ и :порт
    let хост = хост.rsplit('@').next().unwrap_or(хост);
    let хост = хост.split(':').next().unwrap_or(хост);
    if хост.is_empty() {
        return None;
    }
    Some(хост.strip_prefix("www.").unwrap_or(хост).to_string())
}

/// Разобрать текст потока `Zone.Identifier` (INI-подобный). `None` — строки `ZoneId` нет.
pub fn разобрать(текст: &str) -> Option<МеткаСети> {
    let mut зона = -1i32;
    let mut хост = None;
    let mut реферер = None;
    for строка in текст.lines() {
        let Some((к, з)) = строка.split_once('=') else { continue };
        let (к, з) = (к.trim(), з.trim());
        if к.eq_ignore_ascii_case("ZoneId") {
            зона = з.parse().unwrap_or(-1);
        } else if к.eq_ignore_ascii_case("HostUrl") {
            хост = Some(з.to_string());
        } else if к.eq_ignore_ascii_case("ReferrerUrl") {
            реферер = Some(з.to_string());
        }
    }
    (зона >= 0).then_some(МеткаСети { зона, хост, реферер })
}

/// Прочитать метку файла. Открываем поток `<путь>:Zone.Identifier`; нет потока — `None` (файл не из интернета).
/// 🟥 Без `canonicalize`: он падал на занятом/пишущемся файле (гонка, нет прав) и файл из интернета молча становился
/// «без метки» — программа из сети не придерживалась. Путь сторож и так даёт абсолютный; поток читаем прямо.
pub fn прочитать(путь: &std::path::Path) -> Option<МеткаСети> {
    let текст = std::fs::read_to_string(путь_потока(путь)).ok()?;
    if текст.len() > 64 * 1024 {
        return None;
    }
    разобрать(&текст)
}

/// Чистая (видно в тесте): путь к потоку метки `<файл>:Zone.Identifier`. Длинный путь (≥248) получает префикс `\\?\`,
/// иначе Windows не откроет поток.
pub fn путь_потока(путь: &std::path::Path) -> String {
    let п = путь.display().to_string().replace('/', "\\");
    if п.len() >= 248 && !п.starts_with(r"\\?\") {
        format!(r"\\?\{п}:Zone.Identifier")
    } else {
        format!("{п}:Zone.Identifier")
    }
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn метка_из_интернета() {
        let т = "[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://www.rutracker.org/forum/x.php\r\nReferrerUrl=https://rutracker.org/\r\n";
        let м = разобрать(т).expect("есть ZoneId");
        assert!(м.из_интернета() && м.зона == 3);
        assert_eq!(м.сайт().as_deref(), Some("rutracker.org"), "реферер первым, www убран");
    }

    #[test]
    fn метка_своя_зона() {
        let м = разобрать("[ZoneTransfer]\nZoneId=1\n").unwrap();
        assert!(!м.из_интернета(), "зона 1 — локальная сеть, не интернет");
    }

    #[test]
    fn нет_зоны() {
        assert!(разобрать("[ZoneTransfer]\nHostUrl=x\n").is_none(), "без ZoneId — не метка");
    }

    #[test]
    fn about_internet_не_сайт() {
        // у сохранённой страницы расширение ставит about:internet — это не сайт
        let м = разобрать("[ZoneTransfer]\nZoneId=3\nHostUrl=about:internet\n").unwrap();
        assert!(м.из_интернета() && м.сайт().is_none());
    }

    #[test]
    fn хост_с_портом_и_логином() {
        assert_eq!(хост_из_url("https://user@files.example.com:8443/a/b"), Some("files.example.com".to_string()));
    }

    #[test]
    fn путь_потока_прямой_без_canonicalize() {
        // обычный абсолютный путь — поток дописан как есть, без \\?\ и без обращения к диску
        assert_eq!(путь_потока(std::path::Path::new(r"C:\Users\akoti\Downloads\прога.exe")),
            r"C:\Users\akoti\Downloads\прога.exe:Zone.Identifier");
        // косые черты приводятся к обратным
        assert_eq!(путь_потока(std::path::Path::new("C:/Downloads/a.bin")), r"C:\Downloads\a.bin:Zone.Identifier");
        // длинный путь получает префикс \\?\
        let длинный = format!(r"C:\Downloads\{}\x.exe", "a".repeat(260));
        assert!(путь_потока(std::path::Path::new(&длинный)).starts_with(r"\\?\"));
    }
}
