//! Подпись программы (Authenticode) — тем же механизмом, что у Windows (`WinVerifyTrust`), перенос `Authenticode.cs`.
//! БЕЗ СЕТИ: отзыв сертификатов не проверяем (`WTD_REVOKE_NONE`, `WTD_CACHE_ONLY_URL_RETRIEVAL`) — это поход в
//! интернет на каждый файл, а «Механик» полностью офлайн; отозванное и так знает Защитник.

use std::ffi::c_void;
use std::path::Path;

#[repr(C)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[repr(C)]
struct ФайлИнфо {
    размер: u32,
    путь: *const u16,
    файл: *const c_void,
    субъект: *const c_void,
}

#[repr(C)]
struct Данные {
    размер: u32,
    политика: *const c_void,
    sip: *const c_void,
    вид_окна: u32,
    отзыв: u32,
    выбор_объекта: u32,
    файл: *const ФайлИнфо,
    действие: u32,
    состояние: *const c_void,
    url: *const u16,
    флаги: u32,
    контекст_окна: u32,
    настройки_подписи: *const c_void,
}

#[link(name = "wintrust")]
extern "system" {
    fn WinVerifyTrust(окно: isize, действие: *const Guid, данные: *mut Данные) -> i32;
}

/// Итог проверки подписи.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Подпись {
    /// Подпись есть и сходится.
    Есть,
    /// Подписи издателя нет.
    Нет,
    /// Подпись есть, но не сходится (файл правили после подписи) или сертификат запрещён — признак подделки.
    Сломана,
}

impl Подпись {
    /// Действительная подпись: по решению Артура такую программу можно отпустить.
    pub fn действительна(self) -> bool {
        self == Подпись::Есть
    }
    #[allow(dead_code)] // сведение: подпись словами — для окна/журнала, когда сторож начнёт их показывать
    pub fn словами(self) -> &'static str {
        match self {
            Подпись::Есть => "подпись в порядке",
            Подпись::Нет => "без подписи издателя",
            Подпись::Сломана => "подпись не сходится — так делают подделки",
        }
    }
}

const WTD_UI_NONE: u32 = 2;
const WTD_REVOKE_NONE: u32 = 0;
const WTD_CHOICE_FILE: u32 = 1;
const WTD_STATEACTION_VERIFY: u32 = 1;
const WTD_STATEACTION_CLOSE: u32 = 2;
const WTD_REVOCATION_CHECK_NONE: u32 = 0x10;
const WTD_CACHE_ONLY_URL_RETRIEVAL: u32 = 0x1000;

// коды доверия
const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100u32 as i32;
const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B_0003u32 as i32;
const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B_0001u32 as i32;
const TRUST_E_BAD_DIGEST: i32 = 0x8009_6010u32 as i32;
const TRUST_E_EXPLICIT_DISTRUST: i32 = 0x800B_0111u32 as i32;
const CERT_E_REVOKED: i32 = 0x800B_010Cu32 as i32;

/// Чистая (видно в тесте): код `WinVerifyTrust` → наш вид подписи.
pub fn по_коду(код: i32) -> Подпись {
    match код {
        0 => Подпись::Есть,
        TRUST_E_NOSIGNATURE | TRUST_E_SUBJECT_FORM_UNKNOWN | TRUST_E_PROVIDER_UNKNOWN => Подпись::Нет,
        TRUST_E_BAD_DIGEST | TRUST_E_EXPLICIT_DISTRUST | CERT_E_REVOKED => Подпись::Сломана,
        _ => Подпись::Нет,
    }
}

/// Проверить подпись файла (офлайн, только из кэша). Структуры закрываем сразу после.
pub fn проверить(путь: &Path) -> Подпись {
    let Ok(полный) = std::fs::canonicalize(путь) else { return Подпись::Нет };
    let широкий: Vec<u16> = полный.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    let файл = ФайлИнфо {
        размер: std::mem::size_of::<ФайлИнфо>() as u32,
        путь: широкий.as_ptr(),
        файл: std::ptr::null(),
        субъект: std::ptr::null(),
    };
    let mut данные = Данные {
        размер: std::mem::size_of::<Данные>() as u32,
        политика: std::ptr::null(),
        sip: std::ptr::null(),
        вид_окна: WTD_UI_NONE,
        отзыв: WTD_REVOKE_NONE,
        выбор_объекта: WTD_CHOICE_FILE,
        файл: &файл,
        действие: WTD_STATEACTION_VERIFY,
        состояние: std::ptr::null(),
        url: std::ptr::null(),
        флаги: WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL,
        контекст_окна: 0,
        настройки_подписи: std::ptr::null(),
    };
    // GENERIC_VERIFY_V2
    let действие = Guid {
        d1: 0x00AA_C56B,
        d2: 0xCD44,
        d3: 0x11d0,
        d4: [0x8C, 0xC2, 0x00, 0xC0, 0x4F, 0xC2, 0x95, 0xEE],
    };
    let код = unsafe { WinVerifyTrust(0, &действие, &mut данные) };
    данные.действие = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(0, &действие, &mut данные) };
    по_коду(код)
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn коды_подписи() {
        assert_eq!(по_коду(0), Подпись::Есть);
        assert_eq!(по_коду(TRUST_E_NOSIGNATURE), Подпись::Нет);
        assert_eq!(по_коду(TRUST_E_BAD_DIGEST), Подпись::Сломана, "правили после подписи — подделка");
        assert_eq!(по_коду(CERT_E_REVOKED), Подпись::Сломана);
        assert!(Подпись::Есть.действительна() && !Подпись::Нет.действительна());
    }

    /// Живая проба (временная папка): файл без встроенной подписи читается как «Нет» — значит WinVerifyTrust
    /// работает офлайн и честно сообщает об отсутствии подписи. (Встроенную Authenticode видим; файлы,
    /// подписанные только через каталог Windows, как многие системные exe, тоже читаются как «Нет» — это то же
    /// ограничение `WTD_CHOICE_FILE`, что и у «Отправки»; для скачанных установщиков подпись встроенная.)
    #[test]
    #[ignore]
    fn живьём_подпись_без_подписи() {
        let врем = std::env::temp_dir().join(format!("mekh-podpis-{}", crate::svyaz::сейчас_мс()));
        std::fs::create_dir_all(&врем).unwrap();
        let файл = врем.join("без-подписи.bin");
        std::fs::write(&файл, b"MZ not really a signed program").unwrap();
        let п = проверить(&файл);
        assert_eq!(п, Подпись::Нет, "у файла нет встроенной подписи");
        let _ = std::fs::remove_dir_all(&врем);
    }
}
