//! Что внутри файла на самом деле: сигнатура (первые байты), PE-заголовок, образцы команд в тексте,
//! JavaScript и самозапуск в PDF, макросы Office, имена внутри архива ZIP.
//! Перенос `ContentInspector.Sniff`, `Peek.cs` из «Кино Киры — Отправки» (13.11), без сети.
//! Всё — над срезами байт, чтобы было видно в тестах.

use super::vid::Род;

/// Что это за контейнер по первым байтам.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Контейнер {
    Pe,        // программа Windows (MZ)
    Elf,       // программа Linux
    MachO,     // программа macOS / Java fat
    Zip,       // ZIP, а также docx/xlsx/pptx/jar/apk
    Rar,
    SevenZip,
    Cab,
    Gzip,
    Bzip2,
    Xz,
    Zstd,
    Ole,       // старый Office или установщик MSI
    Pdf,
    Rtf,
    Lnk,       // ярлык
    Iso,
    Swf,       // Flash — внутри код
    Видео,
    Картинка,
    Звук,
    Текст,
    Неизвестно,
}

/// Совпадение байт в начале среза.
fn вначале(h: &[u8], off: usize, образец: &[u8]) -> bool {
    h.len() >= off + образец.len() && &h[off..off + образец.len()] == образец
}

/// Сигнатура по первым байтам (как `ContentInspector.Sniff`). `len` — полный размер файла.
pub fn sniff(h: &[u8], len: u64) -> Контейнер {
    if len == 0 {
        return Контейнер::Неизвестно;
    }
    if вначале(h, 0, b"MZ") {
        return Контейнер::Pe;
    }
    if вначале(h, 0, &[0x7F, b'E', b'L', b'F']) {
        return Контейнер::Elf;
    }
    if вначале(h, 0, &[0xCF, 0xFA, 0xED, 0xFE])
        || вначале(h, 0, &[0xCE, 0xFA, 0xED, 0xFE])
        || вначале(h, 0, &[0xFE, 0xED, 0xFA, 0xCE])
        || вначале(h, 0, &[0xFE, 0xED, 0xFA, 0xCF])
        || вначале(h, 0, &[0xCA, 0xFE, 0xBA, 0xBE])
    {
        return Контейнер::MachO;
    }
    if вначале(h, 0, b"PK\x03\x04") || вначале(h, 0, b"PK\x05\x06") || вначале(h, 0, b"PK\x07\x08") {
        return Контейнер::Zip;
    }
    if вначале(h, 0, b"Rar!\x1A\x07") {
        return Контейнер::Rar;
    }
    if вначале(h, 0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Контейнер::SevenZip;
    }
    if вначале(h, 0, b"MSCF") {
        return Контейнер::Cab;
    }
    if вначале(h, 0, &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Контейнер::Ole;
    }
    if вначале(h, 0, b"%PDF") {
        return Контейнер::Pdf;
    }
    if вначале(h, 0, b"{\\rtf") {
        return Контейнер::Rtf;
    }
    if вначале(h, 0, &[0x1F, 0x8B]) {
        return Контейнер::Gzip;
    }
    if вначале(h, 0, b"BZh") {
        return Контейнер::Bzip2;
    }
    if вначале(h, 0, &[0xFD, b'7', b'z', b'X', b'Z', 0x00]) {
        return Контейнер::Xz;
    }
    if вначале(h, 0, &[0x28, 0xB5, 0x2F, 0xFD]) {
        return Контейнер::Zstd;
    }
    if вначале(h, 0, &[0x4C, 0x00, 0x00, 0x00, 0x01, 0x14, 0x02, 0x00]) {
        return Контейнер::Lnk;
    }
    if вначале(h, 0, b"FWS") || вначале(h, 0, b"CWS") || вначале(h, 0, b"ZWS") {
        return Контейнер::Swf;
    }
    if вначале(h, 0x8001, b"CD001") {
        return Контейнер::Iso;
    }
    // видео и картинки — чтобы маскировка видела «безобидный» настоящий тип
    if h.len() >= 12 && вначале(h, 4, b"ftyp") {
        return Контейнер::Видео;
    }
    if вначале(h, 0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        return Контейнер::Видео; // Matroska/WebM
    }
    if вначале(h, 0, b"RIFF") && вначале(h, 8, b"AVI ") {
        return Контейнер::Видео;
    }
    if вначале(h, 0, b"RIFF") && вначале(h, 8, b"WAVE") {
        return Контейнер::Звук;
    }
    if вначале(h, 0, &[0x30, 0x26, 0xB2, 0x75]) {
        return Контейнер::Видео; // ASF/WMV
    }
    if вначале(h, 0, &[0xFF, 0xD8, 0xFF]) {
        return Контейнер::Картинка; // JPEG
    }
    if вначале(h, 0, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Контейнер::Картинка;
    }
    if вначале(h, 0, b"GIF87a") || вначале(h, 0, b"GIF89a") {
        return Контейнер::Картинка;
    }
    if похож_на_текст(h) {
        return Контейнер::Текст;
    }
    Контейнер::Неизвестно
}

/// Род по контейнеру — для проверки маскировки (`vid::маскировка`).
pub fn род_контейнера(к: Контейнер) -> Род {
    match к {
        Контейнер::Pe | Контейнер::Elf | Контейнер::MachO => Род::Программа,
        Контейнер::Lnk => Род::Ярлык,
        Контейнер::Iso => Род::ОбразДиска,
        Контейнер::Zip | Контейнер::Rar | Контейнер::SevenZip | Контейнер::Cab | Контейнер::Gzip
        | Контейнер::Bzip2 | Контейнер::Xz | Контейнер::Zstd => Род::Архив,
        Контейнер::Swf => Род::Скрипт,
        Контейнер::Pdf | Контейнер::Rtf | Контейнер::Ole => Род::Документ,
        Контейнер::Видео => Род::Видео,
        Контейнер::Картинка => Род::Картинка,
        Контейнер::Звук => Род::Звук,
        Контейнер::Текст => Род::Текст,
        Контейнер::Неизвестно => Род::Прочее,
    }
}

/// Похоже ли на текст: почти все байты печатные (как `LooksText`), нулей нет.
fn похож_на_текст(h: &[u8]) -> bool {
    let срез = &h[..h.len().min(4096)];
    if срез.is_empty() {
        return false;
    }
    let mut печатных = 0usize;
    for &b in срез {
        if b == 0 {
            return false;
        }
        if b == 9 || b == 10 || b == 13 || b >= 0x20 {
            печатных += 1;
        }
    }
    печатных as f64 >= срез.len() as f64 * 0.97
}

/// Настоящий PE-заголовок: «MZ», смещение e_lfanew и подпись «PE\0\0». Так отличаем настоящую программу
/// от файла, который просто начинается с «MZ».
#[allow(dead_code)] // сведение: сверяется в тестах; разбор пока решает по сигнатуре
pub fn pe_заголовок(h: &[u8]) -> bool {
    if !вначале(h, 0, b"MZ") || h.len() < 0x40 {
        return false;
    }
    let off = u32::from_le_bytes([h[0x3C], h[0x3D], h[0x3E], h[0x3F]]) as usize;
    вначале(h, off, b"PE\0\0")
}

// ───────────────────────────── команды в тексте, субтитрах, «видео» ─────────────────────────────

/// Сильные образцы команд и скриптов — целым словом (`Peek.StrongScript`).
pub const ОБРАЗЦЫ_КОМАНД: &[&str] = &[
    "powershell", "mshta", "wscript.shell", "cmd.exe /c", "cmd /c ", "<hta:application", "frombase64string",
    "invoke-expression", "rundll32", "regsvr32", "certutil -", "bitsadmin", "start-process", "downloadstring",
    "createobject(", "shell.application", "javascript:", "vbscript:", "iex(", "powershell -",
];

fn словесный_байт(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// Образец найден целым словом: буква/цифра вплотную к краю — часть другого слова
/// (суахили «akamshtaki» — не «mshta»; «powershellish» — не «powershell»). Перенос `HasWord`.
fn есть_слово(срез: &[u8], образец: &[u8]) -> bool {
    if образец.is_empty() || срез.len() < образец.len() {
        return false;
    }
    let mut от = 0usize;
    while от + образец.len() <= срез.len() {
        match срез[от..].windows(образец.len()).position(|w| w == образец) {
            None => return false,
            Some(поз) => {
                let i = от + поз;
                let конец = i + образец.len();
                let лев = !словесный_байт(образец[0]) || i == 0 || !словесный_байт(срез[i - 1]);
                let прав = !словесный_байт(образец[образец.len() - 1]) || конец >= срез.len() || !словесный_байт(срез[конец]);
                if лев && прав {
                    return true;
                }
                от = i + 1;
            }
        }
    }
    false
}

/// Первый образец команды в срезе (ASCII, без учёта регистра), иначе `None`. `срез` — уже прочитанные байты.
pub fn найти_команду(срез: &[u8]) -> Option<&'static str> {
    let низ: Vec<u8> = срез.iter().map(|b| b.to_ascii_lowercase()).collect();
    for &о in ОБРАЗЦЫ_КОМАНД {
        if есть_слово(&низ, о.as_bytes()) {
            return Some(о);
        }
    }
    None
}

// ───────────────────────────── PDF ─────────────────────────────

/// Что опасного в PDF (поиском байт, как `Peek.Pdf`; офлайн, без распаковки потоков — это ограничение описано в отчёте).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Пдф {
    /// `/Launch` — открытие запускает программу.
    pub запуск: bool,
    /// `/EmbeddedFile` — внутри вложен файл.
    pub вложение: bool,
    /// `/JavaScript`, `/JS` — есть скрипт.
    pub скрипт: bool,
    /// `/OpenAction` — действие при открытии.
    pub при_открытии: bool,
}

pub fn разобрать_pdf(данные: &[u8]) -> Пдф {
    let есть = |о: &[u8]| данные.windows(о.len()).any(|w| w == о);
    Пдф {
        запуск: есть(b"/Launch"),
        вложение: есть(b"/EmbeddedFile"),
        скрипт: есть(b"/JavaScript") || есть(b"/JS"),
        при_открытии: есть(b"/OpenAction"),
    }
}

// ───────────────────────────── архив ZIP ─────────────────────────────

/// Одна запись архива.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ЗаписьАрхива {
    pub имя: String,
    pub зашифрована: bool,
    pub сжато: u64,
    pub несжато: u64,
}

/// Что внутри архива ZIP: имена, что из них запускается, вложенные архивы, пароль, «бомба».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ВзглядВАрхив {
    pub открылся: bool,
    pub файлов: usize,
    pub зашифрован: bool,
    /// Имена запускаемых внутри (программы, скрипты…).
    pub запускаемые: Vec<String>,
    /// Вложенные архивы.
    pub вложенные: Vec<String>,
    /// Подвохи в именах внутри.
    pub подвохи: Vec<String>,
    pub несжато: u64,
    pub сжато: u64,
}

impl ВзглядВАрхив {
    /// Архив-бомба: больше 64 ГБ в распаковке или сжат в тысячи раз (`ArchiveLook.Bomb`).
    pub fn бомба(&self) -> bool {
        self.несжато > (64 << 30)
            || (self.несжато > (1 << 30) && self.сжато > 0 && self.несжато / self.сжато.max(1) > 2000)
    }
}

const МАКС_ЗАПИСЕЙ: usize = 50_000;

/// Разбор центрального каталога ZIP над полным срезом архива. Без распаковки на диск, без крейтов.
/// Поддержка ZIP64 для числа записей и смещения каталога (минимальная).
pub fn записи_zip(архив: &[u8]) -> Option<Vec<ЗаписьАрхива>> {
    // End Of Central Directory: сигнатура PK\x05\x06, ищем с конца (комментарий до 64 КБ)
    let eocd_sig = [b'P', b'K', 0x05, 0x06];
    let мин = архив.len().checked_sub(22)?;
    let нижн = мин.saturating_sub(0xFFFF);
    let mut eocd = None;
    let mut i = мин;
    loop {
        if вначале(&архив[i..], 0, &eocd_sig) {
            eocd = Some(i);
            break;
        }
        if i == нижн {
            break;
        }
        i -= 1;
    }
    let eocd = eocd?;
    let чтение16 = |о: usize| -> Option<u16> {
        архив.get(о..о + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let чтение32 = |о: usize| -> Option<u32> {
        архив.get(о..о + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let чтение64 = |о: usize| -> Option<u64> {
        архив.get(о..о + 8).map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    };
    let mut смещение_каталога = чтение32(eocd + 16)? as u64;
    let mut записей = чтение16(eocd + 10)? as u64;

    // ZIP64: если поля = 0xFFFF/0xFFFFFFFF, искать ZIP64 EOCD локатор перед EOCD
    if (записей == 0xFFFF || смещение_каталога == 0xFFFF_FFFF) && eocd >= 20 {
        let лок = eocd - 20;
        if вначале(&архив[лок..], 0, &[b'P', b'K', 0x06, 0x07]) {
            if let Some(z64) = чтение64(лок + 8) {
                let z64 = z64 as usize;
                if вначале(&архив[z64..], 0, &[b'P', b'K', 0x06, 0x06]) {
                    записей = чтение64(z64 + 32)?;
                    смещение_каталога = чтение64(z64 + 48)?;
                }
            }
        }
    }

    let mut записи = Vec::new();
    let mut о = смещение_каталога as usize;
    let хедер = [b'P', b'K', 0x01, 0x02];
    while записи.len() < записей as usize && записи.len() < МАКС_ЗАПИСЕЙ {
        if !вначале(&архив[о.min(архив.len())..], 0, &хедер) {
            break;
        }
        let флаги = чтение16(о + 8)?;
        let сжато = чтение32(о + 20)? as u64;
        let несжато = чтение32(о + 24)? as u64;
        let длина_имени = чтение16(о + 28)? as usize;
        let длина_доп = чтение16(о + 30)? as usize;
        let длина_комм = чтение16(о + 32)? as usize;
        let начало_имени = о + 46;
        let имя_байт = архив.get(начало_имени..начало_имени + длина_имени)?;
        let имя = String::from_utf8_lossy(имя_байт).to_string();
        записи.push(ЗаписьАрхива {
            имя,
            зашифрована: флаги & 1 != 0,
            сжато,
            несжато,
        });
        о = начало_имени + длина_имени + длина_доп + длина_комм;
    }
    Some(записи)
}

/// Собрать «взгляд в архив» из записей: что запускается, вложенные архивы, подвохи, пароль, размеры.
pub fn взгляд_в_архив(записи: &[ЗаписьАрхива]) -> ВзглядВАрхив {
    use super::vid;
    let mut в = ВзглядВАрхив { открылся: true, ..Default::default() };
    for з in записи {
        if з.имя.ends_with('/') || з.имя.is_empty() {
            continue;
        }
        в.файлов += 1;
        в.несжато += з.несжато;
        в.сжато += з.сжато;
        if з.зашифрована {
            в.зашифрован = true;
        }
        let имя = з.имя.rsplit(['/', '\\']).next().unwrap_or(&з.имя);
        let разбор = vid::разобрать_имя(имя);
        if разбор.опасно {
            добавить(&mut в.подвохи, format!("«{имя}»: {}", разбор.замечания.join("; ")));
        }
        let род = vid::род_по_расширению(&vid::расширение(имя));
        if род.запускается() {
            добавить(&mut в.запускаемые, имя.to_string());
        } else if род == Род::Архив {
            добавить(&mut в.вложенные, имя.to_string());
        }
    }
    в
}

fn добавить(список: &mut Vec<String>, с: String) {
    if список.len() < 12 {
        список.push(с);
    }
}

/// Документ Office с макросами по именам записей ZIP (`vbaProject.bin` или путь `/macros/`). Перенос `OfficeHasMacros`.
pub fn office_макросы(записи: &[ЗаписьАрхива]) -> bool {
    записи.iter().any(|з| {
        let имя = з.имя.rsplit(['/', '\\']).next().unwrap_or(&з.имя);
        имя.eq_ignore_ascii_case("vbaProject.bin") || з.имя.to_lowercase().contains("/macros/")
    })
}

/// Старый Office (OLE): поток `_VBA_PROJECT` (UTF-16) в первых байтах — макросы.
pub fn ole_макросы(данные: &[u8]) -> bool {
    let образец: Vec<u8> = "_VBA_PROJECT".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    данные.windows(образец.len()).any(|w| w == образец.as_slice())
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn сигнатуры() {
        assert_eq!(sniff(b"MZ\x90\x00", 4), Контейнер::Pe);
        assert_eq!(sniff(b"%PDF-1.7", 8), Контейнер::Pdf);
        assert_eq!(sniff(b"PK\x03\x04blah", 8), Контейнер::Zip);
        assert_eq!(sniff(b"Rar!\x1A\x07\x00", 7), Контейнер::Rar);
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0], 4), Контейнер::Картинка);
        assert_eq!(sniff(b"ordinary lecture text, plain ascii", 34), Контейнер::Текст);
        assert_eq!(sniff(b"", 0), Контейнер::Неизвестно);
    }

    #[test]
    fn pe_настоящий() {
        // MZ + e_lfanew=0x40 + PE\0\0
        let mut b = vec![0u8; 0x48];
        b[0] = b'M';
        b[1] = b'Z';
        b[0x3C] = 0x40;
        b[0x40..0x44].copy_from_slice(b"PE\0\0");
        assert!(pe_заголовок(&b), "настоящая программа Windows");
        // просто «MZ…» без PE — не программа (старое по одной MZ ошиблось бы)
        assert!(!pe_заголовок(b"MZ this is just text with MZ at start padded..............."));
    }

    #[test]
    fn команда_целым_словом() {
        assert_eq!(найти_команду(b"run: powershell -enc AAAA"), Some("powershell"));
        assert_eq!(найти_команду(b"iex(New-Object Net.WebClient)"), Some("iex("));
        // «akamshtaki» — суахили, не команда «mshta»: старое по «contains» дало бы ложную тревогу
        assert!(найти_команду(b"nakamshtaki wewe").is_none(), "целое слово: не часть другого");
        // «powershellish» — не «powershell»
        assert!(найти_команду(b"this is powershellish stuff").is_none());
        // регистр не важен
        assert_eq!(найти_команду(b"MSHTA http://x"), Some("mshta"));
    }

    #[test]
    fn pdf_разбор() {
        let безобиден = b"%PDF-1.5 ... /EmbeddedFile lab.csv ... endobj";
        let п = разобрать_pdf(безобиден);
        assert!(п.вложение && !п.запуск && !п.скрипт, "лекция с вложенным .csv — только вложение");
        let опасен = b"%PDF-1.5 /OpenAction /Launch calc.exe /JavaScript app.alert(1)";
        let п2 = разобрать_pdf(опасен);
        assert!(п2.запуск && п2.скрипт && п2.при_открытии);
    }

    #[test]
    fn zip_каталог_и_взгляд() {
        // один сохранённый (stored) файл «evil.exe» без сжатия
        let архив = собрать_zip(&[("evil.exe", b"MZxx", false), ("readme.txt", b"hello", false)]);
        let записи = записи_zip(&архив).expect("каталог ZIP разобран");
        assert_eq!(записи.len(), 2);
        let в = взгляд_в_архив(&записи);
        assert_eq!(в.файлов, 2);
        assert!(в.запускаемые.contains(&"evil.exe".to_string()), "программа внутри — старое тоже ловит");
        assert!(!в.зашифрован);
    }

    #[test]
    fn zip_с_паролем() {
        let архив = собрать_zip(&[("secret.exe", b"MZ", true)]);
        let записи = записи_zip(&архив).unwrap();
        let в = взгляд_в_архив(&записи);
        assert!(в.зашифрован, "бит 0 флагов — архив под паролем");
    }

    #[test]
    fn office_макросы_по_имени() {
        let с_макросом = [
            ЗаписьАрхива { имя: "word/vbaProject.bin".into(), зашифрована: false, сжато: 1, несжато: 1 },
            ЗаписьАрхива { имя: "word/document.xml".into(), зашифрована: false, сжато: 1, несжато: 1 },
        ];
        assert!(office_макросы(&с_макросом));
        let без = [ЗаписьАрхива { имя: "word/document.xml".into(), зашифрована: false, сжато: 1, несжато: 1 }];
        assert!(!office_макросы(&без), "чистый docx — без макросов");
    }

    #[test]
    fn бомба_по_отношению() {
        let мал = ВзглядВАрхив { несжато: 100, сжато: 50, ..Default::default() };
        assert!(!мал.бомба());
        let бомба = ВзглядВАрхив { несжато: 2 << 30, сжато: 1000, ..Default::default() };
        assert!(бомба.бомба(), "сжат в миллионы раз — бомба");
    }

    // ── вспомогательное: собрать минимальный ZIP (stored) для тестов ──
    fn собрать_zip(файлы: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut поток = Vec::new();
        let mut каталог = Vec::new();
        let mut смещения = Vec::new();
        for &(имя, данные, шифр) in файлы {
            смещения.push(поток.len() as u32);
            let флаги: u16 = if шифр { 1 } else { 0 };
            // локальный заголовок PK\x03\x04
            поток.extend_from_slice(b"PK\x03\x04");
            поток.extend_from_slice(&20u16.to_le_bytes()); // версия
            поток.extend_from_slice(&флаги.to_le_bytes());
            поток.extend_from_slice(&0u16.to_le_bytes()); // метод: stored
            поток.extend_from_slice(&0u32.to_le_bytes()); // время/дата
            поток.extend_from_slice(&0u32.to_le_bytes()); // crc
            поток.extend_from_slice(&(данные.len() as u32).to_le_bytes());
            поток.extend_from_slice(&(данные.len() as u32).to_le_bytes());
            поток.extend_from_slice(&(имя.len() as u16).to_le_bytes());
            поток.extend_from_slice(&0u16.to_le_bytes()); // доп
            поток.extend_from_slice(имя.as_bytes());
            поток.extend_from_slice(данные);
        }
        let начало_каталога = поток.len() as u32;
        for (&(имя, данные, шифр), &см) in файлы.iter().zip(&смещения) {
            let флаги: u16 = if шифр { 1 } else { 0 };
            каталог.extend_from_slice(b"PK\x01\x02");
            каталог.extend_from_slice(&20u16.to_le_bytes()); // версия-создателя
            каталог.extend_from_slice(&20u16.to_le_bytes()); // версия-нужна
            каталог.extend_from_slice(&флаги.to_le_bytes());
            каталог.extend_from_slice(&0u16.to_le_bytes()); // метод
            каталог.extend_from_slice(&0u32.to_le_bytes()); // время/дата
            каталог.extend_from_slice(&0u32.to_le_bytes()); // crc
            каталог.extend_from_slice(&(данные.len() as u32).to_le_bytes());
            каталог.extend_from_slice(&(данные.len() as u32).to_le_bytes());
            каталог.extend_from_slice(&(имя.len() as u16).to_le_bytes());
            каталог.extend_from_slice(&0u16.to_le_bytes()); // доп
            каталог.extend_from_slice(&0u16.to_le_bytes()); // комментарий
            каталог.extend_from_slice(&0u16.to_le_bytes()); // диск
            каталог.extend_from_slice(&0u16.to_le_bytes()); // внутр. атрибуты
            каталог.extend_from_slice(&0u32.to_le_bytes()); // внешн. атрибуты
            каталог.extend_from_slice(&см.to_le_bytes());
            каталог.extend_from_slice(имя.as_bytes());
        }
        let размер_каталога = каталог.len() as u32;
        поток.extend_from_slice(&каталог);
        // EOCD
        поток.extend_from_slice(b"PK\x05\x06");
        поток.extend_from_slice(&0u16.to_le_bytes()); // номер диска
        поток.extend_from_slice(&0u16.to_le_bytes()); // диск каталога
        поток.extend_from_slice(&(файлы.len() as u16).to_le_bytes());
        поток.extend_from_slice(&(файлы.len() as u16).to_le_bytes());
        поток.extend_from_slice(&размер_каталога.to_le_bytes());
        поток.extend_from_slice(&начало_каталога.to_le_bytes());
        поток.extend_from_slice(&0u16.to_le_bytes()); // длина комментария
        поток
    }
}
