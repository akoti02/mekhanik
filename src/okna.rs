//! Вызовы Windows «Механика» (замер перенесён из БАРСа `tormoza_pk.rs` 25.10, Р-193; лечение — Р-194): своими
//! объявлениями, без крейта `windows`.

use std::ffi::c_void;
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(имя: *const u16) -> isize;
    fn GetProcAddress(модуль: isize, имя: *const u8) -> *const c_void;
    fn CreateWaitableTimerExW(атрибуты: *const c_void, имя: *const u16, флаги: u32, доступ: u32) -> isize;
    fn SetWaitableTimer(h: isize, срок: *const i64, период: i32, ф: *const c_void, арг: *const c_void, проснуть: i32) -> i32;
    fn WaitForSingleObject(h: isize, мс: u32) -> u32;
    fn OpenProcess(доступ: u32, наследовать: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn GetCurrentThread() -> isize;
    fn GetThreadTimes(h: isize, создан: *mut u64, конец: *mut u64, ядро: *mut u64, польз: *mut u64) -> i32;
    fn GetPriorityClass(h: isize) -> u32;
    fn SetPriorityClass(h: isize, класс: u32) -> i32;
    fn CreateToolhelp32Snapshot(флаги: u32, pid: u32) -> isize;
    fn Process32FirstW(h: isize, з: *mut Запись) -> i32;
    fn Process32NextW(h: isize, з: *mut Запись) -> i32;
    fn GlobalMemoryStatusEx(м: *mut Память) -> i32;
    fn GetDiskFreeSpaceExW(путь: *const u16, доступно: *mut u64, всего: *mut u64, свободно: *mut u64) -> i32;
    fn CreateMutexW(атрибуты: *const c_void, взять: i32, имя: *const u16) -> isize;
    fn GetLastError() -> u32;
    fn GetTickCount() -> u32;
    fn ProcessIdToSessionId(pid: u32, сеанс: *mut u32) -> i32;
    fn K32EmptyWorkingSet(h: isize) -> i32;
    fn GetProcessTimes(h: isize, создан: *mut u64, конец: *mut u64, ядро: *mut u64, польз: *mut u64) -> i32;
    fn GetProcessInformation(h: isize, класс: u32, данные: *mut c_void, размер: u32) -> i32;
    fn SetProcessInformation(h: isize, класс: u32, данные: *const c_void, размер: u32) -> i32;
    fn QueryUnbiasedInterruptTime(время: *mut u64) -> i32;
}

/// 🟥 27.10: время без сна ПК, мс (QueryUnbiasedInterruptTime — сон и спячка в него не входят). Часы шага ушли на
/// минуты, а это время — нет: ПК спал, это не тормоз (27.10 20:01–22:13 сон сочли тормозом в 7 927 с).
pub fn без_сна_мс() -> Option<u64> {
    let mut в = 0u64;
    (unsafe { QueryUnbiasedInterruptTime(&mut в) } != 0).then_some(в / 10_000)
}

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(окно: isize, pid: *mut u32) -> u32;
    fn IsHungAppWindow(окно: isize) -> i32;
    fn GetLastInputInfo(в: *mut ПоследнийВвод) -> i32;
}

#[repr(C)]
struct ПоследнийВвод { размер: u32, когда: u32 }

fn широкая(с: &str) -> Vec<u16> { с.encode_utf16().chain(std::iter::once(0)).collect() }

fn функция(библиотека: &str, имя: &[u8]) -> Option<*const c_void> {
    let б = широкая(библиотека);
    let м = unsafe { LoadLibraryW(б.as_ptr()) };
    if м == 0 { return None; }
    let ф = unsafe { GetProcAddress(м, имя.as_ptr()) };
    (!ф.is_null()).then_some(ф)
}

/// Один «Механик» на сеанс: мьютекс уже есть — второй экземпляр выходит. Ручку держим до конца процесса.
pub fn единственный() -> bool {
    let имя = широкая("Local\\mekhanik-odin");
    let h = unsafe { CreateMutexW(std::ptr::null(), 0, имя.as_ptr()) };
    // ERROR_ALREADY_EXISTS = 183
    h != 0 && unsafe { GetLastError() } != 183
}

/// Точный сон (таймер высокого разрешения, без смены разрешения таймера всей системы).
pub struct Сон(isize);
impl Сон {
    pub fn новый() -> Сон {
        // CREATE_WAITABLE_TIMER_HIGH_RESOLUTION = 2, TIMER_ALL_ACCESS = 0x1F0003
        Сон(unsafe { CreateWaitableTimerExW(std::ptr::null(), std::ptr::null(), 2, 0x1F_0003) })
    }
    pub fn спать(&self, сколько: Duration) {
        if self.0 == 0 { std::thread::sleep(сколько); return; }
        let срок: i64 = -((сколько.as_nanos() / 100) as i64).max(1);
        if unsafe { SetWaitableTimer(self.0, &срок, 0, std::ptr::null(), std::ptr::null(), 0) } == 0 {
            std::thread::sleep(сколько);
            return;
        }
        unsafe { WaitForSingleObject(self.0, 0xFFFF_FFFF) };
    }
}
impl Drop for Сон { fn drop(&mut self) { if self.0 != 0 { unsafe { CloseHandle(self.0) }; } } }

/// Время процессора этого потока, мс.
pub fn время_потока_мс() -> u64 {
    let (mut с, mut к, mut я, mut п) = (0u64, 0u64, 0u64, 0u64);
    if unsafe { GetThreadTimes(GetCurrentThread(), &mut с, &mut к, &mut я, &mut п) } == 0 { return 0; }
    (я + п) / 10_000
}

// ── видеокарта: запрос Direct3D 11 на адаптере экрана ──
type Создать = unsafe extern "system" fn(*mut c_void, u32, isize, u32, *const u32, u32, u32, *mut *mut c_void,
    *mut u32, *mut *mut c_void) -> i32;
type СоздатьЗапрос = unsafe extern "system" fn(*mut c_void, *const [u32; 2], *mut *mut c_void) -> i32;
type Конец = unsafe extern "system" fn(*mut c_void, *mut c_void);
type Данные = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void, u32, u32) -> i32;
type Отпустить = unsafe extern "system" fn(*mut c_void) -> u32;

/// Метод COM по номеру в таблице (ID3D11Device::CreateQuery — 24, ID3D11DeviceContext::End — 28, GetData — 29).
unsafe fn метод<T: Copy>(объект: *mut c_void, номер: usize) -> T {
    let таблица = *(объект as *const *const *const c_void);
    std::mem::transmute_copy::<*const c_void, T>(&*таблица.add(номер))
}

pub struct Видеокарта { устройство: *mut c_void, контекст: *mut c_void, запрос: *mut c_void }

impl Видеокарта {
    pub fn открыть() -> Result<Видеокарта, String> {
        let ф = функция("d3d11.dll", b"D3D11CreateDevice\0").ok_or("нет D3D11CreateDevice")?;
        let создать: Создать = unsafe { std::mem::transmute::<*const c_void, Создать>(ф) };
        let (mut у, mut к, mut уровень) = (std::ptr::null_mut(), std::ptr::null_mut(), 0u32);
        // адаптер по умолчанию — тот, что рисует экран; D3D_DRIVER_TYPE_HARDWARE = 1, D3D11_SDK_VERSION = 7
        let hr = unsafe { создать(std::ptr::null_mut(), 1, 0, 0, std::ptr::null(), 0, 7, &mut у, &mut уровень, &mut к) };
        if hr < 0 || у.is_null() || к.is_null() { return Err(format!("D3D11CreateDevice {hr:#x}")); }
        let mut в = Видеокарта { устройство: у, контекст: к, запрос: std::ptr::null_mut() };
        let описание: [u32; 2] = [0, 0]; // D3D11_QUERY_EVENT
        let hr = unsafe { метод::<СоздатьЗапрос>(у, 24)(у, &описание, &mut в.запрос) };
        if hr < 0 || в.запрос.is_null() { return Err(format!("CreateQuery {hr:#x}")); }
        Ok(в)
    }
    /// 🟥 27.10: адаптер, на котором меряем (тот, что рисует экран), — именем, как в счётчиках видеокарты
    /// (`luid_0x00000000_0x00010f5c`). Экран рисует Intel Arc, а счёт по процессам складывал Intel и NVIDIA: виновника
    /// задержки экрана искали не на той видеокарте.
    pub fn адаптер(&self) -> Option<String> {
        type Спросить = unsafe extern "system" fn(*mut c_void, *const [u8; 16], *mut *mut c_void) -> i32;
        type ВзятьАдаптер = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32;
        type Описание = unsafe extern "system" fn(*mut c_void, *mut [u8; 304]) -> i32;
        // IID_IDXGIDevice {54ec77fa-1377-44e6-8c32-88fd5f44c84c}
        let iid: [u8; 16] = [0xfa, 0x77, 0xec, 0x54, 0x77, 0x13, 0xe6, 0x44, 0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c];
        let (mut дхги, mut адаптер) = (std::ptr::null_mut(), std::ptr::null_mut());
        unsafe {
            if метод::<Спросить>(self.устройство, 0)(self.устройство, &iid, &mut дхги) < 0 || дхги.is_null() { return None; }
            // IDXGIDevice::GetAdapter — 7
            let hr = метод::<ВзятьАдаптер>(дхги, 7)(дхги, &mut адаптер);
            метод::<Отпустить>(дхги, 2)(дхги);
            if hr < 0 || адаптер.is_null() { return None; }
            // IDXGIAdapter::GetDesc — 8; DXGI_ADAPTER_DESC: AdapterLuid — с 296-го байта (LowPart, HighPart)
            let mut оп = [0u8; 304];
            let hr = метод::<Описание>(адаптер, 8)(адаптер, &mut оп);
            метод::<Отпустить>(адаптер, 2)(адаптер);
            if hr < 0 { return None; }
            let низ = u32::from_le_bytes([оп[296], оп[297], оп[298], оп[299]]);
            let верх = u32::from_le_bytes([оп[300], оп[301], оп[302], оп[303]]);
            Some(format!("luid_0x{верх:08x}_0x{низ:08x}"))
        }
    }
    /// Сколько ждала крошечная работа на видеокарте, мс (не дольше `предел`); `Err` — устройство потеряно.
    pub fn ожидание(&self, предел: Duration, сон: &Сон) -> Result<f64, String> {
        let старт = Instant::now();
        unsafe { метод::<Конец>(self.контекст, 28)(self.контекст, self.запрос) };
        let mut готово: i32 = 0;
        loop {
            let hr = unsafe { метод::<Данные>(self.контекст, 29)(self.контекст, self.запрос,
                &mut готово as *mut i32 as *mut c_void, 4, 0) };
            if hr == 0 { return Ok(старт.elapsed().as_secs_f64() * 1000.0); }
            if hr < 0 { return Err(format!("GetData {hr:#x}")); }
            if старт.elapsed() >= предел { return Ok(предел.as_secs_f64() * 1000.0); }
            сон.спать(Duration::from_millis(1));
        }
    }
}
impl Drop for Видеокарта {
    fn drop(&mut self) {
        for о in [self.запрос, self.контекст, self.устройство] {
            if !о.is_null() { unsafe { метод::<Отпустить>(о, 2)(о) }; }
        }
    }
}

// ── заморозка процесса на шаг уступки и приоритет процессора ──
type Пауза = unsafe extern "system" fn(isize) -> i32;
static ЗАМОРОЗИТЬ: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
static ОТПУСТИТЬ: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();

/// Заморозить (`да`) или отпустить процесс целиком (NtSuspendProcess / NtResumeProcess: счёт вложенный — сколько
/// раз заморозили, столько и отпускать). `false` — не открыть или отказ.
pub fn заморозить(pid: u32, да: bool) -> bool {
    let ячейка = if да { &ЗАМОРОЗИТЬ } else { &ОТПУСТИТЬ };
    let имя: &[u8] = if да { b"NtSuspendProcess\0" } else { b"NtResumeProcess\0" };
    let Some(адрес) = *ячейка.get_or_init(|| функция("ntdll.dll", имя).map(|ф| ф as usize)) else { return false };
    let ф: Пауза = unsafe { std::mem::transmute::<usize, Пауза>(адрес) };
    // PROCESS_SUSPEND_RESUME
    let h = unsafe { OpenProcess(0x0800, 0, pid) };
    if h == 0 { return false; }
    let ст = unsafe { ф(h) };
    unsafe { CloseHandle(h) };
    ст >= 0
}

/// Приоритет процессора процесса: (был, стал). `None` — не открыть или отказ.
pub fn приоритет(pid: u32, класс: u32) -> Option<(u32, u32)> {
    // PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_INFORMATION
    let h = unsafe { OpenProcess(0x1000 | 0x0200, 0, pid) };
    if h == 0 { return None; }
    let был = unsafe { GetPriorityClass(h) };
    let итог = if был == 0 { None } else if был == класс { Some((был, был)) }
        else if unsafe { SetPriorityClass(h, класс) } != 0 { Some((был, класс)) } else { None };
    unsafe { CloseHandle(h) };
    итог
}

// ── очередь видеокарты процесса (gdi32, процессам своего пользователя без прав администратора) ──
type ВзятьКласс = unsafe extern "system" fn(isize, *mut i32) -> i32;
type ДатьКласс = unsafe extern "system" fn(isize, i32) -> i32;

/// Класс очереди видеокарты процесса: (был, стал). `None` — не открыть или вызов отказал.
pub fn очередь_видеокарты(pid: u32, класс: i32) -> Option<(i32, i32)> {
    let взять: ВзятьКласс = unsafe { std::mem::transmute::<*const c_void, ВзятьКласс>(
        функция("gdi32.dll", b"D3DKMTGetProcessSchedulingPriorityClass\0")?) };
    let дать: ДатьКласс = unsafe { std::mem::transmute::<*const c_void, ДатьКласс>(
        функция("gdi32.dll", b"D3DKMTSetProcessSchedulingPriorityClass\0")?) };
    // PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION (чтению класса нужно и право записи — проверено 24.10)
    let h = unsafe { OpenProcess(0x0400 | 0x0200, 0, pid) };
    if h == 0 { return None; }
    let mut был = -1;
    let mut итог = None;
    if unsafe { взять(h, &mut был) } >= 0 {
        итог = if был == класс { Some((был, был)) } else if unsafe { дать(h, класс) } >= 0 { Some((был, класс)) } else { None };
    }
    unsafe { CloseHandle(h) };
    итог
}

/// 🟥 25.10 (Р-194): только понизить очередь видеокарты (до `класс`): (был, стал); была ниже — не трогаем (`(был, был)`).
pub fn понизить_видеокарту(pid: u32, класс: i32) -> Option<(i32, i32)> {
    let взять: ВзятьКласс = unsafe { std::mem::transmute::<*const c_void, ВзятьКласс>(
        функция("gdi32.dll", b"D3DKMTGetProcessSchedulingPriorityClass\0")?) };
    let дать: ДатьКласс = unsafe { std::mem::transmute::<*const c_void, ДатьКласс>(
        функция("gdi32.dll", b"D3DKMTSetProcessSchedulingPriorityClass\0")?) };
    let h = unsafe { OpenProcess(0x0400 | 0x0200, 0, pid) };
    if h == 0 { return None; }
    let mut был = -1;
    let mut итог = None;
    if unsafe { взять(h, &mut был) } >= 0 {
        итог = if был <= класс { Some((был, был)) } else if unsafe { дать(h, класс) } >= 0 { Some((был, класс)) } else { None };
    }
    unsafe { CloseHandle(h) };
    итог
}

// ── снимок процессов одним вызовом (NtQuerySystemInformation, SystemProcessInformation) ──
type Спросить = unsafe extern "system" fn(u32, *mut c_void, u32, *mut u32) -> i32;

/// Процесс в снимке (накопленные счётчики — разница двух снимков даёт скорость).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Проц {
    pub имя: String,
    pub родитель: u32,
    /// Время процессора, 100 нс.
    pub время: u64,
    /// Жёстких промахов памяти (чтение страниц с диска).
    pub промахи: u64,
    /// Своя зачисленная память, байт.
    pub память: u64,
    /// Рабочий набор (в ОЗУ), байт.
    pub рабочий: u64,
    /// Прочитано и записано, байт.
    pub ввод_вывод: u64,
    pub потоков: u32,
    pub дескрипторов: u32,
}

pub type Снимок = std::collections::HashMap<u32, Проц>;

pub fn снимок(буф: &mut Vec<u8>) -> Option<Снимок> {
    let спросить: Спросить = unsafe { std::mem::transmute::<*const c_void, Спросить>(
        функция("ntdll.dll", b"NtQuerySystemInformation\0")?) };
    let mut длина = 0u32;
    let mut вышло = false;
    for _ in 0..3 {
        let ст = unsafe { спросить(5, буф.as_mut_ptr() as *mut c_void, буф.len() as u32, &mut длина) };
        if ст == 0 { вышло = true; break; }
        // STATUS_INFO_LENGTH_MISMATCH — мал буфер
        if ст as u32 == 0xC000_0004 { буф.resize((длина as usize).max(буф.len() * 2) + (64 << 10), 0); continue; }
        return None;
    }
    if !вышло { return None; }
    let б = &буф[..(длина as usize).min(буф.len())];
    let u32_ = |о: usize| б.get(о..о + 4).map(|с| u32::from_le_bytes(с.try_into().unwrap_or([0; 4]))).unwrap_or(0);
    let u64_ = |о: usize| б.get(о..о + 8).map(|с| u64::from_le_bytes(с.try_into().unwrap_or([0; 8]))).unwrap_or(0);
    let начало = б.as_ptr() as usize;
    let mut все = std::collections::HashMap::new();
    let mut о = 0usize;
    loop {
        // SYSTEM_PROCESS_INFORMATION (x64) — 256 байт до потоков
        if о + 256 > б.len() { break; }
        let следующий = u32_(о) as usize;
        let потоков = u32_(о + 4);
        let дескрипторов = u32_(о + 96);
        let промахи = u32_(о + 16) as u64;
        let время = u64_(о + 40) + u64_(о + 48);
        let длина_имени = (u32_(о + 56) & 0xFFFF) as usize;
        let адрес = u64_(о + 64) as usize;
        let pid = u64_(о + 80) as u32;
        let родитель = u64_(о + 88) as u32;
        let рабочий = u64_(о + 144);
        let память = u64_(о + 184);
        // ReadTransferCount + WriteTransferCount
        let ввод_вывод = u64_(о + 232) + u64_(о + 240);
        let имя = if адрес >= начало && адрес + длина_имени <= начало + б.len() && длина_имени > 0 {
            let с = адрес - начало;
            let слова: Vec<u16> = б[с..с + длина_имени].chunks_exact(2).map(|п| u16::from_le_bytes([п[0], п[1]])).collect();
            String::from_utf16_lossy(&слова)
        } else { "Idle".to_string() };
        все.insert(pid, Проц { имя, родитель, время, промахи, память, рабочий, ввод_вывод, потоков, дескрипторов });
        if следующий == 0 { break; }
        о += следующий;
    }
    Some(все)
}

// ── все процессы машины (номер, родитель, имя) ──
#[repr(C)]
struct Запись { размер: u32, _исп: u32, pid: u32, _куча: usize, _модуль: u32, _нитей: u32, родитель: u32,
                _приоритет: i32, _флаги: u32, имя: [u16; 260] }

pub fn процессы() -> Vec<(u32, u32, String)> {
    let h = unsafe { CreateToolhelp32Snapshot(0x2, 0) }; // TH32CS_SNAPPROCESS
    if h == -1 || h == 0 { return Vec::new(); }
    let mut все: Vec<(u32, u32, String)> = Vec::with_capacity(400);
    let mut з = Запись { размер: std::mem::size_of::<Запись>() as u32, _исп: 0, pid: 0, _куча: 0, _модуль: 0, _нитей: 0,
        родитель: 0, _приоритет: 0, _флаги: 0, имя: [0; 260] };
    let mut есть = unsafe { Process32FirstW(h, &mut з) } != 0;
    while есть {
        let конец = з.имя.iter().position(|&c| c == 0).unwrap_or(з.имя.len());
        все.push((з.pid, з.родитель, String::from_utf16_lossy(&з.имя[..конец])));
        есть = unsafe { Process32NextW(h, &mut з) } != 0;
    }
    unsafe { CloseHandle(h) };
    все
}

// ── память ──
#[repr(C)]
struct Память { длина: u32, загрузка: u32, всего_физ: u64, доступно_физ: u64, всего_зач: u64, доступно_зач: u64,
    всего_вирт: u64, доступно_вирт: u64, доступно_расш: u64 }

/// (доступно памяти МБ, зачислено % предела, свободно зачисленной МБ).
pub fn память() -> (Option<u64>, Option<f64>, Option<u64>) {
    let mut м = Память { длина: std::mem::size_of::<Память>() as u32, загрузка: 0, всего_физ: 0, доступно_физ: 0,
        всего_зач: 0, доступно_зач: 0, всего_вирт: 0, доступно_вирт: 0, доступно_расш: 0 };
    if unsafe { GlobalMemoryStatusEx(&mut м) } == 0 { return (None, None, None); }
    let зачислено = (м.всего_зач > 0).then(|| (м.всего_зач - м.доступно_зач) as f64 * 100.0 / м.всего_зач as f64);
    (Some(м.доступно_физ >> 20), зачислено, Some(м.доступно_зач >> 20))
}

/// Свободно на томе, МБ.
pub fn диск_свободно_мб(буква: char) -> Option<u64> {
    let путь = широкая(&format!("{буква}:\\"));
    let (mut д, mut в, mut с) = (0u64, 0u64, 0u64);
    (unsafe { GetDiskFreeSpaceExW(путь.as_ptr(), &mut д, &mut в, &mut с) } != 0).then_some(д >> 20)
}

/// Переднее окно висит — номер его процесса.
pub fn висит_pid() -> Option<u32> {
    let окно = unsafe { GetForegroundWindow() };
    if окно == 0 || unsafe { IsHungAppWindow(окно) } == 0 { return None; }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(окно, &mut pid) };
    Some(pid)
}

// ── счётчики: подкачка с диска, прерывания драйверов ──
type Ручка = *mut c_void;
#[repr(C)]
struct Значение { статус: u32, _п: u32, число: f64 }
#[link(name = "pdh")]
extern "system" {
    fn PdhOpenQueryW(источник: *const u16, данные: usize, запрос: *mut Ручка) -> u32;
    fn PdhAddEnglishCounterW(запрос: Ручка, путь: *const u16, данные: usize, счётчик: *mut Ручка) -> u32;
    fn PdhCollectQueryData(запрос: Ручка) -> u32;
    fn PdhGetFormattedCounterValue(счётчик: Ручка, формат: u32, тип: *mut u32, значение: *mut Значение) -> u32;
}
pub struct Счётчики { запрос: Ручка, подкачка: Ручка, dpc: Ручка, прерывания: Ручка, диск_простой: Ручка }
impl Счётчики {
    pub fn открыть() -> Option<Счётчики> {
        let mut з: Ручка = std::ptr::null_mut();
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut з) } != 0 { return None; }
        let добавить = |путь: &str| -> Ручка {
            let п = широкая(путь);
            let mut с: Ручка = std::ptr::null_mut();
            if unsafe { PdhAddEnglishCounterW(з, п.as_ptr(), 0, &mut с) } == 0 { с } else { std::ptr::null_mut() }
        };
        let с = Счётчики { запрос: з, подкачка: добавить(r"\Memory\Pages Input/sec"),
            dpc: добавить(r"\Processor(_Total)\% DPC Time"), прерывания: добавить(r"\Processor(_Total)\% Interrupt Time"),
            диск_простой: добавить(r"\PhysicalDisk(_Total)\% Idle Time") };
        unsafe { PdhCollectQueryData(з) };
        Some(с)
    }
    /// (подкачка стр/с, прерывания + отложенные вызовы %, диск занят %) за время с прошлого сбора.
    pub fn снять(&self) -> (Option<f64>, Option<f64>, Option<f64>) {
        if unsafe { PdhCollectQueryData(self.запрос) } != 0 { return (None, None, None); }
        let взять = |с: Ручка| -> Option<f64> {
            if с.is_null() { return None; }
            let mut з = Значение { статус: 0, _п: 0, число: 0.0 };
            // PDH_FMT_DOUBLE | PDH_FMT_NOCAP100
            (unsafe { PdhGetFormattedCounterValue(с, 0x200 | 0x8000, std::ptr::null_mut(), &mut з) } == 0).then_some(з.число)
        };
        let драйверы = match (взять(self.dpc), взять(self.прерывания)) { (Some(а), Some(б)) => Some(а + б), _ => None };
        let диск = взять(self.диск_простой).map(|п| (100.0 - п).clamp(0.0, 100.0));
        (взять(self.подкачка), драйверы, диск)
    }
}

// ───────────────────────────── 🟥 25.10 (Р-194): для лечения ─────────────────────────────

/// Сколько секунд он не трогал мышь и клавиатуру.
pub fn простой_ввода_с() -> u64 {
    let mut в = ПоследнийВвод { размер: std::mem::size_of::<ПоследнийВвод>() as u32, когда: 0 };
    if unsafe { GetLastInputInfo(&mut в) } == 0 { return u64::MAX; }
    (unsafe { GetTickCount() }.wrapping_sub(в.когда) / 1000) as u64
}

/// Процесс окна на переднем плане.
pub fn передний_pid() -> Option<u32> {
    let окно = unsafe { GetForegroundWindow() };
    if окно == 0 { return None; }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(окно, &mut pid) };
    (pid != 0).then_some(pid)
}

/// Сеанс процесса (0 — службы Windows).
pub fn сеанс(pid: u32) -> Option<u32> {
    let mut с = 0u32;
    (unsafe { ProcessIdToSessionId(pid, &mut с) } != 0).then_some(с)
}

/// Время создания процесса (FILETIME) — номер мог достаться другому процессу: сверяем перед возвратом.
pub fn создан(pid: u32) -> Option<u64> {
    // PROCESS_QUERY_LIMITED_INFORMATION
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h == 0 { return None; }
    let (mut с, mut к, mut я, mut п) = (0u64, 0u64, 0u64, 0u64);
    let ок = unsafe { GetProcessTimes(h, &mut с, &mut к, &mut я, &mut п) } != 0;
    unsafe { CloseHandle(h) };
    ок.then_some(с)
}

/// Выжать рабочий набор процесса: его страницы уходят из ОЗУ (в список ожидания и файл подкачки), нужные он вернёт сам
/// при обращении. Ничего не ломает и не теряет. `false` — не открыть или отказ.
pub fn выжать(pid: u32) -> bool {
    // PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION
    let h = unsafe { OpenProcess(0x0100 | 0x0400, 0, pid) };
    if h == 0 { return false; }
    let ок = unsafe { K32EmptyWorkingSet(h) } != 0;
    unsafe { CloseHandle(h) };
    ок
}

type СпроситьПроцесс = unsafe extern "system" fn(isize, u32, *mut c_void, u32, *mut u32) -> i32;
type ЗадатьПроцесс = unsafe extern "system" fn(isize, u32, *const c_void, u32) -> i32;

/// Очередь диска процесса (ProcessIoPriority: 0 — очень низко, 1 — низко, 2 — обычно): (была, стала); `класс` None —
/// только прочесть. `None` — не открыть или отказ.
pub fn очередь_диска(pid: u32, класс: Option<u32>) -> Option<(u32, u32)> {
    let спросить: СпроситьПроцесс = unsafe { std::mem::transmute::<*const c_void, СпроситьПроцесс>(
        функция("ntdll.dll", b"NtQueryInformationProcess\0")?) };
    let задать: ЗадатьПроцесс = unsafe { std::mem::transmute::<*const c_void, ЗадатьПроцесс>(
        функция("ntdll.dll", b"NtSetInformationProcess\0")?) };
    // PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION
    let h = unsafe { OpenProcess(0x0400 | 0x0200, 0, pid) };
    if h == 0 { return None; }
    let mut была: u32 = 0;
    let mut итог = None;
    // ProcessIoPriority = 33
    if unsafe { спросить(h, 33, &mut была as *mut u32 as *mut c_void, 4, std::ptr::null_mut()) } >= 0 {
        итог = match класс {
            None => Some((была, была)),
            Some(к) if к == была => Some((была, была)),
            Some(к) => (unsafe { задать(h, 33, &к as *const u32 as *const c_void, 4) } >= 0).then_some((была, к)),
        };
    }
    unsafe { CloseHandle(h) };
    итог
}

/// 🟥 26.10: приоритет памяти процесса (ProcessMemoryPriority: 1 — очень низкий … 5 — обычный): его страницы в списке
/// ожидания Windows отдаёт другим первыми, и чтение огромных файлов не выталкивает из памяти экран и его программы.
/// (был, стал); `уровень` None — только прочесть. `None` — не открыть или отказ.
pub fn память_процесса(pid: u32, уровень: Option<u32>) -> Option<(u32, u32)> {
    // PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION
    let h = unsafe { OpenProcess(0x0400 | 0x0200, 0, pid) };
    if h == 0 { return None; }
    let mut был: u32 = 0;
    let mut итог = None;
    // ProcessMemoryPriority = 0, MEMORY_PRIORITY_INFORMATION — одно ULONG
    if unsafe { GetProcessInformation(h, 0, &mut был as *mut u32 as *mut c_void, 4) } != 0 {
        итог = match уровень {
            None => Some((был, был)),
            Some(у) if у == был => Some((был, был)),
            Some(у) => (unsafe { SetProcessInformation(h, 0, &у as *const u32 as *const c_void, 4) } != 0).then_some((был, у)),
        };
    }
    unsafe { CloseHandle(h) };
    итог
}

/// Полный путь программы процесса. `None` — не открыть.
pub fn путь_процесса(pid: u32) -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" { fn QueryFullProcessImageNameW(h: isize, флаги: u32, имя: *mut u16, длина: *mut u32) -> i32; }
    // PROCESS_QUERY_LIMITED_INFORMATION
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h == 0 { return None; }
    let mut буф = [0u16; 1024];
    let mut длина = буф.len() as u32;
    let ок = unsafe { QueryFullProcessImageNameW(h, 0, буф.as_mut_ptr(), &mut длина) } != 0;
    unsafe { CloseHandle(h) };
    ок.then(|| String::from_utf16_lossy(&буф[..длина as usize]))
}

/// Приоритет процессора — только прочесть.
pub fn приоритет_сейчас(pid: u32) -> Option<u32> {
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h == 0 { return None; }
    let к = unsafe { GetPriorityClass(h) };
    unsafe { CloseHandle(h) };
    (к != 0).then_some(к)
}

// ── видеокарта по процессам: счётчик `\GPU Engine(*)\Utilization Percentage` ──
#[repr(C)]
#[derive(Clone, Copy)]
struct ЗначениеМ { статус: u32, число: f64 }
/// PDH_FMT_COUNTERVALUE_ITEM_W: имя экземпляра (строка в том же буфере) и значение — 24 байта на x64.
#[repr(C)]
#[derive(Clone, Copy)]
struct Элемент { имя: *const u16, значение: ЗначениеМ }
#[link(name = "pdh")]
extern "system" {
    fn PdhGetFormattedCounterArrayW(счётчик: Ручка, формат: u32, байт: *mut u32, штук: *mut u32, элементы: *mut Элемент) -> u32;
}

/// Чистая (видно в тесте): номер процесса из имени экземпляра `pid_27884_luid_0x…_engtype_3D` и вид движка.
pub fn экземпляр(имя: &str) -> Option<(u32, &str)> {
    let хвост = имя.strip_prefix("pid_")?;
    let конец = хвост.find('_')?;
    let pid = хвост[..конец].parse().ok()?;
    let вид = имя.rsplit("engtype_").next().unwrap_or("");
    Some((pid, вид))
}

/// Доля видеокарты по процессам за время с прошлого сбора, %: счёт (3D и вычисления) и декодер видео — отдельно.
pub struct ВидеокартаПроцессов { запрос: Ручка, счётчик: Ручка }

/// Доли видеокарты по процессам. `экран` — 🟥 27.10: вся работа процесса (любые движки) на адаптере экрана: с ней спорит
/// работа экрана; 3D и вычисления NVIDIA экрану Intel не мешают так прямо.
#[derive(Clone, Debug, Default)]
pub struct Доли { pub счёт: std::collections::HashMap<u32, f64>, pub декодер: std::collections::HashMap<u32, f64>,
    pub экран: std::collections::HashMap<u32, f64> }

/// Чистая (видно в тесте): адаптер из имени экземпляра — `luid_0x…_0x…`.
pub fn адаптер_экземпляра(имя: &str) -> Option<&str> {
    let с = имя.find("luid_")?;
    let хвост = &имя[с..];
    let конец = хвост.find("_phys").unwrap_or(хвост.len());
    Some(&хвост[..конец])
}
impl ВидеокартаПроцессов {
    pub fn открыть() -> Option<ВидеокартаПроцессов> {
        let mut з: Ручка = std::ptr::null_mut();
        if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut з) } != 0 { return None; }
        let п = широкая(r"\GPU Engine(*)\Utilization Percentage");
        let mut с: Ручка = std::ptr::null_mut();
        if unsafe { PdhAddEnglishCounterW(з, п.as_ptr(), 0, &mut с) } != 0 { return None; }
        unsafe { PdhCollectQueryData(з) };
        Some(ВидеокартаПроцессов { запрос: з, счётчик: с })
    }
    /// `экран` — адаптер экрана (`Видеокарта::адаптер`); `None` — доля экрана не считается.
    pub fn снять(&self, экран: Option<&str>) -> Доли {
        let mut итог = Доли::default();
        if unsafe { PdhCollectQueryData(self.запрос) } != 0 { return итог; }
        // PDH_FMT_DOUBLE | PDH_FMT_NOCAP100
        const ФОРМАТ: u32 = 0x200 | 0x8000;
        for _ in 0..3 {
            let (mut байт, mut штук) = (0u32, 0u32);
            let к = unsafe { PdhGetFormattedCounterArrayW(self.счётчик, ФОРМАТ, &mut байт, &mut штук, std::ptr::null_mut()) };
            // PDH_MORE_DATA
            if к != 0x8000_07D2 { return итог; }
            let mut буфер: Vec<u64> = vec![0; (байт as usize * 2) / 8 + 1];
            let к = unsafe { PdhGetFormattedCounterArrayW(self.счётчик, ФОРМАТ, &mut байт, &mut штук,
                буфер.as_mut_ptr() as *mut Элемент) };
            if к == 0x8000_07D2 { continue; }
            if к != 0 || штук as usize * std::mem::size_of::<Элемент>() > буфер.len() * 8 { return итог; }
            let начало = буфер.as_ptr() as usize;
            let конец = начало + буфер.len() * 8;
            let элементы = unsafe { std::slice::from_raw_parts(буфер.as_ptr() as *const Элемент, штук as usize) };
            for э in элементы {
                if э.значение.статус > 1 { continue; }
                let имя = unsafe { строка(э.имя, начало, конец) };
                let Some((pid, вид)) = экземпляр(&имя) else { continue };
                // счётчик пишет цифры адаптера заглавными (`0x00010F5C`), DXGI — как сложим: сравниваем без регистра
                if экран.is_some_and(|э| адаптер_экземпляра(&имя).is_some_and(|а| а.eq_ignore_ascii_case(э))) {
                    *итог.экран.entry(pid).or_insert(0.0) += э.значение.число;
                }
                if вид.starts_with("3D") || вид.starts_with("Compute") {
                    *итог.счёт.entry(pid).or_insert(0.0) += э.значение.число;
                } else if вид.starts_with("VideoDecode") {
                    *итог.декодер.entry(pid).or_insert(0.0) += э.значение.число;
                }
            }
            return итог;
        }
        итог
    }
}

/// Строка UTF-16 до нуля — только если указатель смотрит внутрь нашего буфера.
unsafe fn строка(p: *const u16, начало: usize, конец: usize) -> String {
    let адрес = p as usize;
    if p.is_null() || адрес < начало || адрес >= конец || адрес % 2 != 0 { return String::new(); }
    let предел = (конец - адрес) / 2;
    let mut n = 0;
    while n < предел && *p.add(n) != 0 { n += 1; }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn экземпляр_видеокарты() {
        assert_eq!(экземпляр("pid_27884_luid_0x00000000_0x0000D1A4_phys_0_eng_0_engtype_3D"), Some((27884, "3D")));
        assert_eq!(экземпляр("pid_4_luid_0x0_0x1_phys_0_eng_7_engtype_Compute 0"), Some((4, "Compute 0")));
        assert_eq!(экземпляр("_Total"), None);
        assert_eq!(адаптер_экземпляра("pid_27884_luid_0x00000000_0x00010f5c_phys_0_eng_0_engtype_3D"),
            Some("luid_0x00000000_0x00010f5c"));
        assert_eq!(адаптер_экземпляра("_Total"), None);
    }

    /// Живая проба на своём же процессе: приоритет, очередь диска, рабочий набор, время создания (без чужих программ).
    #[test]
    #[ignore]
    fn живьём_свой_процесс() {
        let я = std::process::id();
        assert!(создан(я).is_some());
        let был = приоритет_сейчас(я).expect("приоритет");
        assert_eq!(приоритет(я, 0x4000).map(|(_, с)| с), Some(0x4000));
        assert_eq!(приоритет(я, был).map(|(_, с)| с), Some(был));
        let (д0, _) = очередь_диска(я, None).expect("очередь диска");
        assert_eq!(очередь_диска(я, Some(0)).map(|(_, с)| с), Some(0));
        assert_eq!(очередь_диска(я, Some(д0)).map(|(_, с)| с), Some(д0));
        let (п0, _) = память_процесса(я, None).expect("приоритет памяти");
        assert_eq!(память_процесса(я, Some(1)).map(|(_, с)| с), Some(1));
        assert_eq!(память_процесса(я, None).map(|(б, _)| б), Some(1), "Windows приняла «очень низкий»");
        assert_eq!(память_процесса(я, Some(п0)).map(|(_, с)| с), Some(п0));
        let mut буф = vec![0u8; 2 << 20];
        let до = снимок(&mut буф).and_then(|с| с.get(&я).map(|п| п.рабочий)).unwrap_or(0);
        assert!(выжать(я));
        let после = снимок(&mut буф).and_then(|с| с.get(&я).map(|п| п.рабочий)).unwrap_or(u64::MAX);
        assert!(после < до, "рабочий набор {до} → {после}");
        let в = ВидеокартаПроцессов::открыть().expect("счётчик видеокарты");
        std::thread::sleep(Duration::from_millis(500));
        let экран = Видеокарта::открыть().expect("видеокарта экрана").адаптер().expect("адаптер экрана");
        let доли = в.снять(Some(&экран));
        eprintln!("экран {экран}; видеокарта по процессам: {:?}; декодер: {:?}; экран: {:?}", доли.счёт, доли.декодер, доли.экран);
        assert!(доли.экран.keys().any(|pid| *pid != 0), "на адаптере экрана кто-то работает (dwm)");
        assert!(простой_ввода_с() < u64::MAX);
    }
}

// ───────────────────────────── 🟥 25.10 (Р-195): уведомление Windows — когда БАРСа нет ─────────────────────────────

#[link(name = "advapi32")]
extern "system" {
    fn RegCreateKeyExW(ключ: isize, путь: *const u16, _р: u32, _к: *const u16, оп: u32, доступ: u32, _а: *const c_void,
                       итог: *mut isize, было: *mut u32) -> i32;
    fn RegSetValueExW(ключ: isize, имя: *const u16, _р: u32, тип: u32, данные: *const u8, байт: u32) -> i32;
    fn RegCloseKey(ключ: isize) -> i32;
}

/// Имя отправителя уведомлений — «Механик» (`HKCU\Software\Classes\AppUserModelId\Mekhanik`, без прав администратора).
pub const ОТПРАВИТЕЛЬ: &str = "Mekhanik";

pub fn зарегистрировать_отправителя() -> bool {
    // HKEY_CURRENT_USER
    let hkcu: isize = 0x8000_0001u32 as i32 as isize;
    let путь = широкая(&format!(r"Software\Classes\AppUserModelId\{ОТПРАВИТЕЛЬ}"));
    let mut к: isize = 0;
    // KEY_WRITE
    if unsafe { RegCreateKeyExW(hkcu, путь.as_ptr(), 0, std::ptr::null(), 0, 0x20006, std::ptr::null(), &mut к, std::ptr::null_mut()) } != 0 {
        return false;
    }
    let имя = широкая("DisplayName");
    let значение = широкая("Механик");
    // REG_SZ = 1; длина в байтах вместе с нулём
    let ок = unsafe { RegSetValueExW(к, имя.as_ptr(), 0, 1, значение.as_ptr() as *const u8, (значение.len() * 2) as u32) } == 0;
    unsafe { RegCloseKey(к) };
    ок
}

/// Чистая (видно в тесте): текст для XML уведомления.
pub fn xml_текст(с: &str) -> String {
    с.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Показать уведомление Windows (центр уведомлений, как у любой программы): PowerShell без окна, скрипт — base64 UTF-16,
/// чтобы русские буквы и кавычки дошли целыми. Ничего не ждём.
pub fn уведомить(заголовок: &str, текст: &str) -> bool { команда_уведомления(заголовок, текст).spawn().is_ok() }

fn команда_уведомления(заголовок: &str, текст: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let xml = format!("<toast><visual><binding template='ToastGeneric'><text>{}</text><text>{}</text></binding></visual></toast>",
        xml_текст(заголовок), xml_текст(текст));
    let скрипт = format!("$ProgressPreference = 'SilentlyContinue'; \
        [Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
        [Windows.Data.Xml.Dom.XmlDocument, Windows.Data.Xml.Dom.XmlDocument, ContentType = WindowsRuntime] > $null; \
        $x = New-Object Windows.Data.Xml.Dom.XmlDocument; $x.LoadXml(\"{}\"); \
        [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{ОТПРАВИТЕЛЬ}').Show((New-Object Windows.UI.Notifications.ToastNotification $x))",
        xml.replace('"', "`\""));
    let байты: Vec<u8> = скрипт.encode_utf16().flat_map(|с| с.to_le_bytes()).collect();
    let закодировано = base64(&байты);
    let mut к = std::process::Command::new("powershell");
    // CREATE_NO_WINDOW
    к.args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &закодировано]).creation_flags(0x0800_0000);
    к
}

/// Base64 без зависимостей.
fn base64(д: &[u8]) -> String {
    const А: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut в = String::with_capacity(д.len().div_ceil(3) * 4);
    for ч in д.chunks(3) {
        let н = (ч[0] as u32) << 16 | (*ч.get(1).unwrap_or(&0) as u32) << 8 | *ч.get(2).unwrap_or(&0) as u32;
        в.push(А[(н >> 18) as usize & 63] as char);
        в.push(А[(н >> 12) as usize & 63] as char);
        в.push(if ч.len() > 1 { А[(н >> 6) as usize & 63] as char } else { '=' });
        в.push(if ч.len() > 2 { А[н as usize & 63] as char } else { '=' });
    }
    в
}

#[cfg(test)]
mod тесты_уведомления {
    use super::*;

    #[test]
    fn base64_как_у_всех() {
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b""), "");
    }

    /// Живая проба: одно уведомление «Механик» в центре уведомлений Windows (PowerShell отработал без ошибок).
    #[test]
    #[ignore]
    fn живьём_уведомление() {
        assert!(зарегистрировать_отправителя());
        let в = команда_уведомления("Механик: проверка уведомлений",
            "Так «Механик» скажет сам, если БАРСа на ПК не будет. Ничего делать не нужно.").output().expect("powershell");
        let ошибки = String::from_utf8_lossy(&в.stderr);
        // PowerShell пишет в поток ошибок и ход подготовки модулей (CLIXML «progress») — ошибка только `S="Error"`
        assert!(в.status.success() && !ошибки.contains("S=\"Error\""), "{ошибки}");
    }

    #[test]
    fn xml_без_разметки() {
        assert_eq!(xml_текст("a<b>&\"c'"), "a&lt;b&gt;&amp;&quot;c&apos;");
    }
}
