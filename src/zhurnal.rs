//! 🟥 13.11 (волна 2а, «Механик» 1.0) ЖУРНАЛ v2 — ОДИН ПОТОК ЗАПИСИ. Разбор 25.10–13.11 (`otladka_svyaz.md` §3.6, §3.15,
//! `zhurnal.md` §12): строки писались прямо из шага замера (250 мс) — под занятым диском замер ждал диск ровно тогда,
//! когда диск и был причиной; один файл с одним `.old` — около 19.11 пропало бы всё до 08.11; переворот файла шёл не под
//! замком, а пишут два потока. Теперь:
//! - всё дисковое связи (строки журнала, сообщения БАРСу, `barsu.json`, память «уже сказано») — заданиями в ограниченную
//!   очередь (`ОЧЕРЕДЬ`); пишет один поток `zhurnal` с низкой очередью диска (диск Артура важнее журнала). Очередь полна
//!   — строка теряется со счётом (первой строкой, когда поток догонит, — `журнал_потеряно`): замер не ждёт диск никогда;
//! - файлы по дням `zhurnal\ГГГГ-ММ-ДД.jsonl` (день — местный, по часам ПК) хранятся `ХРАНИТЬ_ДНЕЙ`; сводка каждого
//!   прошедшего дня — строкой в `svodka.jsonl` (не стирается никогда);
//! - старый `zhurnal.jsonl` и `zhurnal.jsonl.old` при первом запуске переносятся в `zhurnal\staroe\` (переименованием —
//!   ничего не переписывается) и раскладываются по дням: история с 25.10 не теряется.
//!
//! Строка: `{"ts": местное время с мс и смещением, "ms": мс эпохи UTC (сортировать — по нему: 30.10–03.11 часы ПК шли
//! по −4/−5 ч), "event", "data", "lvl"}`; `lvl` — куда ещё ушло: 0 — только свой журнал, 1 — и журнал БАРСа, 2 —
//! сигнал разуму, 3 — уведомление Windows; `cut` — почему не ушло дальше (`budget` — дневной бюджет журнала БАРСа,
//! `bez_nego` — тормоз без него, `uroven` — шум, БАРСу не нужен, `proba` — стенд, `net_barsa` — БАРСа нет).

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Сколько дней хранить сырой журнал (и перенесённый старый). Сводка дня — навсегда.
pub const ХРАНИТЬ_ДНЕЙ: u64 = 60;
/// Заданий в очереди не больше — дальше строки теряются (со счётом), замер не ждёт.
pub const ОЧЕРЕДЬ: usize = 4096;
/// Сырой журнал всех дней вместе больше — стираются самые старые дни (09.11 на C было меньше 1 ГБ).
const ВСЕГО_МАКС_БАЙТ: u64 = 500 << 20;
/// Без заданий поток просыпается так часто: подмести просроченные сообщения БАРСу, проверить смену дня.
const ТИК: Duration = Duration::from_secs(5);
/// Имена старого журнала (0.1–0.5) — в корне папки, от старого к новому.
const СТАРЫЕ: [&str; 2] = ["zhurnal.jsonl.old", "zhurnal.jsonl"];
/// Метка «разложено по дням» рядом с перенесённым старым файлом.
const МЕТКА: &str = ".razlozhen";

/// Задание потоку записи.
pub enum Задание {
    /// Строка журнала (с `\n`) — в файл дня `день`.
    Строка { день: String, текст: String },
    /// Файл целиком: временный → подмена (`надёжно` — ещё и сброс на диск до подмены).
    Файл { путь: PathBuf, текст: String, надёжно: bool },
    /// Стереть файл (нет его — не беда).
    Стереть(PathBuf),
    /// Дослать всё, что встало в очередь раньше, и ответить.
    Дослать(SyncSender<()>),
}

static ОЧЕРЕДЬ_ЗАПИСИ: OnceLock<SyncSender<Задание>> = OnceLock::new();
/// Строк журнала потеряно (очередь была полна) — поток запишет счёт первой строкой, когда догонит.
static ПОТЕРЯНО: AtomicU64 = AtomicU64::new(0);

/// Папка журнала по дням.
pub fn папка_журнала(база: &Path) -> PathBuf { база.join("zhurnal") }
/// Файл дня.
pub fn файл_дня(база: &Path, день: &str) -> PathBuf { папка_журнала(база).join(format!("{день}.jsonl")) }
/// Сводка по дням (не стирается).
pub fn файл_сводки(база: &Path) -> PathBuf { база.join("svodka.jsonl") }

/// Сегодня по часам ПК — «ГГГГ-ММ-ДД».
pub fn сегодня() -> String { chrono::Local::now().format("%Y-%m-%d").to_string() }

/// Готовая строка журнала: (день, текст с `\n`). Чистая, кроме часов.
pub fn строка(событие: &str, данные: &Value, уровень: u8, срезано: Option<&str>) -> (String, String) {
    let т = chrono::Local::now();
    let mut о = json!({ "ts": т.to_rfc3339_opts(chrono::SecondsFormat::Millis, false), "ms": т.timestamp_millis().max(0),
        "event": событие, "data": данные, "lvl": уровень });
    if let Some(с) = срезано { о["cut"] = json!(с); }
    (т.format("%Y-%m-%d").to_string(), format!("{о}\n"))
}

/// Чистая (видно в тесте): день строки — по её `ts` («2026-11-13T22:03:00.123+00:00» → «2026-11-13»).
pub fn день_строки(с: &str) -> Option<&str> {
    let н = с.find("\"ts\":\"")? + 6;
    let д = с.get(н..н + 10)?;
    это_день(д).then_some(д)
}

/// Чистая: «ГГГГ-ММ-ДД».
pub fn это_день(д: &str) -> bool {
    let б = д.as_bytes();
    б.len() == 10 && б[4] == b'-' && б[7] == b'-'
        && б.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn очередь() -> &'static SyncSender<Задание> {
    ОЧЕРЕДЬ_ЗАПИСИ.get_or_init(|| {
        let (к, приём) = sync_channel(ОЧЕРЕДЬ);
        let база = crate::svyaz::папка();
        // не поднялся поток — приёмник уйдёт вместе с замыканием, и задания пойдут сразу (`сам`)
        let _ = std::thread::Builder::new().name("zhurnal".into()).spawn(move || поток(приём, база));
        к
    })
}

/// Строку журнала — в очередь. Полна — строка теряется со счётом (замер не ждёт диск); потока нет — пишем сами.
pub fn в_журнал(день: String, текст: String) {
    match очередь().try_send(Задание::Строка { день, текст }) {
        Ok(()) => {}
        Err(TrySendError::Full(_)) => { ПОТЕРЯНО.fetch_add(1, Ordering::Relaxed); }
        Err(TrySendError::Disconnected(з)) => сам(з),
    }
}

/// Задание — в очередь; полна — пропустить (`false`): так пишется то, что через 2 с перепишется снова (`barsu.json`).
pub fn потом(з: Задание) -> bool {
    match очередь().try_send(з) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Disconnected(з)) => { сам(з); true }
    }
}

/// Важное задание (сигнал разуму, просьба, память «уже сказано»): в очередь, а полна — сделать сразу самому.
pub fn важное(з: Задание) {
    match очередь().try_send(з) {
        Ok(()) => {}
        Err(TrySendError::Full(з)) | Err(TrySendError::Disconnected(з)) => сам(з),
    }
}

/// Сделать задание самому, мимо потока (поток не поднялся или очередь полна, а задание важное).
fn сам(з: Задание) {
    match з {
        Задание::Строка { день, текст } => { let _ = дописать(&crate::svyaz::папка(), &день, &текст); }
        Задание::Файл { путь, текст, надёжно } => {
            if let Some(р) = путь.parent() { let _ = std::fs::create_dir_all(р); }
            let _ = if надёжно { crate::svyaz::записать_надёжно(&путь, &текст) } else { crate::svyaz::записать(&путь, &текст) };
        }
        Задание::Стереть(п) => { let _ = std::fs::remove_file(п); }
        Задание::Дослать(о) => { let _ = о.try_send(()); }
    }
}

/// Дождаться, пока поток запишет всё, что в очереди сейчас (выход, падение): не дольше `предел`.
pub fn дослать(предел: Duration) -> bool {
    let до = Instant::now() + предел;
    let (к, ответ) = sync_channel(1);
    let mut з = Задание::Дослать(к);
    loop {
        match очередь().try_send(з) {
            Ok(()) => break,
            Err(TrySendError::Full(в)) => {
                if Instant::now() >= до { return false; }
                з = в;
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(TrySendError::Disconnected(_)) => return true,
        }
    }
    ответ.recv_timeout(до.saturating_duration_since(Instant::now())).is_ok()
}

/// Дописать строку в файл дня — одной записью (пишут и поток, и при полной очереди важное — сам).
pub fn дописать(база: &Path, день: &str, текст: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(папка_журнала(база))?;
    OpenOptions::new().create(true).append(true).open(файл_дня(база, день))?.write_all(текст.as_bytes())
}

// ───────────────────────────── поток записи ─────────────────────────────

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThread() -> isize;
    fn GetModuleHandleW(имя: *const u16) -> isize;
    fn GetProcAddress(модуль: isize, имя: *const u8) -> *const c_void;
    fn CreateMutexW(атрибуты: *const c_void, взять: i32, имя: *const u16) -> isize;
    fn WaitForSingleObject(h: isize, мс: u32) -> u32;
    fn ReleaseMutex(h: isize) -> i32;
    fn CloseHandle(h: isize) -> i32;
}

/// Обслуживать журнал (перенос старого, сводки, чистка) — одному процессу на папку: журнал один, а писать в него
/// могут и мозг, и окно (`--okno`). Занято другим — пропустить (сделает он); замка не создать — сами.
fn обслуживать_одному<T>(база: &Path, ф: impl FnOnce() -> T) -> Option<T> {
    // имя — по пути папки (у каждой пробы своя папка — свой замок)
    let mut х: u64 = 0xcbf2_9ce4_8422_2325;
    for б in база.to_string_lossy().to_lowercase().bytes() { х = (х ^ б as u64).wrapping_mul(0x0100_0000_01b3); }
    let имя: Vec<u16> = format!(r"Local\mekhanik-zhurnal-{х:016x}").encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe { CreateMutexW(std::ptr::null(), 0, имя.as_ptr()) };
    if h == 0 { return Some(ф()); }
    // WAIT_OBJECT_0 и WAIT_ABANDONED (прежний хозяин упал) — замок наш
    let ответ = unsafe { WaitForSingleObject(h, 0) };
    let итог = if ответ == 0 || ответ == 0x80 {
        let и = ф();
        unsafe { ReleaseMutex(h) };
        Some(и)
    } else { None };
    unsafe { CloseHandle(h) };
    итог
}

/// Своему потоку — низкая очередь диска (`NtSetInformationThread`, ThreadIoPriority = 22, 1 — низкая): процессор у
/// потока обычный (строки не копятся под занятым процессором), а диск Артура важнее журнала.
fn понизить_свой_диск() -> bool {
    type Задать = unsafe extern "system" fn(isize, u32, *const c_void, u32) -> i32;
    let имя: Vec<u16> = "ntdll.dll".encode_utf16().chain(std::iter::once(0)).collect();
    let м = unsafe { GetModuleHandleW(имя.as_ptr()) };
    if м == 0 { return false; }
    let ф = unsafe { GetProcAddress(м, b"NtSetInformationThread\0".as_ptr()) };
    if ф.is_null() { return false; }
    let задать: Задать = unsafe { std::mem::transmute::<*const c_void, Задать>(ф) };
    let низкая: u32 = 1;
    unsafe { задать(GetCurrentThread(), 22, &низкая as *const u32 as *const c_void, 4) >= 0 }
}

fn поток(приём: Receiver<Задание>, база: PathBuf) {
    let понижен = понизить_свой_диск();
    let mut первый = true;
    loop {
        let итог = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| круги(&приём, &база, понижен, первый)));
        первый = false;
        match итог {
            Ok(()) => return,
            Err(_) => {
                // упал сам писарь — строка мимо очереди, и снова (очередь цела: приёмник у нас)
                let (день, текст) = строка("журнал_поток_упал", &json!({ "смысл": "поток записи журнала упал — поднят снова" }), 0, None);
                let _ = дописать(&база, &день, &текст);
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }
}

fn круги(приём: &Receiver<Задание>, база: &Path, понижен: bool, первый: bool) {
    let mut п = Писарь::новый(база.to_path_buf());
    let перенесено = п.начать(&сегодня());
    if первый {
        let (день, текст) = строка("журнал_v2", &json!({ "папка": "zhurnal\\ГГГГ-ММ-ДД.jsonl", "хранить_дней": ХРАНИТЬ_ДНЕЙ,
            "сводка": "svodka.jsonl", "очередь": ОЧЕРЕДЬ, "диск_потока_низкий": понижен, "перенесено_строк": перенесено.0,
            "перенесено_дней": перенесено.1 }), 0, None);
        п.строка(&день, &текст);
    }
    let mut тик = Instant::now();
    loop {
        match приём.recv_timeout(ТИК) {
            Ok(з) => п.принять(з),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if тик.elapsed() >= ТИК {
            тик = Instant::now();
            п.проверить_день(&сегодня());
            // просроченные просьбы и сигналы БАРСу — подмести здесь же (всё дисковое связи — в этом потоке)
            crate::svyaz::подмести();
        }
    }
}

/// Писарь: файл дня открыт, пока день не сменился; смена дня — сводка прошедших и чистка старых.
pub struct Писарь {
    база: PathBuf,
    день: String,
    файл: Option<File>,
    /// По какой день сделаны сводки и чистка.
    обслужен: String,
    /// Счёт потерянных строк (у потока — общий `ПОТЕРЯНО`, у пробы — свой).
    потеряно: &'static AtomicU64,
    ошибок: u64,
}

impl Писарь {
    pub fn новый(база: PathBuf) -> Писарь { Писарь::со_счётом(база, &ПОТЕРЯНО) }

    pub fn со_счётом(база: PathBuf, потеряно: &'static AtomicU64) -> Писарь {
        Писарь { база, день: String::new(), файл: None, обслужен: String::new(), потеряно, ошибок: 0 }
    }

    fn папка(&self) -> PathBuf { папка_журнала(&self.база) }

    /// Начало: перенести старый журнал, сводки прошедших дней, чистка. Вернуть (строк перенесено, дней).
    pub fn начать(&mut self, сегодня: &str) -> (u64, usize) {
        let _ = std::fs::create_dir_all(self.папка());
        let база = self.база.clone();
        let итог = обслуживать_одному(&база, || {
            let перенесено = self.перенести_старый();
            self.обслужить(сегодня);
            перенесено
        });
        // обслуживает другой процесс — день считаем обслуженным (завтра — снова попробуем)
        if итог.is_none() { self.обслужен = сегодня.to_string(); }
        итог.unwrap_or((0, 0))
    }

    /// День сменился (или ещё не обслужен) — сводки и чистка.
    pub fn проверить_день(&mut self, сегодня: &str) {
        if сегодня <= self.обслужен.as_str() { return; }
        let база = self.база.clone();
        if обслуживать_одному(&база, || self.обслужить(сегодня)).is_none() { self.обслужен = сегодня.to_string(); }
    }

    fn обслужить(&mut self, сегодня: &str) {
        self.сводки(сегодня);
        self.чистить(сегодня);
        if self.ошибок > 0 && !self.обслужен.is_empty() {
            // запись не удавалась (диск полон, файл отняли) — счёт за прошедший день, первой строкой нового
            let (_, т) = строка("журнал_ошибки_записи", &json!({ "раз": self.ошибок, "смысл": "запись журнала или файлов связи                 не удавалась (диск полон или файл занят)" }), 0, None);
            self.в_файл(сегодня, &т);
            self.ошибок = 0;
        }
        self.обслужен = сегодня.to_string();
    }

    pub fn принять(&mut self, з: Задание) {
        match з {
            Задание::Строка { день, текст } => self.строка(&день, &текст),
            Задание::Файл { путь, текст, надёжно } => {
                if let Some(р) = путь.parent() { let _ = std::fs::create_dir_all(р); }
                let итог = if надёжно { crate::svyaz::записать_надёжно(&путь, &текст) } else { crate::svyaz::записать(&путь, &текст) };
                if итог.is_err() { self.ошибок += 1; }
            }
            Задание::Стереть(п) => { let _ = std::fs::remove_file(п); }
            Задание::Дослать(о) => { let _ = о.try_send(()); }
        }
    }

    /// Строка в файл её дня. Строка прошлого дня (два потока у полуночи) — дописать туда, файл дня не меняя.
    pub fn строка(&mut self, день: &str, текст: &str) {
        let потеряно = self.потеряно.swap(0, Ordering::Relaxed);
        if потеряно > 0 {
            // счёт — перед строкой, что пришла следом, в её же день
            let (_, т) = строка("журнал_потеряно", &json!({ "строк": потеряно,
                "смысл": "очередь записи была полна (диск занят) — столько строк не записано: замер диск не ждёт" }), 0, None);
            self.в_файл(день, &т);
        }
        self.в_файл(день, текст);
    }

    fn в_файл(&mut self, день: &str, текст: &str) {
        if !self.день.is_empty() && день < self.день.as_str() {
            if дописать(&self.база, день, текст).is_err() { self.ошибок += 1; }
            return;
        }
        if день != self.день || self.файл.is_none() {
            self.файл = None;
            let _ = std::fs::create_dir_all(self.папка());
            self.файл = OpenOptions::new().create(true).append(true).open(файл_дня(&self.база, день)).ok();
            self.день = день.to_string();
        }
        let ок = self.файл.as_mut().is_some_and(|ф| ф.write_all(текст.as_bytes()).is_ok());
        if !ок {
            // диск полон или файл отняли — открыть заново со следующей строкой
            self.ошибок += 1;
            self.файл = None;
        }
    }

    /// Старый журнал (0.1–0.5): файлы из корня — в `zhurnal\staroe\` переименованием, строки — по дням. Каждый
    /// перенесённый файл раскладывается один раз (метка `.razlozhen`); раскладка прервалась — при следующем старте
    /// снова, а куски, что уже дописаны, узнаются по первой строке и второй раз не пишутся. (строк, дней).
    pub fn перенести_старый(&mut self) -> (u64, usize) {
        let старое = self.папка().join("staroe");
        let мс = crate::svyaz::сейчас_мс();
        for (к, имя) in СТАРЫЕ.iter().enumerate() {
            let путь = self.база.join(имя);
            if !путь.is_file() { continue; }
            if std::fs::create_dir_all(&старое).is_err() { return (0, 0); }
            // файл занят (читают без права на удаление) — в следующий запуск
            let _ = std::fs::rename(&путь, старое.join(format!("{мс}-{к}-{имя}")));
        }
        let Ok(д) = std::fs::read_dir(&старое) else { return (0, 0) };
        let mut файлы: Vec<PathBuf> = д.filter_map(Result::ok).map(|ф| ф.path())
            .filter(|п| п.is_file() && !п.to_string_lossy().ends_with(МЕТКА)).collect();
        файлы.sort();
        let (mut строк, mut дни) = (0u64, BTreeSet::new());
        for ф in файлы {
            let метка = PathBuf::from(format!("{}{МЕТКА}", ф.display()));
            if метка.exists() { continue; }
            if let Ok((с, д)) = self.разложить(&ф) {
                строк += с;
                дни.extend(д);
                let _ = crate::svyaz::записать(&метка, "");
            }
        }
        (строк, дни.len())
    }

    /// Разложить один старый файл по дням; вернуть (строк, дни).
    fn разложить(&mut self, файл: &Path) -> std::io::Result<(u64, BTreeSet<String>)> {
        let байты = std::fs::read(файл)?;
        let текст = String::from_utf8_lossy(&байты);
        // строка без даты — в день предыдущей (а первая — в день записи файла)
        let запасной = std::fs::metadata(файл).and_then(|м| м.modified()).ok()
            .map(|т| chrono::DateTime::<chrono::Local>::from(т).format("%Y-%m-%d").to_string()).unwrap_or_else(сегодня);
        let куски = куски_по_дням(&текст, &запасной);
        let mut строк = 0u64;
        let mut дни = BTreeSet::new();
        for (день, кусок) in &куски {
            let цель = файл_дня(&self.база, день);
            let первая = кусок.lines().next().unwrap_or("");
            let уже = !первая.is_empty() && std::fs::read(&цель).ok()
                .is_some_and(|б| String::from_utf8_lossy(&б).contains(первая));
            if !уже { дописать(&self.база, день, кусок)?; }
            строк += кусок.lines().count() as u64;
            дни.insert(день.clone());
        }
        Ok((строк, дни))
    }

    /// Сводки прошедших дней, которых ещё нет в `svodka.jsonl` (дни — по файлам; сегодняшний — завтра).
    pub fn сводки(&mut self, сегодня: &str) {
        let путь = файл_сводки(&self.база);
        let есть: BTreeSet<String> = std::fs::read_to_string(&путь).unwrap_or_default().lines()
            .filter_map(|с| serde_json::from_str::<Value>(с).ok())
            .filter_map(|v| v.get("day").and_then(Value::as_str).map(str::to_string)).collect();
        let mut новые = String::new();
        for (день, файл) in self.дни() {
            if день.as_str() >= сегодня || есть.contains(&день) { continue; }
            let Ok(б) = std::fs::read(&файл) else { continue };
            let mut с = сводка_дня(&день, &String::from_utf8_lossy(&б));
            с["bytes"] = json!(б.len());
            новые.push_str(&format!("{с}\n"));
        }
        if новые.is_empty() { return; }
        let ок = OpenOptions::new().create(true).append(true).open(&путь).and_then(|mut ф| ф.write_all(новые.as_bytes()));
        if ок.is_err() { self.ошибок += 1; }
    }

    /// Дни журнала по файлам, по порядку.
    fn дни(&self) -> Vec<(String, PathBuf)> {
        let Ok(д) = std::fs::read_dir(self.папка()) else { return Vec::new() };
        let mut в: Vec<(String, PathBuf)> = д.filter_map(Result::ok).filter_map(|ф| {
            let имя = ф.file_name().to_string_lossy().to_string();
            let день = имя.strip_suffix(".jsonl")?.to_string();
            (это_день(&день) && ф.path().is_file()).then(|| (день, ф.path()))
        }).collect();
        в.sort();
        в
    }

    /// Чистка: дни старше `ХРАНИТЬ_ДНЕЙ`, перенесённый старый журнал старше того же (по времени записи), и самые
    /// старые дни, пока всё вместе больше `ВСЕГО_МАКС_БАЙТ` (сегодняшний не трогаем). Сводку — никогда.
    pub fn чистить(&mut self, сегодня: &str) {
        let Some(срок) = срок_хранения(сегодня) else { return };
        let mut живые = Vec::new();
        for (день, файл) in self.дни() {
            if день < срок { let _ = std::fs::remove_file(&файл); } else { живые.push((день, файл)); }
        }
        let mut всего: u64 = живые.iter().filter_map(|(_, ф)| std::fs::metadata(ф).ok()).map(|м| м.len()).sum();
        for (день, файл) in &живые {
            if всего <= ВСЕГО_МАКС_БАЙТ || день.as_str() >= сегодня { break; }
            let размер = std::fs::metadata(файл).map(|м| м.len()).unwrap_or(0);
            if std::fs::remove_file(файл).is_ok() { всего = всего.saturating_sub(размер); }
        }
        let старое = self.папка().join("staroe");
        let давно = Duration::from_secs(ХРАНИТЬ_ДНЕЙ * 24 * 3600);
        if let Ok(д) = std::fs::read_dir(&старое) {
            for ф in д.filter_map(Result::ok) {
                let стар = ф.metadata().and_then(|м| м.modified()).ok().and_then(|т| т.elapsed().ok()).is_some_and(|в| в > давно);
                if стар { let _ = std::fs::remove_file(ф.path()); }
            }
        }
    }
}

/// Чистая (видно в тесте): самый старый день, что ещё хранится (`сегодня` − `ХРАНИТЬ_ДНЕЙ`).
pub fn срок_хранения(сегодня: &str) -> Option<String> {
    let д = chrono::NaiveDate::parse_from_str(сегодня, "%Y-%m-%d").ok()?;
    Some(д.checked_sub_days(chrono::Days::new(ХРАНИТЬ_ДНЕЙ))?.format("%Y-%m-%d").to_string())
}

/// Чистая (видно в тесте): строки текста — куски подряд идущих строк одного дня, по порядку (день может вернуться:
/// 29.10 часы ПК перешли на −4 ч). Строка без даты — в день предыдущей, первая — в `запасной`.
pub fn куски_по_дням(текст: &str, запасной: &str) -> Vec<(String, String)> {
    let mut куски: Vec<(String, String)> = Vec::new();
    let mut был = запасной.to_string();
    for с in текст.lines() {
        if с.trim().is_empty() { continue; }
        let день = день_строки(с).map(str::to_string).unwrap_or_else(|| был.clone());
        был = день.clone();
        match куски.last_mut() {
            Some((д, т)) if *д == день => { т.push_str(с); т.push('\n'); }
            _ => куски.push((день, format!("{с}\n"))),
        }
    }
    куски
}

/// Чистая (видно в тесте): сводка дня по его строкам — сколько каких событий, тормоза (при нём и без него), лечение по
/// классам, сигналы разуму по отпечаткам, сколько ушло в журнал БАРСа и сколько срезал бюджет, сколько потеряно.
pub fn сводка_дня(день: &str, текст: &str) -> Value {
    let mut события: BTreeMap<String, u64> = BTreeMap::new();
    let mut лечение: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut сигналы: BTreeMap<String, u64> = BTreeMap::new();
    let (mut строк, mut битых, mut барсу, mut задавлено, mut потеряно, mut не_послано) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut тормозов, mut при_нём, mut без_него) = (0u64, 0u64, 0u64);
    let (mut первая, mut последняя): (Option<String>, Option<String>) = (None, None);
    for с in текст.lines() {
        if с.trim().is_empty() { continue; }
        let Ok(v) = serde_json::from_str::<Value>(с) else { битых += 1; continue };
        строк += 1;
        let событие = v.get("event").and_then(Value::as_str).unwrap_or("?").to_string();
        *события.entry(событие.clone()).or_default() += 1;
        if let Some(ts) = v.get("ts").and_then(Value::as_str) {
            if первая.is_none() { первая = Some(ts.to_string()); }
            последняя = Some(ts.to_string());
        }
        if v.get("lvl").and_then(Value::as_u64) == Some(1) { барсу += 1; }
        if v.get("cut").and_then(Value::as_str) == Some("budget") { задавлено += 1; }
        let д = v.get("data");
        let поле = |к: &str| д.and_then(|д| д.get(к));
        match событие.as_str() {
            "тормоз_пк" => {
                тормозов += 1;
                match поле("за_пк").and_then(Value::as_bool) { Some(true) => при_нём += 1, Some(false) => без_него += 1, None => {} }
            }
            "механик_лечит" | "механик_помогло" | "механик_не_помогло" | "механик_исчерпал" => {
                let класс = поле("класс").and_then(Value::as_str).unwrap_or("?").to_string();
                *лечение.entry(событие.clone()).or_default().entry(класс).or_default() += 1;
            }
            "сигнал_разуму" => {
                let отпечаток = поле("fingerprint").and_then(Value::as_str).unwrap_or("?").to_string();
                *сигналы.entry(отпечаток).or_default() += 1;
            }
            "сигнал_разуму_не_послан" => не_послано += 1,
            "журнал_потеряно" => потеряно += поле("строк").and_then(Value::as_u64).unwrap_or(0),
            _ => {}
        }
    }
    json!({ "day": день, "lines": строк, "bad": битых, "first": первая, "last": последняя, "events": события,
        "tormoza": { "vsego": тормозов, "za_pk": при_нём, "bez_nego": без_него }, "lechenie": лечение,
        "signaly": { "poslano": сигналы.values().sum::<u64>(), "ne_poslano": не_послано, "po_klyucham": сигналы },
        "barsu": { "strok": барсу, "zadavleno": задавлено }, "poteryano": потеряно })
}

#[cfg(test)]
mod тесты {
    use super::*;

    fn папка_пробы(имя: &str) -> PathBuf {
        let п = std::env::temp_dir().join(format!("mekhanik-zhurnal-{}-{имя}", std::process::id()));
        let _ = std::fs::remove_dir_all(&п);
        std::fs::create_dir_all(&п).unwrap();
        п
    }

    /// Две живые строки старого журнала (0.5.0, 13.11; ключи по алфавиту — так пишет serde_json) и строка 30.10 с −04:00.
    const СТАРЫЕ_СТРОКИ: &str = concat!(
        r#"{"data":{"гпу_макс_мс":66.0,"длилось_с":6,"окон":3,"признаки":["видеокарта"]},"event":"тормоз_пк_прошёл","ts":"2026-11-12T23:59:58.120+00:00"}"#, "\n",
        r#"{"data":{"скорость":"128k"},"event":"механик_копия_придержана","ts":"2026-11-13T00:00:02.517+00:00"}"#, "\n",
        "не json\n",
        r#"{"data":{},"event":"механик_запущен","ts":"2026-10-29T20:10:00.000-04:00"}"#, "\n");

    #[test]
    fn день_по_строке() {
        assert_eq!(день_строки(r#"{"data":{},"event":"x","ts":"2026-11-13T22:03:00.123+00:00"}"#), Some("2026-11-13"));
        assert_eq!(день_строки(r#"{"ts":"вчера"}"#), None);
        assert_eq!(день_строки("не json"), None);
        let (день, текст) = строка("проба", &json!({ "а": 1 }), 1, Some("budget"));
        assert_eq!(день_строки(&текст), Some(день.as_str()), "новая строка несёт свой день");
        let v: Value = serde_json::from_str(текст.trim()).unwrap();
        assert!(v["ms"].as_u64().unwrap() > 1_700_000_000_000 && v["lvl"] == 1 && v["cut"] == "budget" && v["event"] == "проба");
        assert!(это_день("2026-11-13") && !это_день("2026-1-13") && !это_день("2026-11-1x"));
    }

    /// Куски по дням: полночь делит, строка без даты идёт в день предыдущей, день может вернуться (часы ПК 29.10).
    #[test]
    fn старый_журнал_по_дням() {
        let к = куски_по_дням(СТАРЫЕ_СТРОКИ, "2026-01-01");
        let дни: Vec<&str> = к.iter().map(|(д, _)| д.as_str()).collect();
        assert_eq!(дни, vec!["2026-11-12", "2026-11-13", "2026-10-29"]);
        assert_eq!(к[1].1.lines().count(), 2, "строка без даты — к предыдущей");
        assert_eq!(куски_по_дням("без даты\n", "2026-01-01")[0].0, "2026-01-01");
    }

    /// 🟥 Перенос старого журнала: файлы из корня уходят в `zhurnal\staroe\`, строки — по дням; второй запуск ничего не
    /// дублирует; раскладка, прерванная на середине (метки нет), доделывается без повторов. На старом коде этого нет:
    /// один `zhurnal.jsonl` и один `.old` — переворот стирал историю старше двух файлов.
    #[test]
    fn перенос_старого_без_потерь_и_повторов() {
        let база = папка_пробы("perenos");
        std::fs::write(база.join("zhurnal.jsonl.old"), &СТАРЫЕ_СТРОКИ[..СТАРЫЕ_СТРОКИ.find("\n").unwrap() + 1]).unwrap();
        std::fs::write(база.join("zhurnal.jsonl"), &СТАРЫЕ_СТРОКИ[СТАРЫЕ_СТРОКИ.find("\n").unwrap() + 1..]).unwrap();
        let mut п = Писарь::новый(база.clone());
        let (строк, дней) = п.начать("2026-11-14");
        assert_eq!((строк, дней), (4, 3));
        assert!(!база.join("zhurnal.jsonl").exists() && !база.join("zhurnal.jsonl.old").exists(), "корень пуст");
        let д13 = std::fs::read_to_string(файл_дня(&база, "2026-11-13")).unwrap();
        assert_eq!(д13.lines().count(), 2);
        assert!(std::fs::read_to_string(файл_дня(&база, "2026-11-12")).unwrap().contains("тормоз_пк_прошёл"));
        let старое: Vec<_> = std::fs::read_dir(папка_журнала(&база).join("staroe")).unwrap().filter_map(Result::ok).collect();
        assert_eq!(старое.len(), 4, "оба старых файла целы рядом с метками");
        // второй запуск — ничего нового
        let mut п2 = Писарь::новый(база.clone());
        assert_eq!(п2.начать("2026-11-14"), (0, 0));
        // прерванная раскладка: метку стёрли — доделать без повторов
        for ф in std::fs::read_dir(папка_журнала(&база).join("staroe")).unwrap().filter_map(Result::ok) {
            if ф.path().to_string_lossy().ends_with(МЕТКА) { std::fs::remove_file(ф.path()).unwrap(); }
        }
        let mut п3 = Писарь::новый(база.clone());
        п3.начать("2026-11-14");
        assert_eq!(std::fs::read_to_string(файл_дня(&база, "2026-11-13")).unwrap(), д13, "куски не задвоились");
        // сводки прошедших дней — по разу
        let сводка = std::fs::read_to_string(файл_сводки(&база)).unwrap();
        assert_eq!(сводка.lines().count(), 3, "три дня — три сводки: {сводка}");
        let _ = std::fs::remove_dir_all(&база);
    }

    /// Файл дня, строка прошлого дня у полуночи, потерянные строки — счётом первой строкой; сводка сегодня не пишется.
    #[test]
    fn писарь_по_дням() {
        let база = папка_пробы("dni");
        let счёт: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
        let mut п = Писарь::со_счётом(база.clone(), счёт);
        п.начать("2026-11-13");
        п.строка("2026-11-13", "{\"event\":\"a\",\"ts\":\"2026-11-13T23:59:59.000+00:00\"}\n");
        п.строка("2026-11-14", "{\"event\":\"b\",\"ts\":\"2026-11-14T00:00:00.001+00:00\"}\n");
        п.строка("2026-11-13", "{\"event\":\"c\",\"ts\":\"2026-11-13T23:59:59.999+00:00\"}\n");
        счёт.fetch_add(2, Ordering::Relaxed);
        п.строка("2026-11-14", "{\"event\":\"d\",\"ts\":\"2026-11-14T00:00:01.000+00:00\"}\n");
        let д13 = std::fs::read_to_string(файл_дня(&база, "2026-11-13")).unwrap();
        let д14 = std::fs::read_to_string(файл_дня(&база, "2026-11-14")).unwrap();
        assert_eq!(д13.lines().count(), 2, "запоздалая строка прошлого дня — в свой день");
        assert!(д14.contains("журнал_потеряно") && д14.contains("\"строк\":2") && д14.lines().count() == 3);
        п.проверить_день("2026-11-14");
        let сводка = std::fs::read_to_string(файл_сводки(&база)).unwrap();
        assert_eq!(сводка.lines().count(), 1, "только прошедший день");
        assert!(сводка.contains("\"day\":\"2026-11-13\""));
        let _ = std::fs::remove_dir_all(&база);
    }

    /// Чистка: старше 60 дней — стереть, сводку — никогда.
    #[test]
    fn чистка_60_дней() {
        assert_eq!(срок_хранения("2026-11-13").as_deref(), Some("2026-09-14"));
        assert_eq!(срок_хранения("2026-03-01").as_deref(), Some("2025-12-31"));
        let база = папка_пробы("chistka");
        let mut п = Писарь::новый(база.clone());
        for д in ["2026-09-13", "2026-09-14", "2026-11-12"] { дописать(&база, д, "{\"event\":\"x\"}\n").unwrap(); }
        std::fs::write(файл_сводки(&база), "{\"day\":\"2026-01-01\"}\n").unwrap();
        п.чистить("2026-11-13");
        assert!(!файл_дня(&база, "2026-09-13").exists() && файл_дня(&база, "2026-09-14").exists() && файл_дня(&база, "2026-11-12").exists());
        assert!(файл_сводки(&база).exists());
        let _ = std::fs::remove_dir_all(&база);
    }

    #[test]
    fn сводка_по_строкам() {
        let т = concat!(
            r#"{"data":{"за_пк":false},"event":"тормоз_пк","lvl":0,"cut":"bez_nego","ts":"2026-11-13T10:00:00.000+00:00"}"#, "\n",
            r#"{"data":{"за_пк":true},"event":"тормоз_пк","lvl":1,"ts":"2026-11-13T11:00:00.000+00:00"}"#, "\n",
            r#"{"data":{"класс":"память"},"event":"механик_лечит","lvl":0,"cut":"budget","ts":"2026-11-13T11:00:01.000+00:00"}"#, "\n",
            r#"{"data":{"fingerprint":"диск:C"},"event":"сигнал_разуму","lvl":2,"ts":"2026-11-13T12:00:00.000+00:00"}"#, "\n",
            r#"{"data":{"строк":7},"event":"журнал_потеряно","lvl":0,"ts":"2026-11-13T12:00:01.000+00:00"}"#, "\n",
            "битая\n");
        let с = сводка_дня("2026-11-13", т);
        assert_eq!(с["lines"], 5);
        assert_eq!(с["bad"], 1);
        assert_eq!(с["tormoza"], json!({ "vsego": 2, "za_pk": 1, "bez_nego": 1 }));
        assert_eq!(с["lechenie"]["механик_лечит"]["память"], 1);
        assert_eq!(с["signaly"]["po_klyucham"]["диск:C"], 1);
        assert_eq!(с["barsu"], json!({ "strok": 1, "zadavleno": 1 }));
        assert_eq!(с["poteryano"], 7);
        assert_eq!(с["first"], "2026-11-13T10:00:00.000+00:00");
    }

    /// Живая проба (только чтение живого журнала): копия настоящего `zhurnal.jsonl` и `.old` во временной папке
    /// раскладывается по дням — строк столько же, дней — сколько дат, сводки — по прошедшим дням.
    #[test]
    #[ignore]
    fn живьём_перенос_копии_журнала() {
        let живая = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join("mekhanik");
        let база = папка_пробы("zhivoy");
        let mut строк = 0usize;
        let mut дни = BTreeSet::new();
        for имя in СТАРЫЕ {
            let Ok(б) = std::fs::read(живая.join(имя)) else { continue };
            let т = String::from_utf8_lossy(&б).to_string();
            строк += т.lines().filter(|с| !с.trim().is_empty()).count();
            дни.extend(т.lines().filter_map(день_строки).map(str::to_string));
            std::fs::write(база.join(имя), &б).unwrap();
        }
        let начало = Instant::now();
        let mut п = Писарь::новый(база.clone());
        let (перенесено, дней) = п.начать(&сегодня());
        let заняло = начало.elapsed();
        let в_днях: usize = Писарь::новый(база.clone()).дни().iter()
            .map(|(_, ф)| std::fs::read_to_string(ф).unwrap().lines().count()).sum();
        let сводок = std::fs::read_to_string(файл_сводки(&база)).unwrap_or_default().lines().count();
        eprintln!("живой журнал: строк {строк}, дат {}, перенесено {перенесено} строк в {дней} дней за {заняло:?}, \
            в файлах дней {в_днях}, сводок {сводок}", дни.len());
        assert_eq!(перенесено as usize, строк);
        assert_eq!(в_днях, строк, "ни одна строка не потеряна и не задвоена");
        assert_eq!(дней, дни.len());
        assert!(сводок >= дни.len() - 1, "сводка — каждому прошедшему дню");
        let _ = std::fs::remove_dir_all(&база);
    }

    /// Живая проба на своём процессе: поток записи поднимается сам, понижает себе диск и досылает очередь (в тестах
    /// папка «Механика» — своя временная, `svyaz::папка`).
    #[test]
    fn поток_досылает() {
        let (день, текст) = строка("проба_потока", &json!({ "н": 1 }), 0, None);
        в_журнал(день.clone(), текст);
        assert!(дослать(Duration::from_secs(5)), "поток ответил");
        let файл = файл_дня(&crate::svyaz::папка(), &день);
        assert!(std::fs::read_to_string(файл).unwrap_or_default().contains("проба_потока"));
    }
}
