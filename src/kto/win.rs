//! Вызовы Windows для паспорта программы — только ЧТЕНИЕ, только местно (в сеть ничего): ресурс версии файла, издатель
//! по встроенной подписи Authenticode (без сетевой проверки отзыва), загруженные библиотеки процесса, строка запуска,
//! заголовок видимого окна, видеопамять по процессам (`\GPU Process Memory`) и имена видеокарт по их LUID
//! (DXGI EnumAdapters1). Своими объявлениями `extern "system"`, без новых крейтов (как в `okna.rs`).
//!
//! Всё защищено: не вышло — `None`/пусто, никогда не паника и ничего не портит. Открываем чужие процессы только на
//! чтение (`PROCESS_QUERY_LIMITED_INFORMATION`, `PROCESS_VM_READ`): приоритеты и состояние чужих процессов не трогаем.

#![allow(clippy::too_many_arguments)]

use std::ffi::c_void;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(имя: *const u16) -> isize;
    fn GetProcAddress(модуль: isize, имя: *const u8) -> *const c_void;
    fn OpenProcess(доступ: u32, наследовать: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn K32EnumProcessModulesEx(h: isize, модули: *mut isize, кб: u32, нужно: *mut u32, фильтр: u32) -> i32;
    fn K32GetModuleBaseNameW(h: isize, модуль: isize, имя: *mut u16, размер: u32) -> u32;
}

fn широкая(с: &str) -> Vec<u16> { с.encode_utf16().chain(std::iter::once(0)).collect() }

fn функция(библиотека: &str, имя: &[u8]) -> Option<*const c_void> {
    let б = широкая(библиотека);
    let м = unsafe { LoadLibraryW(б.as_ptr()) };
    if м == 0 { return None; }
    let ф = unsafe { GetProcAddress(м, имя.as_ptr()) };
    (!ф.is_null()).then_some(ф)
}

/// Метод COM по номеру в таблице (как `okna::метод`).
unsafe fn метод<T: Copy>(объект: *mut c_void, номер: usize) -> T {
    let таблица = *(объект as *const *const *const c_void);
    std::mem::transmute_copy::<*const c_void, T>(&*таблица.add(номер))
}

// ───────────────────────────── ресурс версии ─────────────────────────────

#[link(name = "version")]
extern "system" {
    fn GetFileVersionInfoSizeW(файл: *const u16, дескр: *mut u32) -> u32;
    fn GetFileVersionInfoW(файл: *const u16, дескр: u32, длина: u32, данные: *mut c_void) -> i32;
    fn VerQueryValueW(блок: *const c_void, запрос: *const u16, значение: *mut *mut c_void, длина: *mut u32) -> i32;
}

/// Что взяли из ресурса версии файла.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Версия {
    pub продукт: Option<String>,
    pub фирма: Option<String>,
    pub описание: Option<String>,
}

impl Версия {
    pub fn пусто(&self) -> bool { self.продукт.is_none() && self.фирма.is_none() && self.описание.is_none() }
}

fn строка_ресурса(блок: &[u8], ключ_языка: &str, имя: &str) -> Option<String> {
    let запрос = широкая(&format!("\\StringFileInfo\\{ключ_языка}\\{имя}"));
    let mut значение: *mut c_void = std::ptr::null_mut();
    let mut длина = 0u32;
    let ок = unsafe { VerQueryValueW(блок.as_ptr() as *const c_void, запрос.as_ptr(), &mut значение, &mut длина) };
    if ок == 0 || значение.is_null() || длина == 0 { return None; }
    let слова = unsafe { std::slice::from_raw_parts(значение as *const u16, (длина as usize).saturating_sub(1)) };
    let с = String::from_utf16_lossy(слова).trim().to_string();
    (!с.is_empty()).then_some(с)
}

/// Прочитать ресурс версии файла. `None`/пусто — нет ресурса. Это дорогой разовый вызов (кэшируется в `pasport`).
pub fn версия(путь: &str) -> Версия {
    let п = широкая(путь);
    let mut дескр = 0u32;
    let размер = unsafe { GetFileVersionInfoSizeW(п.as_ptr(), &mut дескр) };
    if размер == 0 { return Версия::default(); }
    let mut данные = vec![0u8; размер as usize];
    if unsafe { GetFileVersionInfoW(п.as_ptr(), 0, размер, данные.as_mut_ptr() as *mut c_void) } == 0 {
        return Версия::default();
    }
    // перевод (язык + кодовая страница) — первый из \VarFileInfo\Translation; иначе ходовые английские
    let mut ключи: Vec<String> = Vec::new();
    let запрос = широкая("\\VarFileInfo\\Translation");
    let mut зн: *mut c_void = std::ptr::null_mut();
    let mut длина = 0u32;
    if unsafe { VerQueryValueW(данные.as_ptr() as *const c_void, запрос.as_ptr(), &mut зн, &mut длина) } != 0
        && !зн.is_null() {
        let пар = unsafe { std::slice::from_raw_parts(зн as *const u16, (длина as usize / 2).min(64)) };
        for пара in пар.chunks_exact(2) {
            ключи.push(format!("{:04x}{:04x}", пара[0], пара[1]));
        }
    }
    for запас in ["040904b0", "040904e4", "000004b0", "040904E4", "00000000"] {
        if !ключи.iter().any(|к| к.eq_ignore_ascii_case(запас)) { ключи.push(запас.to_string()); }
    }
    let найти = |имя: &str| ключи.iter().find_map(|к| строка_ресурса(&данные, к, имя));
    Версия {
        продукт: найти("ProductName"),
        фирма: найти("CompanyName"),
        описание: найти("FileDescription"),
    }
}

// ───────────────────────────── издатель по подписи (офлайн) ─────────────────────────────

const GUID_ПРОВЕРКА: [u8; 16] =
    [0x6b, 0xc5, 0xaa, 0x00, 0x44, 0xcd, 0xd0, 0x11, 0x8c, 0xc2, 0x00, 0xc0, 0x4f, 0xc2, 0x95, 0xee];

#[repr(C)]
struct СведенияФайла { размер: u32, _п: u32, путь: *const u16, файл: isize, субъект: *const c_void }

#[repr(C)]
struct ДанныеДоверия {
    размер: u32, _п0: u32,
    обратный: *const c_void, sip: *const c_void,
    выбор_интерфейса: u32, проверка_отзыва: u32, выбор_объекта: u32, _п1: u32,
    файл: *const СведенияФайла,
    действие: u32, _п2: u32,
    состояние: isize,
    url: *const u16,
    флаги: u32, контекст: u32,
    настройки: *const c_void,
}

type Проверить = unsafe extern "system" fn(isize, *const [u8; 16], *const c_void) -> i32;
type ДанныеПоСостоянию = unsafe extern "system" fn(isize) -> *mut c_void;
type Подписант = unsafe extern "system" fn(*mut c_void, u32, i32, u32) -> *mut c_void;
type СертИзЦепи = unsafe extern "system" fn(*mut c_void, u32) -> *mut c_void;
type ИмяСерта = unsafe extern "system" fn(*const c_void, u32, u32, *const c_void, *mut u16, u32) -> u32;

/// Издатель файла по встроенной подписи Authenticode, проверенной БЕЗ сетевой проверки отзыва (его «полностью офлайн»).
/// `None` — не подписан, подпись не прошла или ошибка. Дорогой разовый вызов (кэшируется).
pub fn издатель(путь: &str) -> Option<String> {
    let проверить: Проверить = unsafe { std::mem::transmute::<*const c_void, Проверить>(
        функция("wintrust.dll", b"WinVerifyTrust\0")?) };
    let п = широкая(путь);
    let файл = СведенияФайла { размер: std::mem::size_of::<СведенияФайла>() as u32, _п: 0, путь: п.as_ptr(),
        файл: 0, субъект: std::ptr::null() };
    let mut дд = ДанныеДоверия {
        размер: std::mem::size_of::<ДанныеДоверия>() as u32, _п0: 0,
        обратный: std::ptr::null(), sip: std::ptr::null(),
        выбор_интерфейса: 2, // WTD_UI_NONE
        проверка_отзыва: 0,  // WTD_REVOKE_NONE — без сети
        выбор_объекта: 1,    // WTD_CHOICE_FILE
        _п1: 0,
        файл: &файл,
        действие: 1,         // WTD_STATEACTION_VERIFY
        _п2: 0,
        состояние: 0,
        url: std::ptr::null(),
        флаги: 0x1000 | 0x100, // WTD_REVOCATION_CHECK_NONE | WTD_CACHE_ONLY_URL_RETRIEVAL
        контекст: 0,
        настройки: std::ptr::null(),
    };
    // WTD_STATEACTION_VERIFY
    let итог = unsafe { проверить(-1, &GUID_ПРОВЕРКА, &дд as *const ДанныеДоверия as *const c_void) };
    let mut имя = None;
    if итог == 0 && дд.состояние != 0 {
        имя = издатель_из_состояния(дд.состояние);
    }
    // WTD_STATEACTION_CLOSE — освободить состояние
    дд.действие = 2;
    unsafe { проверить(-1, &GUID_ПРОВЕРКА, &дд as *const ДанныеДоверия as *const c_void); }
    имя
}

fn издатель_из_состояния(состояние: isize) -> Option<String> {
    let из_сост: ДанныеПоСостоянию = unsafe { std::mem::transmute::<*const c_void, ДанныеПоСостоянию>(
        функция("wintrust.dll", b"WTHelperProvDataFromStateData\0")?) };
    let подписант: Подписант = unsafe { std::mem::transmute::<*const c_void, Подписант>(
        функция("wintrust.dll", b"WTHelperGetProvSignerFromChain\0")?) };
    let серт_из: СертИзЦепи = unsafe { std::mem::transmute::<*const c_void, СертИзЦепи>(
        функция("wintrust.dll", b"WTHelperGetProvCertFromChain\0")?) };
    let имя_серта: ИмяСерта = unsafe { std::mem::transmute::<*const c_void, ИмяСерта>(
        функция("crypt32.dll", b"CertGetNameStringW\0")?) };
    unsafe {
        let данные = из_сост(состояние);
        if данные.is_null() { return None; }
        let п_подп = подписант(данные, 0, 0, 0);
        if п_подп.is_null() { return None; }
        let п_серт = серт_из(п_подп, 0);
        if п_серт.is_null() { return None; }
        // CRYPT_PROVIDER_CERT: cbStruct(4)+pad(4), pCert по смещению 8
        let p_cert = *((п_серт as *const u8).add(8) as *const *const c_void);
        if p_cert.is_null() { return None; }
        // CERT_NAME_SIMPLE_DISPLAY_TYPE = 4
        let нужно = имя_серта(p_cert, 4, 0, std::ptr::null(), std::ptr::null_mut(), 0);
        if нужно <= 1 { return None; }
        let mut буф = vec![0u16; нужно as usize];
        let вышло = имя_серта(p_cert, 4, 0, std::ptr::null(), буф.as_mut_ptr(), нужно);
        if вышло <= 1 { return None; }
        let с = String::from_utf16_lossy(&буф[..(вышло as usize).saturating_sub(1)]).trim().to_string();
        (!с.is_empty()).then_some(с)
    }
}

// ───────────────────────────── загруженные библиотеки процесса ─────────────────────────────

/// Базовые имена загруженных библиотек процесса (строчными). Пусто — не открыть или нет прав (у процессов с правами
/// администратора читать нельзя — это нормально). Открываем на чтение.
pub fn библиотеки(pid: u32) -> Vec<String> {
    // PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ
    let h = unsafe { OpenProcess(0x1000 | 0x0010, 0, pid) };
    if h == 0 { return Vec::new(); }
    let mut модули = vec![0isize; 1024];
    let mut нужно = 0u32;
    // LIST_MODULES_ALL = 0x03
    let ок = unsafe {
        K32EnumProcessModulesEx(h, модули.as_mut_ptr(), (модули.len() * 8) as u32, &mut нужно, 0x03)
    } != 0;
    let mut имена = Vec::new();
    if ок {
        let штук = (нужно as usize / 8).min(модули.len());
        let mut имя = [0u16; 260];
        for &м in &модули[..штук] {
            let длина = unsafe { K32GetModuleBaseNameW(h, м, имя.as_mut_ptr(), имя.len() as u32) };
            if длина > 0 {
                имена.push(String::from_utf16_lossy(&имя[..длина as usize]).to_lowercase());
            }
        }
    }
    unsafe { CloseHandle(h) };
    имена
}

// ───────────────────────────── строка запуска ─────────────────────────────

type СпроситьПроцесс = unsafe extern "system" fn(isize, u32, *mut c_void, u32, *mut u32) -> i32;

/// Строка запуска процесса (ProcessCommandLineInformation = 60). `None` — не открыть или нет. В ней бывает личное —
/// `pasport` сверяет её один раз и дальше не хранит. Открываем на ограниченное чтение.
pub fn строка_запуска(pid: u32) -> Option<String> {
    let спросить: СпроситьПроцесс = unsafe { std::mem::transmute::<*const c_void, СпроситьПроцесс>(
        функция("ntdll.dll", b"NtQueryInformationProcess\0")?) };
    // PROCESS_QUERY_LIMITED_INFORMATION
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h == 0 { return None; }
    let mut нужно = 0u32;
    let mut буф = vec![0u8; 4096];
    let mut итог = None;
    for _ in 0..3 {
        let ст = unsafe { спросить(h, 60, буф.as_mut_ptr() as *mut c_void, буф.len() as u32, &mut нужно) };
        if ст == 0 {
            // UNICODE_STRING в начале буфера: Length(u16), MaximumLength(u16), pad, Buffer(*u16)
            let длина = u16::from_le_bytes([буф[0], буф[1]]) as usize;
            let адрес = usize::from_le_bytes(буф[8..16].try_into().unwrap_or([0; 8]));
            if адрес != 0 && длина > 0 && длина % 2 == 0 {
                let слова = unsafe { std::slice::from_raw_parts(адрес as *const u16, длина / 2) };
                итог = Some(String::from_utf16_lossy(слова));
            }
            break;
        }
        if ст as u32 == 0xC000_0004 && (нужно as usize) > буф.len() { буф.resize(нужно as usize, 0); continue; }
        break;
    }
    unsafe { CloseHandle(h) };
    итог
}

// ───────────────────────────── заголовок видимого окна ─────────────────────────────

#[link(name = "user32")]
extern "system" {
    fn EnumWindows(обратный: unsafe extern "system" fn(isize, isize) -> i32, арг: isize) -> i32;
    fn GetWindowThreadProcessId(окно: isize, pid: *mut u32) -> u32;
    fn IsWindowVisible(окно: isize) -> i32;
    fn GetWindow(окно: isize, отношение: u32) -> isize;
    fn GetWindowTextW(окно: isize, текст: *mut u16, длина: i32) -> i32;
    fn GetWindowTextLengthW(окно: isize) -> i32;
}

struct ПоискОкна { pid: u32, окно: bool, заголовок: Option<String> }

unsafe extern "system" fn на_окно(окно: isize, арг: isize) -> i32 {
    let п = &mut *(арг as *mut ПоискОкна);
    let mut pid = 0u32;
    GetWindowThreadProcessId(окно, &mut pid);
    if pid != п.pid || IsWindowVisible(окно) == 0 { return 1; }
    // только главное окно (без владельца; GW_OWNER = 4)
    if GetWindow(окно, 4) != 0 { return 1; }
    п.окно = true;
    let длина = GetWindowTextLengthW(окно);
    if длина > 0 {
        let mut буф = vec![0u16; длина as usize + 1];
        let вышло = GetWindowTextW(окно, буф.as_mut_ptr(), буф.len() as i32);
        if вышло > 0 {
            let с = String::from_utf16_lossy(&буф[..вышло as usize]);
            if !с.trim().is_empty() { п.заголовок = Some(с); return 0; }
        }
    }
    1
}

/// Есть ли у процесса видимое главное окно и его заголовок (сам заголовок нигде не хранится — `kto` берёт из него
/// только «видео ли это» и отдаёт слово для карточки). Читает только видимые окна рабочего стола.
pub fn окно(pid: u32) -> (bool, Option<String>) {
    let mut п = ПоискОкна { pid, окно: false, заголовок: None };
    unsafe { EnumWindows(на_окно, &mut п as *mut ПоискОкна as isize) };
    (п.окно, п.заголовок)
}

struct ВсеОкна(std::collections::HashMap<u32, (bool, Option<String>)>);

unsafe extern "system" fn на_любое_окно(окно: isize, арг: isize) -> i32 {
    let в = &mut *(арг as *mut ВсеОкна);
    if IsWindowVisible(окно) == 0 || GetWindow(окно, 4) != 0 { return 1; }
    let mut pid = 0u32;
    GetWindowThreadProcessId(окно, &mut pid);
    if pid == 0 { return 1; }
    let запись = в.0.entry(pid).or_insert((false, None));
    запись.0 = true;
    if запись.1.is_none() {
        let длина = GetWindowTextLengthW(окно);
        if длина > 0 {
            let mut буф = vec![0u16; длина as usize + 1];
            let вышло = GetWindowTextW(окно, буф.as_mut_ptr(), буф.len() as i32);
            if вышло > 0 {
                let с = String::from_utf16_lossy(&буф[..вышло as usize]);
                if !с.trim().is_empty() { запись.1 = Some(с); }
            }
        }
    }
    1
}

/// Все видимые главные окна за один проход: pid → (есть окно, заголовок первого с текстом). Дёшево — один `EnumWindows`.
pub fn все_окна() -> std::collections::HashMap<u32, (bool, Option<String>)> {
    let mut в = ВсеОкна(std::collections::HashMap::new());
    unsafe { EnumWindows(на_любое_окно, &mut в as *mut ВсеОкна as isize) };
    в.0
}

// ───────────────────────────── видеопамять по процессам (PDH) ─────────────────────────────

type Ручка = *mut c_void;
#[repr(C)]
#[derive(Clone, Copy)]
struct ЗначениеМ { статус: u32, число: i64 }
#[repr(C)]
#[derive(Clone, Copy)]
struct Элемент { имя: *const u16, значение: ЗначениеМ }

#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(источник: *const u16, данные: usize, запрос: *mut Ручка) -> u32;
    fn PdhAddEnglishCounterW(запрос: Ручка, путь: *const u16, данные: usize, счётчик: *mut Ручка) -> u32;
    fn PdhCollectQueryData(запрос: Ручка) -> u32;
    fn PdhCloseQuery(запрос: Ручка) -> u32;
}

// `PdhGetFormattedCounterArrayW` уже объявлен в `okna.rs` с другим типом последнего поля (`Элемент` там свой). Чтобы не
// плодить спорное второе объявление того же символа, берём его через GetProcAddress.
type МассивСчётчика = unsafe extern "system" fn(Ручка, u32, *mut u32, *mut u32, *mut Элемент) -> u32;
fn массив_счётчика() -> Option<МассивСчётчика> {
    Some(unsafe { std::mem::transmute::<*const c_void, МассивСчётчика>(
        функция("pdh.dll", b"PdhGetFormattedCounterArrayW\0")?) })
}

/// Чистая (видно в тесте): из имени экземпляра `pid_1234_luid_0x00000000_0x0000abcd_phys_0` — номер процесса и LUID
/// видеокарты (`luid_0x00000000_0x0000abcd`), как в именах адаптеров DXGI.
pub fn разобрать_экземпляр(имя: &str) -> Option<(u32, String)> {
    let хвост = имя.strip_prefix("pid_")?;
    let конец = хвост.find('_')?;
    let pid: u32 = хвост[..конец].parse().ok()?;
    let после = &хвост[конец + 1..];
    let нач = после.find("luid_")?;
    let люид = &после[нач..];
    let люид = люид.find("_phys").map(|к| &люид[..к]).unwrap_or(люид);
    Some((pid, люид.to_lowercase()))
}

/// Выделенная видеопамять по процессам, байт: pid → суммарно и по каждой видеокарте (LUID). Дёшево (≈0 мс — счётчик
/// мгновенный, один сбор). Пусто — счётчика нет.
#[derive(Clone, Debug, Default)]
pub struct Видеопамять {
    /// pid → всего байт.
    pub всего: std::collections::HashMap<u32, u64>,
    /// pid → (LUID видеокарты → байт).
    pub по_картам: std::collections::HashMap<u32, std::collections::HashMap<String, u64>>,
}

pub fn видеопамять() -> Видеопамять {
    let mut итог = Видеопамять::default();
    let Some(массив) = массив_счётчика() else { return итог; };
    let mut запрос: Ручка = std::ptr::null_mut();
    if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut запрос) } != 0 { return итог; }
    let путь = широкая(r"\GPU Process Memory(*)\Dedicated Usage");
    let mut счётчик: Ручка = std::ptr::null_mut();
    if unsafe { PdhAddEnglishCounterW(запрос, путь.as_ptr(), 0, &mut счётчик) } != 0 {
        unsafe { PdhCloseQuery(запрос) };
        return итог;
    }
    unsafe { PdhCollectQueryData(запрос) };
    let mut байт = 0u32;
    let mut штук = 0u32;
    // PDH_FMT_LARGE = 0x00000400
    let ст = unsafe { массив(счётчик, 0x400, &mut байт, &mut штук, std::ptr::null_mut()) };
    // PDH_MORE_DATA = 0x800007D2
    if ст as u32 == 0x8000_07D2 && байт > 0 {
        let мест = (байт as usize / std::mem::size_of::<Элемент>()) + 1;
        let mut буф = vec![Элемент { имя: std::ptr::null(), значение: ЗначениеМ { статус: 0, число: 0 } }; мест];
        let ст2 = unsafe {
            массив(счётчик, 0x400, &mut байт, &mut штук, буф.as_mut_ptr())
        };
        if ст2 == 0 {
            for э in буф.iter().take(штук as usize) {
                if э.имя.is_null() || э.значение.статус != 0 || э.значение.число <= 0 { continue; }
                let имя = unsafe { строка_из(э.имя) };
                if let Some((pid, люид)) = разобрать_экземпляр(&имя) {
                    let байт = э.значение.число as u64;
                    *итог.всего.entry(pid).or_insert(0) += байт;
                    *итог.по_картам.entry(pid).or_default().entry(люид).or_insert(0) += байт;
                }
            }
        }
    }
    unsafe { PdhCloseQuery(запрос) };
    итог
}

unsafe fn строка_из(п: *const u16) -> String {
    if п.is_null() { return String::new(); }
    let mut длина = 0usize;
    while *п.add(длина) != 0 && длина < 512 { длина += 1; }
    String::from_utf16_lossy(std::slice::from_raw_parts(п, длина))
}

// ───────────────────────────── имена видеокарт по LUID (DXGI) ─────────────────────────────

type СоздатьФабрику = unsafe extern "system" fn(*const [u8; 16], *mut *mut c_void) -> i32;
type ПеречислитьАдаптеры = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
type ОписаниеАдаптера = unsafe extern "system" fn(*mut c_void, *mut [u8; 320]) -> i32;
type Отпустить = unsafe extern "system" fn(*mut c_void) -> u32;

/// Имя видеокарты (`Description`) и её изготовитель по коду производителя.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Карта { pub имя: String, pub изготовитель: Изготовитель }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Изготовитель { NVIDIA, Intel, AMD, Microsoft, #[default] Другой }

impl Изготовитель {
    /// По коду производителя PCI.
    pub fn по_коду(код: u32) -> Изготовитель {
        match код {
            0x10DE => Изготовитель::NVIDIA,
            0x8086 => Изготовитель::Intel,
            0x1002 | 0x1022 => Изготовитель::AMD,
            0x1414 => Изготовитель::Microsoft,
            _ => Изготовитель::Другой,
        }
    }
    pub fn слово(self) -> &'static str {
        match self {
            Изготовитель::NVIDIA => "NVIDIA", Изготовитель::Intel => "Intel", Изготовитель::AMD => "AMD",
            Изготовитель::Microsoft => "программная", Изготовитель::Другой => "видеокарта",
        }
    }
}

/// Видеокарты по их LUID (`luid_0x..._0x...`) — через DXGI EnumAdapters1, офлайн. Пусто — нет DXGI.
pub fn видеокарты() -> std::collections::HashMap<String, Карта> {
    let mut карты = std::collections::HashMap::new();
    let Some(ф) = функция("dxgi.dll", b"CreateDXGIFactory1\0") else { return карты; };
    let создать: СоздатьФабрику = unsafe { std::mem::transmute::<*const c_void, СоздатьФабрику>(ф) };
    // IID_IDXGIFactory1 {770aae78-f26f-4dba-a829-253c83d1b387}
    let iid: [u8; 16] = [0x78, 0xae, 0x0a, 0x77, 0x6f, 0xf2, 0xba, 0x4d, 0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87];
    let mut фабрика: *mut c_void = std::ptr::null_mut();
    if unsafe { создать(&iid, &mut фабрика) } < 0 || фабрика.is_null() { return карты; }
    unsafe {
        // IDXGIFactory1::EnumAdapters1 — 12
        let перечислить: ПеречислитьАдаптеры = метод(фабрика, 12);
        let mut и = 0u32;
        loop {
            let mut адаптер: *mut c_void = std::ptr::null_mut();
            if перечислить(фабрика, и, &mut адаптер) < 0 || адаптер.is_null() { break; }
            // IDXGIAdapter1::GetDesc1 — 10
            let описание: ОписаниеАдаптера = метод(адаптер, 10);
            let mut оп = [0u8; 320];
            if описание(адаптер, &mut оп) >= 0 {
                let имя = {
                    let слова: Vec<u16> = оп[..256].chunks_exact(2).map(|п| u16::from_le_bytes([п[0], п[1]])).collect();
                    let конец = слова.iter().position(|&c| c == 0).unwrap_or(слова.len());
                    String::from_utf16_lossy(&слова[..конец]).trim().to_string()
                };
                let код = u32::from_le_bytes([оп[256], оп[257], оп[258], оп[259]]);
                let низ = u32::from_le_bytes([оп[296], оп[297], оп[298], оп[299]]);
                let верх = u32::from_le_bytes([оп[300], оп[301], оп[302], оп[303]]);
                let люид = format!("luid_0x{верх:08x}_0x{низ:08x}");
                карты.insert(люид, Карта { имя, изготовитель: Изготовитель::по_коду(код) });
            }
            метод::<Отпустить>(адаптер, 2)(адаптер);
            и += 1;
            if и > 16 { break; }
        }
        метод::<Отпустить>(фабрика, 2)(фабрика);
    }
    карты
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn разбор_экземпляра_видеопамяти() {
        assert_eq!(разобрать_экземпляр("pid_1234_luid_0x00000000_0x0000abcd_phys_0"),
            Some((1234, "luid_0x00000000_0x0000abcd".into())));
        assert_eq!(разобрать_экземпляр("pid_27884_luid_0x00000000_0x00010f5c_phys_0"),
            Some((27884, "luid_0x00000000_0x00010f5c".into())));
        assert_eq!(разобрать_экземпляр("мусор"), None);
        assert_eq!(разобрать_экземпляр("pid_x_luid_0x0_0x0"), None);
    }

    #[test]
    fn изготовитель_по_коду() {
        assert_eq!(Изготовитель::по_коду(0x10DE), Изготовитель::NVIDIA);
        assert_eq!(Изготовитель::по_коду(0x8086), Изготовитель::Intel);
        assert_eq!(Изготовитель::по_коду(0x1002), Изготовитель::AMD);
        assert_eq!(Изготовитель::по_коду(0x1414), Изготовитель::Microsoft);
        assert_eq!(Изготовитель::по_коду(0xDEAD), Изготовитель::Другой);
        assert_eq!(Изготовитель::NVIDIA.слово(), "NVIDIA");
    }

    /// Живая проба (только чтение своего процесса): читаем ресурс версии и библиотеки СВОЕГО процесса.
    /// Запуск: `cargo test -- --ignored --nocapture` из своей папки через очередь.
    #[test]
    #[ignore]
    fn живьём_свой_процесс() {
        let путь = std::env::current_exe().unwrap().to_string_lossy().to_string();
        let в = версия(&путь);
        println!("версия своего exe: {в:?}");
        let библ = библиотеки(std::process::id());
        println!("своих библиотек: {} (напр. {:?})", библ.len(), библ.iter().take(8).collect::<Vec<_>>());
        assert!(!библ.is_empty(), "у своего процесса должны быть загруженные библиотеки");
        let карты = видеокарты();
        println!("видеокарты: {карты:?}");
        let вп = видеопамять();
        println!("процессов с видеопамятью: {}", вп.всего.len());
    }
}
