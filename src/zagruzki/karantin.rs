//! Карантин — ПЕРЕНОС файла (не удаление) в `%LOCALAPPDATA%\mekhanik\karantin\<id>\`, с записью: откуда,
//! отпечаток, причина, вид, метка «из интернета». Возврат `вернуть(id)` — на прежнее место.
//! Насовсем «Механик» не удаляет ничего: только он сам, своей рукой (есть «в корзину»).
//! Отпечаток файла — SHA-256 своими объявлениями BCrypt (без крейтов, без сети). Память отпечатков — один раз на файл.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use super::vid::Род;

// ───────────────────────────── SHA-256 (BCrypt) ─────────────────────────────

#[link(name = "bcrypt")]
extern "system" {
    fn BCryptOpenAlgorithmProvider(алг: *mut isize, ид: *const u16, реализация: *const u16, флаги: u32) -> i32;
    fn BCryptCloseAlgorithmProvider(алг: isize, флаги: u32) -> i32;
    fn BCryptCreateHash(алг: isize, хэш: *mut isize, объект: *mut u8, объём: u32, ключ: *const u8, ключдлина: u32, флаги: u32) -> i32;
    fn BCryptHashData(хэш: isize, вход: *const u8, длина: u32, флаги: u32) -> i32;
    fn BCryptFinishHash(хэш: isize, итог: *mut u8, длина: u32, флаги: u32) -> i32;
    fn BCryptDestroyHash(хэш: isize) -> i32;
}

fn широкая(с: &str) -> Vec<u16> {
    с.encode_utf16().chain(std::iter::once(0)).collect()
}

/// SHA-256 среза байт в hex (нижний регистр). `None` — BCrypt не дал.
#[allow(dead_code)] // сведение: проверяется в тестах; карантин берёт отпечаток файла целиком
pub fn отпечаток_байт(данные: &[u8]) -> Option<String> {
    let имя = широкая("SHA256");
    let mut алг: isize = 0;
    if unsafe { BCryptOpenAlgorithmProvider(&mut алг, имя.as_ptr(), std::ptr::null(), 0) } != 0 {
        return None;
    }
    let mut хэш: isize = 0;
    let ок = unsafe { BCryptCreateHash(алг, &mut хэш, std::ptr::null_mut(), 0, std::ptr::null(), 0, 0) } == 0;
    let итог = if ок {
        unsafe { BCryptHashData(хэш, данные.as_ptr(), данные.len() as u32, 0) };
        let mut вых = [0u8; 32];
        let г = unsafe { BCryptFinishHash(хэш, вых.as_mut_ptr(), 32, 0) } == 0;
        unsafe { BCryptDestroyHash(хэш) };
        г.then(|| в_hex(&вых))
    } else {
        None
    };
    unsafe { BCryptCloseAlgorithmProvider(алг, 0) };
    итог
}

/// SHA-256 файла (читаем кусками, в hex). `None` — файл не открылся.
pub fn отпечаток_файла(путь: &Path) -> Option<String> {
    use std::io::Read;
    let имя = широкая("SHA256");
    let mut алг: isize = 0;
    if unsafe { BCryptOpenAlgorithmProvider(&mut алг, имя.as_ptr(), std::ptr::null(), 0) } != 0 {
        return None;
    }
    let mut хэш: isize = 0;
    if unsafe { BCryptCreateHash(алг, &mut хэш, std::ptr::null_mut(), 0, std::ptr::null(), 0, 0) } != 0 {
        unsafe { BCryptCloseAlgorithmProvider(алг, 0) };
        return None;
    }
    let итог = (|| {
        let mut ф = std::fs::File::open(путь).ok()?;
        let mut буф = vec![0u8; 1 << 20];
        loop {
            let n = ф.read(&mut буф).ok()?;
            if n == 0 {
                break;
            }
            if unsafe { BCryptHashData(хэш, буф.as_ptr(), n as u32, 0) } != 0 {
                return None;
            }
        }
        let mut вых = [0u8; 32];
        (unsafe { BCryptFinishHash(хэш, вых.as_mut_ptr(), 32, 0) } == 0).then(|| в_hex(&вых))
    })();
    unsafe { BCryptDestroyHash(хэш) };
    unsafe { BCryptCloseAlgorithmProvider(алг, 0) };
    итог
}

/// Чистая (видно в тесте): байты в hex-строку нижнего регистра.
pub fn в_hex(байты: &[u8]) -> String {
    let mut с = String::with_capacity(байты.len() * 2);
    for &b in байты {
        с.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        с.push(char::from_digit((b & 0xF) as u32, 16).unwrap());
    }
    с
}

// ───────────────────────────── папки ─────────────────────────────

/// `%LOCALAPPDATA%\mekhanik\karantin`.
pub fn папка_карантина() -> PathBuf {
    crate::svyaz::папка().join("karantin")
}

/// `%LOCALAPPDATA%\mekhanik\zagruzki` — данные сторожа (отпечатки, история, состояние).
pub fn папка_данных() -> PathBuf {
    crate::svyaz::папка().join("zagruzki")
}

// ───────────────────────────── память отпечатков ─────────────────────────────

/// `otpechatki.json`: что уже проверено чистым и что он вернул — больше не трогать (один раз на файл).
#[derive(Clone, Debug, Default)]
pub struct Отпечатки {
    pub чистые: std::collections::HashSet<String>,
    pub возвращённые: std::collections::HashSet<String>,
}

impl Отпечатки {
    pub fn путь() -> PathBuf {
        папка_данных().join("otpechatki.json")
    }

    pub fn загрузить() -> Отпечатки {
        let Ok(т) = std::fs::read_to_string(Self::путь()) else { return Отпечатки::default() };
        let v: Value = serde_json::from_str(&т).unwrap_or_default();
        let набор = |к: &str| {
            v.get(к)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect::<std::collections::HashSet<_>>()
        };
        Отпечатки { чистые: набор("chistye"), возвращённые: набор("vozvrashchennye") }
    }

    /// Файл с таким отпечатком уже решён — проверять заново не нужно.
    pub fn знаем(&self, отпечаток: &str) -> bool {
        self.чистые.contains(отпечаток) || self.возвращённые.contains(отпечаток)
    }

    pub fn сохранить(&self) -> std::io::Result<()> {
        let _ = std::fs::create_dir_all(папка_данных());
        // 🟥 чтение-правка-запись: сторож и возврат пишут этот файл из разных потоков. Раньше запись клала оба поля из
        // своей памяти — поток без возвращённых затирал запись, которую сделал возврат, и возвращённый файл тут же
        // придерживался снова. Сливаем с тем, что уже на диске, чтобы не затирать чужое.
        let на_диске = Отпечатки::загрузить();
        let слить = |своё: &std::collections::HashSet<String>, диск: &std::collections::HashSet<String>| {
            своё.union(диск).take(20_000).cloned().collect::<Vec<_>>()
        };
        let тело = json!({
            "chistye": слить(&self.чистые, &на_диске.чистые),
            "vozvrashchennye": слить(&self.возвращённые, &на_диске.возвращённые),
        });
        crate::svyaz::записать(&Self::путь(), &тело.to_string())
    }
}

// ───────────────────────────── перенос и возврат ─────────────────────────────

/// Что записываем рядом с задержанным файлом (`zapis.json`).
#[derive(Clone, Debug)]
pub struct Запись {
    pub id: String,
    pub откуда: PathBuf,
    pub имя: String,
    pub отпечаток: Option<String>,
    pub вид: Род,
    pub причина: String,
    /// Метка «из интернета» целиком (сайт), если была.
    pub сайт: Option<String>,
    pub когда_мс: u64,
}

/// Новый идентификатор карантина: время + счётчик (папки не сталкиваются).
fn новый_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static Н: AtomicU64 = AtomicU64::new(0);
    format!("{}-{}", crate::svyaz::сейчас_мс(), Н.fetch_add(1, Ordering::Relaxed))
}

/// Перенести файл в карантин. Возвращает `id` папки карантина. Оригинал исчезает из «Загрузок».
pub fn в_карантин(путь: &Path, вид: Род, причина: &str, сайт: Option<String>, отпечаток: Option<String>) -> std::io::Result<String> {
    let id = новый_id();
    let папка = папка_карантина().join(&id);
    std::fs::create_dir_all(&папка)?;
    let имя = путь.file_name().and_then(|и| и.to_str()).unwrap_or("file").to_string();
    let куда = папка.join(format!("{имя}.zaderzhano"));
    перенести(путь, &куда)?;
    let запись = Запись {
        id: id.clone(),
        откуда: путь.to_path_buf(),
        имя,
        отпечаток,
        вид,
        причина: причина.to_string(),
        сайт,
        когда_мс: crate::svyaz::сейчас_мс(),
    };
    записать_запись(&папка, &запись)?;
    Ok(id)
}

fn записать_запись(папка: &Path, з: &Запись) -> std::io::Result<()> {
    let тело = json!({
        "id": з.id,
        "otkuda": з.откуда.to_string_lossy(),
        "imya": з.имя,
        "otpechatok": з.отпечаток,
        "vid": з.вид.словами(),
        "prichina": з.причина,
        "sayt": з.сайт,
        "kogda_ms": з.когда_мс,
    });
    crate::svyaz::записать(&папка.join("zapis.json"), &serde_json::to_string_pretty(&тело).unwrap_or_else(|_| тело.to_string()))?;
    // и словами — его открывает «Посмотреть»
    let словами = format!(
        "Механик придержал файл.\r\n\r\nИмя: {}\r\nОткуда: {}\r\nПричина: {}\r\nСайт: {}\r\nОтпечаток: {}\r\n",
        з.имя,
        з.откуда.display(),
        з.причина,
        з.сайт.as_deref().unwrap_or("—"),
        з.отпечаток.as_deref().unwrap_or("—"),
    );
    crate::svyaz::записать(&папка.join("zapis.txt"), &словами)
}

/// Прочитать запись карантина по id.
pub fn прочитать_запись(id: &str) -> Option<Запись> {
    let папка = папка_карантина().join(id);
    let т = std::fs::read_to_string(папка.join("zapis.json")).ok()?;
    let v: Value = serde_json::from_str(&т).ok()?;
    Some(Запись {
        id: id.to_string(),
        откуда: PathBuf::from(v.get("otkuda")?.as_str()?),
        имя: v.get("imya")?.as_str()?.to_string(),
        отпечаток: v.get("otpechatok").and_then(Value::as_str).map(str::to_string),
        вид: Род::Прочее, // слово обратно в род не раскладываем — для возврата не нужно
        причина: v.get("prichina").and_then(Value::as_str).unwrap_or("").to_string(),
        сайт: v.get("sayt").and_then(Value::as_str).map(str::to_string),
        когда_мс: v.get("kogda_ms").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// Вернуть файл из карантина на прежнее место. Занято — «имя (2)». Отпечаток — в «возвращённые».
pub fn вернуть(id: &str, отпечатки: &mut Отпечатки) -> std::io::Result<PathBuf> {
    let запись = прочитать_запись(id)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "нет записи карантина"))?;
    let источник = папка_карантина().join(id).join(format!("{}.zaderzhano", запись.имя));
    let цель = свободное_имя(&запись.откуда);
    if let Some(р) = цель.parent() {
        let _ = std::fs::create_dir_all(р);
    }
    перенести(&источник, &цель)?;
    if let Some(о) = запись.отпечаток {
        отпечатки.возвращённые.insert(о);
        let _ = отпечатки.сохранить();
    }
    // пустую папку карантина убрать
    let _ = std::fs::remove_file(папка_карантина().join(id).join("zapis.json"));
    let _ = std::fs::remove_file(папка_карантина().join(id).join("zapis.txt"));
    let _ = std::fs::remove_dir(папка_карантина().join(id));
    Ok(цель)
}

/// Свободное имя: занято — «имя (2).ext», «имя (3).ext»…
pub fn свободное_имя(желаемое: &Path) -> PathBuf {
    if !желаемое.exists() {
        return желаемое.to_path_buf();
    }
    let основа = желаемое.file_stem().and_then(|с| с.to_str()).unwrap_or("file");
    let расш = желаемое.extension().and_then(|с| с.to_str()).map(|с| format!(".{с}")).unwrap_or_default();
    let папка = желаемое.parent().unwrap_or_else(|| Path::new("."));
    for n in 2..10_000 {
        let п = папка.join(format!("{основа} ({n}){расш}"));
        if !п.exists() {
            return п;
        }
    }
    желаемое.to_path_buf()
}

/// Перенос на том же диске — мгновенный (`rename`); на другой диск — копия и удаление после сверки отпечатка.
fn перенести(из: &Path, в: &Path) -> std::io::Result<()> {
    if std::fs::rename(из, в).is_ok() {
        return Ok(());
    }
    // другой диск: копия, сверка отпечатка, потом удаление оригинала
    std::fs::copy(из, в)?;
    let (а, б) = (отпечаток_файла(из), отпечаток_файла(в));
    if а.is_some() && а != б {
        let _ = std::fs::remove_file(в);
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "копия не сошлась с оригиналом"));
    }
    std::fs::remove_file(из)
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn hex_верный() {
        assert_eq!(в_hex(&[0x00, 0x0f, 0xa0, 0xff]), "000fa0ff");
        assert_eq!(в_hex(&[]), "");
    }

    #[test]
    fn отпечаток_известной_строки() {
        // SHA-256("abc") — проверка BCrypt
        let о = отпечаток_байт(b"abc").expect("BCrypt SHA-256 доступен");
        assert_eq!(о, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn память_знает_отпечаток() {
        let mut о = Отпечатки::default();
        о.возвращённые.insert("deadbeef".into());
        о.чистые.insert("c0ffee".into());
        assert!(о.знаем("deadbeef") && о.знаем("c0ffee"));
        assert!(!о.знаем("ничего"));
    }

    /// 🟥 Красно на старом: сохранение чистого (память сторожа без возвращённых) НЕ должно стирать с диска запись,
    /// что сделал возврат из другого потока. Старая `сохранить` клала `vozvrashchennye:[]` и затирала — файл после
    /// «Вернуть» снова придерживался.
    #[test]
    fn сохранить_сливает_не_затирая() {
        let путь = Отпечатки::путь();
        let _ = std::fs::create_dir_all(папка_данных());
        let _ = std::fs::remove_file(&путь);
        // возврат (один поток) записал возвращённый отпечаток
        let mut возврат = Отпечатки::default();
        возврат.возвращённые.insert("ret-xyz".into());
        возврат.сохранить().unwrap();
        // сторож (другой поток, своя память без возвращённых) сохраняет чистый
        let mut сторож = Отпечатки::default();
        сторож.чистые.insert("cln-abc".into());
        сторож.сохранить().unwrap();
        let итог = Отпечатки::загрузить();
        assert!(итог.возвращённые.contains("ret-xyz"), "возврат не затёрт сохранением чистого");
        assert!(итог.чистые.contains("cln-abc"), "чистый на месте");
        let _ = std::fs::remove_file(&путь);
    }

    #[test]
    fn свободное_имя_при_занятом() {
        let врем = std::env::temp_dir().join(format!("mekh-test-{}", crate::svyaz::сейчас_мс()));
        std::fs::create_dir_all(&врем).unwrap();
        let занят = врем.join("файл.txt");
        std::fs::write(&занят, b"x").unwrap();
        let своб = свободное_имя(&занят);
        assert_eq!(своб.file_name().unwrap().to_str().unwrap(), "файл (2).txt");
        let _ = std::fs::remove_dir_all(&врем);
    }

    /// Живая проба на ВРЕМЕННОЙ папке (не на его «Загрузках»): перенос в карантин и возврат.
    #[test]
    #[ignore]
    fn живьём_карантин_и_возврат() {
        let врем = std::env::temp_dir().join(format!("mekh-karantin-{}", crate::svyaz::сейчас_мс()));
        std::fs::create_dir_all(&врем).unwrap();
        let файл = врем.join("проба.bin");
        std::fs::write(&файл, b"harmless bytes").unwrap();
        let отпеч = отпечаток_файла(&файл);
        let id = в_карантин(&файл, Род::Программа, "проба", None, отпеч).expect("в карантин");
        assert!(!файл.exists(), "оригинал перенесён");
        assert!(папка_карантина().join(&id).join("проба.bin.zaderzhano").exists());
        let mut о = Отпечатки::default();
        let назад = вернуть(&id, &mut о).expect("возврат");
        assert!(назад.exists(), "файл вернулся на место");
        assert_eq!(std::fs::read(&назад).unwrap(), b"harmless bytes");
        let _ = std::fs::remove_dir_all(&врем);
    }
}
