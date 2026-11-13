//! Защитник Windows местно (`MpCmdRun -Scan -ScanType 3 -File … -DisableRemediation`) и AMSI — перенос
//! `Defender.cs`, `Amsi.cs`. БЕЗ СЕТИ и БЕЗ обновления баз: `-SignatureUpdate` не зовём (Windows обновляет сама),
//! Team Cymru и freshclam убраны. Проверка идёт ниже его работы (приоритет «простой»).

use std::ffi::c_void;
use std::path::{Path, PathBuf};

/// Итог проверки Защитником.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Итог {
    Чисто,
    Угроза(Vec<String>),
    /// Не смог проверить (нет MpCmdRun, нет файла, не уложился) — повторить позже, НЕ «придержать».
    НеСмог(String),
}

/// Путь к `MpCmdRun.exe`: сперва `%ProgramFiles%\Windows Defender`, иначе самая свежая версия из `Platform`.
pub fn mpcmdrun() -> Option<PathBuf> {
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        let п = PathBuf::from(pf).join("Windows Defender").join("MpCmdRun.exe");
        if п.exists() {
            return Some(п);
        }
    }
    let база = std::env::var_os("ProgramData")?;
    let платформа = PathBuf::from(база).join("Microsoft").join("Windows Defender").join("Platform");
    let mut версии: Vec<PathBuf> = std::fs::read_dir(&платформа)
        .ok()?
        .filter_map(Result::ok)
        .map(|д| д.path())
        .filter(|п| п.is_dir())
        .collect();
    версии.sort();
    версии.into_iter().rev().map(|д| д.join("MpCmdRun.exe")).find(|п| п.exists())
}

/// Чистая (видно в тесте): разобрать вывод `MpCmdRun` и код выхода в итог.
pub fn разобрать_вывод(код: i32, вывод: &str) -> Итог {
    let угрозы: Vec<String> = вывод
        .lines()
        .filter_map(|с| {
            let с = с.trim();
            с.strip_prefix("Threat").and_then(|х| х.trim_start().strip_prefix(':')).map(|х| х.trim().to_string())
        })
        .filter(|с| !с.is_empty())
        .collect();
    let уникальные: Vec<String> = {
        let mut виды = Vec::new();
        for у in угрозы {
            if !виды.contains(&у) {
                виды.push(у);
            }
        }
        виды
    };
    match код {
        0 if уникальные.is_empty() => Итог::Чисто,
        2 => Итог::Угроза(if уникальные.is_empty() { vec!["угроза".into()] } else { уникальные }),
        _ if !уникальные.is_empty() => Итог::Угроза(уникальные),
        _ => Итог::НеСмог(format!("Защитник не смог проверить (код 0x{код:08X})")),
    }
}

/// Проверить файл Защитником. `mpcmdrun` — путь (чтобы в тестах не искать заново).
pub fn проверить(exe: &Path, файл: &Path) -> Итог {
    use std::os::windows::process::CommandExt;
    if !файл.exists() {
        return Итог::НеСмог("файла нет на месте".into());
    }
    let Ok(полный) = std::fs::canonicalize(файл) else {
        return Итог::НеСмог("путь не разобрать".into());
    };
    // путь без префикса \\?\ — MpCmdRun его не любит
    let строка_пути = полный.to_string_lossy();
    let чистый = строка_пути.strip_prefix(r"\\?\").unwrap_or(&строка_пути);
    // CREATE_NO_WINDOW | IDLE_PRIORITY_CLASS — ниже его работы
    let вывод = std::process::Command::new(exe)
        .args(["-Scan", "-ScanType", "3", "-File", чистый, "-DisableRemediation"])
        .creation_flags(0x0800_0000 | 0x0000_0040)
        .output();
    match вывод {
        Ok(в) => разобрать_вывод(в.status.code().unwrap_or(-1), &String::from_utf8_lossy(&в.stdout)),
        Err(e) => Итог::НеСмог(format!("MpCmdRun не запустился: {e}")),
    }
}

// ───────────────────────────── AMSI ─────────────────────────────

#[link(name = "amsi")]
extern "system" {
    fn AmsiInitialize(имя: *const u16, контекст: *mut isize) -> i32;
    fn AmsiUninitialize(контекст: isize);
    fn AmsiOpenSession(контекст: isize, сессия: *mut isize) -> i32;
    fn AmsiCloseSession(контекст: isize, сессия: isize);
    fn AmsiScanBuffer(контекст: isize, буфер: *const c_void, длина: u32, имя: *const u16, сессия: isize, итог: *mut i32) -> i32;
}

/// Ответ AMSI ≥ этого — вредоносно (`AMSI_RESULT_DETECTED`).
pub const AMSI_ОБНАРУЖЕНО: i32 = 32768;

/// Итог проверки AMSI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ОтветAMSI {
    pub проверено: bool,
    pub вредоносно: bool,
    pub код: i32,
}

fn широкая(с: &str) -> Vec<u16> {
    с.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Проверить текст (субтитры, .txt, страницы, скрипты) до 5 МБ через AMSI — тем антивирусом, что стоит (Защитником),
/// с «пониманием» запутанного кода. Работает без прав администратора.
pub fn проверить_amsi(данные: &[u8], имя: &str) -> ОтветAMSI {
    if данные.len() > (5 << 20) {
        return ОтветAMSI::default();
    }
    let приложение = широкая("Mekhanik");
    let mut контекст: isize = 0;
    if unsafe { AmsiInitialize(приложение.as_ptr(), &mut контекст) } != 0 {
        return ОтветAMSI::default();
    }
    let mut сессия: isize = 0;
    let ответ = if unsafe { AmsiOpenSession(контекст, &mut сессия) } == 0 {
        let имя_ш = широкая(имя);
        let mut код = 0i32;
        let ок = unsafe {
            AmsiScanBuffer(контекст, данные.as_ptr() as *const c_void, данные.len() as u32, имя_ш.as_ptr(), сессия, &mut код)
        } == 0;
        unsafe { AmsiCloseSession(контекст, сессия) };
        if ок {
            ОтветAMSI { проверено: true, вредоносно: код >= AMSI_ОБНАРУЖЕНО, код }
        } else {
            ОтветAMSI::default()
        }
    } else {
        ОтветAMSI::default()
    };
    unsafe { AmsiUninitialize(контекст) };
    ответ
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn вывод_чисто() {
        assert_eq!(разобрать_вывод(0, "Scan starting...\nScan finished.\n"), Итог::Чисто);
    }

    #[test]
    fn вывод_угроза() {
        let в = "Scan starting...\nThreat                  : Virus:DOS/EICAR_Test_File\nThreat : Virus:DOS/EICAR_Test_File\n";
        match разобрать_вывод(2, в) {
            Итог::Угроза(у) => assert_eq!(у, vec!["Virus:DOS/EICAR_Test_File".to_string()], "повтор свёрнут"),
            иное => panic!("ожидали угрозу, вышло {иное:?}"),
        }
    }

    #[test]
    fn вывод_не_смог() {
        // ненулевой код без угроз — не «придержать», а «не смог» (повторить позже)
        assert!(matches!(разобрать_вывод(2u32.wrapping_neg() as i32, "Scan error"), Итог::НеСмог(_)));
    }

    /// Живая проба (только чтение, безвредный синтетический текст): AMSI доступен и чистый текст не ругает.
    #[test]
    #[ignore]
    fn живьём_amsi_чистый_текст() {
        let о = проверить_amsi(b"This is an ordinary lecture note about statistics.", "note.txt");
        assert!(о.проверено, "AMSI отвечает");
        assert!(!о.вредоносно, "обычный текст — не вредонос");
    }
}
