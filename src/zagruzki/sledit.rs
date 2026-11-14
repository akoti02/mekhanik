//! Слежка за папками — `ReadDirectoryChangesW` своими объявлениями (как всё в `okna.rs`), без крейтов.
//! Один поток на корень: ждёт события, отдаёт новые и изменённые файлы оркестратору. Переполнение буфера —
//! сигнал «обойти заново». «Докачан ли файл» — опрос размера (пропускаем `.crdownload`, `.part`, `.tmp`).

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(имя: *const u16, доступ: u32, общий: u32, безопасность: *const c_void, создание: u32, флаги: u32, шаблон: isize) -> isize;
    fn ReadDirectoryChangesW(каталог: isize, буфер: *mut c_void, длина: u32, подпапки: i32, фильтр: u32, вернул: *mut u32, перекрытие: *mut c_void, завершение: *const c_void) -> i32;
    fn CloseHandle(h: isize) -> i32;
}

fn широкая(с: &str) -> Vec<u16> {
    с.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Что случилось с файлом.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Событие {
    /// Новый или изменённый файл — нужно рассмотреть.
    Файл(PathBuf),
    /// Буфер переполнился — обойти корень заново.
    ОбойтиЗаново(PathBuf),
}

const FILE_LIST_DIRECTORY: u32 = 1;
const FILE_SHARE_READ_WRITE_DELETE: u32 = 1 | 2 | 4;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILTER: u32 = 0x1 | 0x8 | 0x10; // FILE_NAME | SIZE | LAST_WRITE
const ADDED: u32 = 1;
const MODIFIED: u32 = 3;
const RENAMED_NEW: u32 = 5;

/// Чистая (видно в тесте): разобрать буфер `FILE_NOTIFY_INFORMATION` в список (действие, имя).
pub fn разобрать_события(буфер: &[u8], байт: usize) -> Vec<(u32, String)> {
    let mut итог = Vec::new();
    let mut поз = 0usize;
    while поз + 12 <= байт && поз + 12 <= буфер.len() {
        let след = u32::from_le_bytes(буфер[поз..поз + 4].try_into().unwrap()) as usize;
        let действие = u32::from_le_bytes(буфер[поз + 4..поз + 8].try_into().unwrap());
        let длина_имени = u32::from_le_bytes(буфер[поз + 8..поз + 12].try_into().unwrap()) as usize;
        let начало = поз + 12;
        if начало + длина_имени <= буфер.len() {
            let ширь: Vec<u16> = буфер[начало..начало + длина_имени]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            итог.push((действие, String::from_utf16_lossy(&ширь)));
        }
        if след == 0 {
            break;
        }
        поз += след;
    }
    итог
}

/// Докачан ли файл: размер и время изменения не меняются `тихо`; `.crdownload`/`.part`/`.tmp` — ещё качается.
/// `предел` — сколько ждать всего. Возвращает `true`, когда файл готов.
pub fn ждать_докачки(путь: &Path, тихо: Duration, предел: Duration) -> bool {
    use super::vid;
    let начало = Instant::now();
    let mut прежний: Option<(u64, std::time::SystemTime)> = None;
    let mut устойчиво = Instant::now();
    let mut давний = false;
    loop {
        // 🟥 14.11 (аудит): просят выйти — не держать выход до 10 мин ради одного файла
        if начало.elapsed() > предел || crate::svyaz::просят_выйти() {
            return false;
        }
        if let Some(имя) = путь.file_name().and_then(|и| и.to_str()) {
            if vid::недокачан(имя) {
                // временное имя — браузер ещё пишет; ждём переименования (придёт отдельным событием)
                return false;
            }
        }
        match std::fs::metadata(путь) {
            Err(_) => return false, // пропал посреди ожидания
            Ok(м) => {
                let текущий = (м.len(), м.modified().unwrap_or(std::time::UNIX_EPOCH));
                // 🟥 14.11 (аудит): файл не менялся дольше `тихо` ещё до первого взгляда (лежал до запуска) — хватит одного
                // повторного взгляда через 0,4 с, что размер тот же (время изменения у пишущегося файла может обновиться
                // лишь при закрытии); раньше каждый такой ждал 2 с, один за другим, и догонялка при старте шла N×2 с
                if прежний.is_none() {
                    давний = текущий.1.elapsed().is_ok_and(|д| д >= тихо + Duration::from_secs(1));
                }
                if Some(текущий) == прежний {
                    if давний || устойчиво.elapsed() >= тихо {
                        return true;
                    }
                } else {
                    прежний = Some(текущий);
                    устойчиво = Instant::now();
                }
            }
        }
        std::thread::sleep(Duration::from_millis(400));
    }
}

/// Запустить слежку за корнями; поток на корень. Новые файлы и сигналы «обойти заново» идут в `передатчик`.
/// `стоп` — опустить, чтобы потоки завершились при следующем событии.
pub fn следить(корни: Vec<PathBuf>, передатчик: Sender<Событие>, стоп: Arc<AtomicBool>) {
    for корень in корни {
        let пер = передатчик.clone();
        let ст = стоп.clone();
        let _ = std::thread::Builder::new()
            .name("zagruzki-sledit".to_string())
            .spawn(move || следить_за(корень, пер, ст));
    }
}

fn следить_за(корень: PathBuf, передатчик: Sender<Событие>, стоп: Arc<AtomicBool>) {
    while !стоп.load(Ordering::Relaxed) {
        let имя = широкая(&корень.to_string_lossy());
        let каталог = unsafe {
            CreateFileW(
                имя.as_ptr(),
                FILE_LIST_DIRECTORY,
                FILE_SHARE_READ_WRITE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                0,
            )
        };
        if каталог == -1 || каталог == 0 {
            // корень недоступен — подождать и попробовать снова (его правило: ошибка корня — пересоздать через 30 с)
            std::thread::sleep(Duration::from_secs(30));
            continue;
        }
        let mut буфер = vec![0u8; 64 * 1024];
        while !стоп.load(Ordering::Relaxed) {
            let mut вернул = 0u32;
            let ок = unsafe {
                ReadDirectoryChangesW(
                    каталог,
                    буфер.as_mut_ptr() as *mut c_void,
                    буфер.len() as u32,
                    1,
                    FILTER,
                    &mut вернул,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            if ок == 0 {
                break; // ошибка — переоткрыть каталог
            }
            if вернул == 0 {
                // буфер переполнился — слишком много изменений: обойти корень заново
                let _ = передатчик.send(Событие::ОбойтиЗаново(корень.clone()));
                continue;
            }
            for (действие, имя_файла) in разобрать_события(&буфер, вернул as usize) {
                if действие != ADDED && действие != MODIFIED && действие != RENAMED_NEW {
                    continue;
                }
                let путь = корень.join(имя_файла.replace('\\', "/"));
                if super::papki::исключён(&путь) {
                    continue;
                }
                let _ = передатчик.send(Событие::Файл(путь));
            }
        }
        unsafe { CloseHandle(каталог) };
    }
}

#[cfg(test)]
mod тесты {
    use super::*;

    /// 🟥 14.11 (аудит): файл, что лежит давно (не менялся час), — готов после одного повторного взгляда (≈0,4 с), а не
    /// после полного «тихо» (2 с на файл, один за другим — догонялка при старте шла N×2 с).
    #[test]
    fn давний_файл_без_долгого_ожидания() {
        let врем = std::env::temp_dir().join(format!("mekh-sledit-davniy-{}", std::process::id()));
        std::fs::create_dir_all(&врем).unwrap();
        let файл = врем.join("давно.bin");
        std::fs::write(&файл, b"old bytes").unwrap();
        std::fs::File::options().write(true).open(&файл).unwrap()
            .set_modified(std::time::SystemTime::now() - Duration::from_secs(3600)).unwrap();
        let т = Instant::now();
        assert!(ждать_докачки(&файл, Duration::from_secs(2), Duration::from_secs(10)));
        assert!(т.elapsed() < Duration::from_millis(1500), "давний файл ждали {:?}", т.elapsed());
        let _ = std::fs::remove_dir_all(&врем);
    }

    #[test]
    fn разбор_одного_события() {
        // одна запись: NextEntryOffset=0, Action=ADDED, имя «а.exe»
        let имя: Vec<u16> = "а.exe".encode_utf16().collect();
        let длина = (имя.len() * 2) as u32;
        let mut буф = Vec::new();
        буф.extend_from_slice(&0u32.to_le_bytes());
        буф.extend_from_slice(&ADDED.to_le_bytes());
        буф.extend_from_slice(&длина.to_le_bytes());
        for w in &имя {
            буф.extend_from_slice(&w.to_le_bytes());
        }
        let всего = буф.len();
        let события = разобрать_события(&буф, всего);
        assert_eq!(события, vec![(ADDED, "а.exe".to_string())]);
    }

    #[test]
    fn разбор_двух_событий_по_цепочке() {
        // первая запись с NextEntryOffset до второй
        let сделать = |action: u32, имя: &str, next: u32| {
            let ш: Vec<u16> = имя.encode_utf16().collect();
            let mut b = Vec::new();
            b.extend_from_slice(&next.to_le_bytes());
            b.extend_from_slice(&action.to_le_bytes());
            b.extend_from_slice(&((ш.len() * 2) as u32).to_le_bytes());
            for w in &ш {
                b.extend_from_slice(&w.to_le_bytes());
            }
            while b.len() % 4 != 0 {
                b.push(0);
            }
            b
        };
        let mut первая = сделать(ADDED, "один.pdf", 0);
        let длина_первой = первая.len() as u32;
        первая[0..4].copy_from_slice(&длина_первой.to_le_bytes());
        let вторая = сделать(MODIFIED, "два.zip", 0);
        let mut буф = первая;
        буф.extend_from_slice(&вторая);
        let всего = буф.len();
        let события = разобрать_события(&буф, всего);
        assert_eq!(события.len(), 2);
        assert_eq!(события[1], (MODIFIED, "два.zip".to_string()));
    }

    /// Живая проба на ВРЕМЕННОЙ папке: дописываемый файл признаётся готовым, когда размер устоялся.
    #[test]
    #[ignore]
    fn живьём_ждать_докачки() {
        let врем = std::env::temp_dir().join(format!("mekh-sledit-{}", crate::svyaz::сейчас_мс()));
        std::fs::create_dir_all(&врем).unwrap();
        let файл = врем.join("готов.bin");
        std::fs::write(&файл, b"ready bytes").unwrap();
        assert!(ждать_докачки(&файл, Duration::from_millis(500), Duration::from_secs(5)));
        let _ = std::fs::remove_dir_all(&врем);
    }
}
