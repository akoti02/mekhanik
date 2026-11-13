//! 🟥 25.10 (Р-193): КОПИЯ ПК НА BOX НЕ ОТНИМАЕТ ЕГО ВИДЕО. Замер 25.10 ≈17:00: YouTube буферизовал, загрузка
//! 5,9–17 Мбит/с, а копия ПК (`scripts/kopiya-pk.ps1` БАРСа, днём `--bwlimit 1M`) отдавала ≈1,07 МБ/с; копию придержали —
//! 16,8. Пока играет видео, скорость копии — `ПРИ_ВИДЕО` (rclone слушает пульт, флаг `--rc` в скрипте копии). Копия не
//! останавливается и ничего не теряет — только медленнее, пока он смотрит.
//!
//! 🟥 13.11 (волна 2а) БЕЗ ДЁРГАНЬЯ И С ТОЧНЫМ ВОЗВРАТОМ. Разбор 25.10–13.11: 940 придержек, 255 короче минуты —
//! отпускало после одного замера без видео; возвращалось расписание, переписанное в «Механик» из скрипта, а не скорость
//! самого rclone; при выходе и падении придержка не снималась (13.11 19:16 — 23 мин, до следующего видео). Теперь:
//! - придержать — только когда видео идёт `ДЕРЖАТЬ_ПОСЛЕ_МС` подряд, отпустить — когда его нет `ОТПУСТИТЬ_ПОСЛЕ_МС`;
//! - пульт, вход и расписание — из строки запуска самого rclone (`--rc-addr`, `--rc-user`/`--rc-pass`, `--bwlimit`):
//!   вернуть — скорость его расписания на этот миг; строки не прочесть — ту, что пульт сказал до придержки
//!   (`core/bwlimit` без `rate`); уже не быстрее `ПРИ_ВИДЕО` — не трогаем;
//! - каждая папка копии — свой процесс rclone: новый прогон, пока он смотрит, — придержать и его;
//! - пульт — только на этом ПК (`127.0.0.1`, `localhost`, `::1`), иначе не ходим: «Механик» полностью офлайн;
//! - разговор с пультом (до 3 с) — в своём потоке `kopiya`, шаг сторожа не ждёт; придержка — в `kopiya.json` (надёжно):
//!   упал «Механик» — новый подхватит и отпустит; при выходе (`Drop`) — вернуть сразу.

use serde_json::{json, Value};
use std::ffi::c_void;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::svyaz::{self, журнал, журнал_барсу};

const ПРИ_ВИДЕО: &str = "128k";
const ПРИ_ВИДЕО_БАЙТ: i64 = 128 * 1024;
/// Видео идёт столько подряд — придержать (мигнувший декодер и короткий ролик — не повод).
pub const ДЕРЖАТЬ_ПОСЛЕ_МС: u64 = 30_000;
/// Видео нет столько подряд — отпустить (пауза, переключение серий — не повод дёргать копию).
pub const ОТПУСТИТЬ_ПОСЛЕ_МС: u64 = 90_000;
/// Пока держим — проверять раз в столько (новый прогон rclone или его расписание сменило ступень).
const ПРОВЕРЯТЬ: Duration = Duration::from_secs(60);
/// Шаг своего потока.
const ШАГ: Duration = Duration::from_secs(5);
/// Пульт молчит или отказал — писать себе не чаще.
const ОШИБКА_РАЗ_В: Duration = Duration::from_secs(30 * 60);
/// Пульт rclone по умолчанию (`--rc` без `--rc-addr`).
const ПУЛЬТ_ПО_УМОЛЧАНИЮ: &str = "localhost:5572";

// ───────────────────────────── чистое: дребезг, строка запуска, расписание ─────────────────────────────

/// Чистая (видно в тесте): гаситель дребезга придержки.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Дребезг { pub видео_с: Option<u64>, pub нет_с: Option<u64>, pub держим: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Шаг { Придержать, Отпустить, Ничего }

impl Дребезг {
    /// Шаг: видео ли сейчас и который миг (мс). Придержать — видео подряд `ДЕРЖАТЬ_ПОСЛЕ_МС`; отпустить — нет видео
    /// подряд `ОТПУСТИТЬ_ПОСЛЕ_МС`.
    pub fn шаг(&mut self, видео: bool, сейчас: u64) -> Шаг {
        if видео {
            self.нет_с = None;
            let с = *self.видео_с.get_or_insert(сейчас);
            if !self.держим && сейчас.saturating_sub(с) >= ДЕРЖАТЬ_ПОСЛЕ_МС { self.держим = true; return Шаг::Придержать; }
        } else {
            self.видео_с = None;
            let с = *self.нет_с.get_or_insert(сейчас);
            if self.держим && сейчас.saturating_sub(с) >= ОТПУСТИТЬ_ПОСЛЕ_МС { self.держим = false; return Шаг::Отпустить; }
        }
        Шаг::Ничего
    }
}

/// Чистая (видно в тесте): строка запуска Windows → слова (правила `CommandLineToArgvW`: кавычки, `\"`, `\\"`).
pub fn слова_строки(с: &str) -> Vec<String> {
    let мы: Vec<char> = с.chars().collect();
    let mut слова = Vec::new();
    let mut i = 0;
    while i < мы.len() {
        while i < мы.len() && (мы[i] == ' ' || мы[i] == '\t') { i += 1; }
        if i >= мы.len() { break; }
        let mut слово = String::new();
        let mut в_кавычках = false;
        while i < мы.len() && (в_кавычках || (мы[i] != ' ' && мы[i] != '\t')) {
            if мы[i] == '\\' {
                let mut н = 0;
                while i < мы.len() && мы[i] == '\\' { н += 1; i += 1; }
                if i < мы.len() && мы[i] == '"' {
                    слово.extend(std::iter::repeat_n('\\', н / 2));
                    if н % 2 == 1 { слово.push('"'); i += 1; }
                } else { слово.extend(std::iter::repeat_n('\\', н)); }
            } else if мы[i] == '"' {
                // "" внутри кавычек — одна кавычка
                if в_кавычках && мы.get(i + 1) == Some(&'"') { слово.push('"'); i += 2; continue; }
                в_кавычках = !в_кавычках;
                i += 1;
            } else { слово.push(мы[i]); i += 1; }
        }
        слова.push(слово);
    }
    слова
}

/// Чистая: значение ключа (`--ключ значение` или `--ключ=значение`).
pub fn значение(слова: &[String], ключ: &str) -> Option<String> {
    let с_равно = format!("{ключ}=");
    слова.iter().enumerate().find_map(|(i, с)| if с == ключ { слова.get(i + 1).cloned() }
        else { с.strip_prefix(с_равно.as_str()).map(str::to_string) })
}

/// Пульт rclone: адрес на этом ПК и вход (если задан).
#[derive(Clone, Debug, PartialEq)]
pub struct Пульт { pub адрес: SocketAddr, pub вход: Option<(String, String)> }

/// Чистая (видно в тесте): пульт из строки запуска rclone. `Ok(None)` — пульта нет (`--rc` не задан); `Err` — пульт не на
/// этом ПК: туда «Механик» не ходит (полностью офлайн).
pub fn пульт_из_слов(слова: &[String]) -> Result<Option<Пульт>, &'static str> {
    let rc = слова.iter().any(|с| с == "--rc" || (с.starts_with("--rc=") && с != "--rc=false"));
    let адрес = значение(слова, "--rc-addr");
    if !rc && адрес.is_none() { return Ok(None); }
    let адрес = адрес.unwrap_or_else(|| ПУЛЬТ_ПО_УМОЛЧАНИЮ.to_string());
    let (хост, порт) = адрес.rsplit_once(':').ok_or("адрес пульта без порта")?;
    let порт: u16 = порт.parse().map_err(|_| "порт пульта не число")?;
    let хост = хост.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = match хост {
        "" | "localhost" => IpAddr::V4(Ipv4Addr::LOCALHOST),
        х => х.parse().map_err(|_| "пульт не по адресу — имена не разрешаю (офлайн)")?,
    };
    // «на всех» (0.0.0.0, ::) — ходим на свой 127.0.0.1; чужой адрес — нет
    let ip = if ip.is_unspecified() { IpAddr::V4(Ipv4Addr::LOCALHOST) } else { ip };
    if !ip.is_loopback() { return Err("пульт не на этом ПК — «Механик» в сеть не ходит"); }
    let вход = match (значение(слова, "--rc-user"), значение(слова, "--rc-pass")) {
        (Some(л), Some(п)) if !л.is_empty() => Some((л, п)),
        _ => None,
    };
    Ok(Some(Пульт { адрес: SocketAddr::new(ip, порт), вход }))
}

/// Чистая (видно в тесте): скорость по расписанию rclone (`--bwlimit`) на этот миг: «1M» — одна на весь день;
/// «08:00,1M 23:30,off» — по часам (раньше первой точки — последняя вчерашняя); «Mon-08:00,512k Sat-10:00,off» — по
/// дням недели (`день` 0 — понедельник). `None` — не разобрать.
pub fn по_расписанию_rclone(р: &str, день: u32, час: u32, минута: u32) -> Option<String> {
    let части: Vec<&str> = р.split_whitespace().collect();
    if части.is_empty() { return None; }
    if части.len() == 1 && !части[0].contains(',') { return Some(части[0].to_string()); }
    let сейчас = день * 1440 + час * 60 + минута;
    let mut точки: Vec<(u32, String)> = Vec::new();
    for ч in части {
        let (время, скорость) = ч.split_once(',')?;
        let (дни, чм): (Vec<u32>, &str) = match время.split_once('-') {
            Some((д, т)) => (vec![день_недели(д)?], т),
            None => ((0..7).collect(), время),
        };
        let (ч_, м_) = чм.split_once(':')?;
        let (ч_, м_): (u32, u32) = (ч_.parse().ok()?, м_.parse().ok()?);
        if ч_ > 23 || м_ > 59 || скорость.is_empty() { return None; }
        for д in дни { точки.push((д * 1440 + ч_ * 60 + м_, скорость.to_string())); }
    }
    точки.sort_by_key(|(т, _)| *т);
    точки.iter().rev().find(|(т, _)| *т <= сейчас).or(точки.last()).map(|(_, с)| с.clone())
}

fn день_недели(д: &str) -> Option<u32> {
    let д = д.to_lowercase();
    ["mon", "tue", "wed", "thu", "fri", "sat", "sun"].iter().position(|н| д.starts_with(н)).map(|п| п as u32)
}

/// Чистая (видно в тесте): байт/с (−1 — без предела) → скорость строкой, которую rclone примет (`1M` — МиБ/с, `k` — КиБ/с).
pub fn скорость_строкой(байт: i64) -> String {
    if байт <= 0 { return "off".into(); }
    if байт % (1 << 20) == 0 { return format!("{}M", байт >> 20); }
    format!("{}k", ((байт + 512) / 1024).max(1))
}

// ───────────────────────────── строка запуска процесса ─────────────────────────────

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(доступ: u32, наследовать: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn GetModuleHandleW(имя: *const u16) -> isize;
    fn GetProcAddress(модуль: isize, имя: *const u8) -> *const c_void;
}

/// Строка запуска процесса (`NtQueryInformationProcess`, ProcessCommandLineInformation = 60; прав — только прочесть,
/// PROCESS_QUERY_LIMITED_INFORMATION). `None` — не открыть.
pub fn командная_строка(pid: u32) -> Option<String> {
    type Спросить = unsafe extern "system" fn(isize, u32, *mut c_void, u32, *mut u32) -> i32;
    let имя: Vec<u16> = "ntdll.dll".encode_utf16().chain(std::iter::once(0)).collect();
    let м = unsafe { GetModuleHandleW(имя.as_ptr()) };
    if м == 0 { return None; }
    let ф = unsafe { GetProcAddress(м, b"NtQueryInformationProcess\0".as_ptr()) };
    if ф.is_null() { return None; }
    let спросить: Спросить = unsafe { std::mem::transmute::<*const c_void, Спросить>(ф) };
    let h = unsafe { OpenProcess(0x1000, 0, pid) };
    if h == 0 { return None; }
    // буфер из u64 — указатель внутри UNICODE_STRING выровнен
    let mut буф: Vec<u64> = vec![0; 1024];
    let mut вышло = false;
    for _ in 0..3 {
        let mut длина = 0u32;
        let ст = unsafe { спросить(h, 60, буф.as_mut_ptr() as *mut c_void, (буф.len() * 8) as u32, &mut длина) } as u32;
        if ст == 0 { вышло = true; break; }
        // мал буфер: STATUS_INFO_LENGTH_MISMATCH, STATUS_BUFFER_OVERFLOW, STATUS_BUFFER_TOO_SMALL
        if matches!(ст, 0xC000_0004 | 0x8000_0005 | 0xC000_0023) && (длина as usize) > буф.len() * 8 {
            буф = vec![0; длина as usize / 8 + 64];
            continue;
        }
        break;
    }
    unsafe { CloseHandle(h) };
    if !вышло { return None; }
    // UNICODE_STRING (x64): Length u16, MaximumLength u16, 4 байта выравнивания, Buffer — указатель
    let начало = буф.as_ptr() as usize;
    let конец = начало + буф.len() * 8;
    let длина = (буф[0] & 0xFFFF) as usize;
    let указатель = буф[1] as usize;
    if длина == 0 { return Some(String::new()); }
    if указатель < начало || указатель.saturating_add(длина) > конец || указатель % 2 != 0 { return None; }
    let слова = unsafe { std::slice::from_raw_parts(указатель as *const u16, длина / 2) };
    Some(String::from_utf16_lossy(слова))
}

/// Процесс rclone с пультом.
#[derive(Clone, Debug)]
struct Rclone { pid: u32, создан: u64, пульт: Result<Option<Пульт>, &'static str>, расписание: Option<String> }

fn найти_rclone() -> Option<Rclone> {
    let mut без_пульта = None;
    for (pid, _, имя) in crate::okna::процессы() {
        if !имя.eq_ignore_ascii_case("rclone.exe") { continue; }
        let Some(строка) = командная_строка(pid) else { continue };
        let слова = слова_строки(&строка);
        let пульт = пульт_из_слов(&слова);
        let Some(создан) = crate::okna::создан(pid) else { continue };
        let р = Rclone { pid, создан, расписание: значение(&слова, "--bwlimit"), пульт };
        // прогоны копии идут по одному; снимок базы (`copyto` без `--rc`) — придержать нечем
        if matches!(р.пульт, Ok(Some(_))) { return Some(р); }
        без_пульта.get_or_insert(р);
    }
    без_пульта
}

// ───────────────────────────── пульт ─────────────────────────────

/// Чистая: base64 (для входа в пульт).
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

/// Чистая (видно в тесте): тело ответа HTTP (с `Transfer-Encoding: chunked` — собрать куски); не 200 — первая строка.
pub fn тело_ответа(ответ: &str) -> Result<String, String> {
    let (голова, тело) = ответ.split_once("\r\n\r\n").ok_or_else(|| "нет ответа".to_string())?;
    let первая = голова.lines().next().unwrap_or("");
    if !(первая.starts_with("HTTP/1.1 200") || первая.starts_with("HTTP/1.0 200")) { return Err(первая.to_string()); }
    if !голова.to_lowercase().contains("transfer-encoding: chunked") { return Ok(тело.to_string()); }
    let mut собрано = String::new();
    let mut ост = тело;
    loop {
        let (размер, дальше) = ост.split_once("\r\n").ok_or("кусок без конца")?;
        let н = usize::from_str_radix(размер.split(';').next().unwrap_or("").trim(), 16).map_err(|_| "размер куска")?;
        if н == 0 { break; }
        собрано.push_str(дальше.get(..н).ok_or("кусок короче")?);
        ост = дальше.get(н..).unwrap_or("").trim_start_matches("\r\n");
    }
    Ok(собрано)
}

/// `core/bwlimit`: `скорость` — задать, `None` — спросить. Вернуть байт/с сейчас (−1 — без предела).
fn к_пульту(п: &Пульт, скорость: Option<&str>) -> Result<i64, String> {
    let mut с = TcpStream::connect_timeout(&п.адрес, Duration::from_millis(300)).map_err(|e| format!("{e}"))?;
    let _ = с.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = с.set_write_timeout(Some(Duration::from_secs(2)));
    let тело = match скорость { Some(с) => json!({ "rate": с }).to_string(), None => "{}".to_string() };
    let вход = п.вход.as_ref().map(|(л, пр)| format!("Authorization: Basic {}\r\n", base64(format!("{л}:{пр}").as_bytes())))
        .unwrap_or_default();
    write!(с, "POST /core/bwlimit HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n{вход}Content-Length: {}\r\n\
        Connection: close\r\n\r\n{тело}", тело.len()).map_err(|e| format!("{e}"))?;
    let mut байты = Vec::new();
    let _ = с.read_to_end(&mut байты);
    let т = тело_ответа(&String::from_utf8_lossy(&байты))?;
    let v: Value = serde_json::from_str(&т).map_err(|_| "ответ пульта не JSON".to_string())?;
    v.get("bytesPerSecond").and_then(Value::as_i64).ok_or_else(|| "в ответе нет bytesPerSecond".to_string())
}

// ───────────────────────────── поток копии ─────────────────────────────

/// Кого держим: процесс rclone (номер и время создания), что было до придержки, его расписание.
#[derive(Clone, Debug, PartialEq)]
pub struct Держу { pub pid: u32, pub создан: u64, pub прежняя_байт: i64, pub расписание: Option<String>, pub с_мс: u64 }

impl Держу {
    fn в_json(&self) -> Value {
        json!({ "proto": svyaz::ПРОТОКОЛ, "rclone_pid": self.pid, "rclone_created": self.создан,
            "prezhnyaya_bps": self.прежняя_байт, "raspisanie": self.расписание, "s_ms": self.с_мс })
    }
    fn из_json(v: &Value) -> Option<Держу> {
        Some(Держу { pid: v.get("rclone_pid")?.as_u64()? as u32, создан: v.get("rclone_created")?.as_u64()?,
            прежняя_байт: v.get("prezhnyaya_bps")?.as_i64()?, расписание: v.get("raspisanie").and_then(Value::as_str).map(str::to_string),
            с_мс: v.get("s_ms").and_then(Value::as_u64).unwrap_or(0) })
    }
}

/// Чистая (видно в тесте): что вернуть — скорость расписания rclone на этот миг, а не прочесть — что было до придержки.
pub fn вернуть_скорость(х: &Держу, день: u32, час: u32, минута: u32) -> (String, &'static str) {
    match х.расписание.as_deref().and_then(|р| по_расписанию_rclone(р, день, час, минута)) {
        Some(с) => (с, "по расписанию самого rclone"),
        None => (скорость_строкой(х.прежняя_байт), "как было до придержки"),
    }
}

fn файл() -> std::path::PathBuf { svyaz::папка().join("kopiya.json") }

/// Общее со сторожем: видео ли (и когда сказано) и просьба вернуть всё (выход).
struct Общее { видео: Option<(bool, u64)>, вернуть: Option<SyncSender<()>> }
static ОБЩЕЕ: Mutex<Общее> = Mutex::new(Общее { видео: None, вернуть: None });
static ПОТОК: OnceLock<bool> = OnceLock::new();

/// Копия ПК — со стороны сторожа: шаг раз в 10 с (`видео` — играет ли видео по датчику БАРСа или своему), а говорит с
/// пультом свой поток. Ушла из сторожа (выход, падение) — придержку вернуть сразу.
#[derive(Default)]
pub struct Копия { _н: () }

impl Копия {
    pub fn шаг(&mut self, видео: bool) {
        ОБЩЕЕ.lock().unwrap_or_else(|e| e.into_inner()).видео = Some((видео, svyaz::сейчас_мс()));
        ПОТОК.get_or_init(|| std::thread::Builder::new().name("kopiya".into()).spawn(работать).is_ok());
    }
}

impl Drop for Копия {
    fn drop(&mut self) {
        if ПОТОК.get() != Some(&true) { return; }
        let (к, ответ) = sync_channel(1);
        ОБЩЕЕ.lock().unwrap_or_else(|e| e.into_inner()).вернуть = Some(к);
        let _ = ответ.recv_timeout(Duration::from_secs(5));
        svyaz::zhurnal::дослать(Duration::from_secs(1));
    }
}

fn работать() {
    loop {
        if std::panic::catch_unwind(круги).is_ok() { return; }
        журнал("механик_копия_поток_упал", &json!({ "смысл": "поток копии ПК упал — через минуту снова" }));
        std::thread::sleep(Duration::from_secs(60));
    }
}

struct Поток { д: Дребезг, держу: Option<Держу>, сказано_барсу: bool, проверено: Option<Instant>, ошибка: Option<Instant> }

fn круги() {
    // прошлый «Механик» держал и не вернул (упал или вышел) — подхватить: видео нет 1,5 мин — вернём
    let держу = std::fs::read_to_string(файл()).ok().and_then(|т| serde_json::from_str::<Value>(&т).ok())
        .and_then(|v| Держу::из_json(&v));
    let mut п = Поток { д: Дребезг::default(), сказано_барсу: держу.is_some(), держу, проверено: None, ошибка: None };
    if п.держу.is_some() {
        п.д = Дребезг { держим: true, нет_с: Some(svyaz::сейчас_мс()), видео_с: None };
        журнал("механик_копия_подхвачена", &json!({ "смысл": "прошлый «Механик» придержал копию и не вернул: вернёт этот, \
            когда видео не будет 1,5 мин" }));
    }
    let mut шаг_когда: Option<Instant> = None;
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let (видео, вернуть) = {
            let mut о = ОБЩЕЕ.lock().unwrap_or_else(|e| e.into_inner());
            (о.видео.map(|(в, _)| в), о.вернуть.take())
        };
        if let Some(ответ) = вернуть {
            п.отпустить("«Механик» выходит");
            п.д = Дребезг::default();
            let _ = ответ.try_send(());
            continue;
        }
        if шаг_когда.is_some_and(|т| т.elapsed() < ШАГ) { continue; }
        шаг_когда = Some(Instant::now());
        match п.д.шаг(видео.unwrap_or(false), svyaz::сейчас_мс()) {
            Шаг::Придержать => { п.сказано_барсу = false; п.держать(); }
            Шаг::Отпустить => п.отпустить("видео нет 1,5 мин"),
            Шаг::Ничего => if п.д.держим && п.проверено.is_none_or(|т| т.elapsed() >= ПРОВЕРЯТЬ) { п.держать(); },
        }
    }
}

impl Поток {
    fn ошибка(&mut self, что: &str, почему: &str) {
        if self.ошибка.is_some_and(|т| т.elapsed() < ОШИБКА_РАЗ_В) { return; }
        self.ошибка = Some(Instant::now());
        журнал("механик_копия_пульт", &json!({ "что": что, "почему": почему }));
    }

    fn забыть(&mut self) {
        self.держу = None;
        svyaz::zhurnal::важное(svyaz::zhurnal::Задание::Стереть(файл()));
    }

    /// Придержать (или проверить, что держим): новый прогон rclone — придержать и его.
    fn держать(&mut self) {
        self.проверено = Some(Instant::now());
        let Some(р) = найти_rclone() else {
            if let Some(х) = self.держу.clone() {
                журнал("механик_копия_прогон_кончился", &json!({ "rclone": х.pid, "смысл": "прогон копии кончился, пока держал: \
                    отпускать некого" }));
                self.забыть();
            }
            return;
        };
        let пульт = match &р.пульт {
            Ok(Some(п)) => п.clone(),
            Ok(None) => return,
            Err(почему) => { self.ошибка("пульт", почему); return; }
        };
        if let Some(х) = &self.держу {
            if (х.pid, х.создан) == (р.pid, р.создан) {
                // держим этот прогон: расписание rclone могло сменить ступень (23:30) — поставить снова
                match к_пульту(&пульт, None) {
                    Ok(б) if б > 0 && б <= ПРИ_ВИДЕО_БАЙТ => {}
                    Ok(_) => { let _ = к_пульту(&пульт, Some(ПРИ_ВИДЕО)); }
                    Err(e) => self.ошибка("проверка", &e),
                }
                return;
            }
            // прежний прогон кончился, идёт новый — он со своим расписанием, возвращать прежнему некого
            self.забыть();
        }
        let было = match к_пульту(&пульт, None) { Ok(б) => б, Err(e) => { self.ошибка("спросить скорость", &e); return; } };
        if было > 0 && было <= ПРИ_ВИДЕО_БАЙТ { return; }
        if let Err(e) = к_пульту(&пульт, Some(ПРИ_ВИДЕО)) { self.ошибка("придержать", &e); return; }
        let х = Держу { pid: р.pid, создан: р.создан, прежняя_байт: было, расписание: р.расписание.clone(), с_мс: svyaz::сейчас_мс() };
        svyaz::zhurnal::важное(svyaz::zhurnal::Задание::Файл { путь: файл(), текст: х.в_json().to_string(), надёжно: true });
        let данные = json!({ "скорость": ПРИ_ВИДЕО, "было": скорость_строкой(было), "rclone": р.pid,
            "почему": "видео идёт полминуты — копия ПК на Box не отнимает у него интернет" });
        if self.сказано_барсу { журнал("механик_копия_придержана", &данные); } else { журнал_барсу("механик_копия_придержана", &данные); }
        self.сказано_барсу = true;
        self.держу = Some(х);
    }

    /// Вернуть скорость (тот же прогон rclone — его расписание на этот миг) и забыть придержку.
    fn отпустить(&mut self, почему: &str) {
        let Some(х) = self.держу.clone() else { return };
        use chrono::{Datelike, Timelike};
        let т = chrono::Local::now();
        let (скорость, откуда) = вернуть_скорость(&х, т.weekday().num_days_from_monday(), т.hour(), т.minute());
        let итог = match найти_rclone() {
            Some(р) if (р.pid, р.создан) == (х.pid, х.создан) => match &р.пульт {
                Ok(Some(п)) => к_пульту(п, Some(&скорость)).map(|_| "вернул").unwrap_or_else(|e| { self.ошибка("вернуть", &e); "пульт не ответил" }),
                _ => "пульта нет",
            },
            _ => "прогон кончился — отпускать некого",
        };
        let данные = json!({ "скорость": скорость, "откуда": откуда, "итог": итог, "держали_с": svyaz::сейчас_мс().saturating_sub(х.с_мс) / 1000,
            "почему": почему });
        if self.сказано_барсу { журнал_барсу("механик_копия_отпущена", &данные); } else { журнал("механик_копия_отпущена", &данные); }
        self.сказано_барсу = false;
        self.забыть();
    }
}

#[cfg(test)]
mod тесты {
    use super::*;

    /// 🟥 Без дребезга: придержать — видео полминуты подряд, отпустить — нет видео 1,5 мин; живой ряд 13.11 20:32:18 →
    /// 20:32:39 (отпуск — и через 21 с снова придержка). КРАСНЫЙ на старом коде: придержка и отпуск — с первого замера.
    #[test]
    fn без_дребезга() {
        let mut д = Дребезг::default();
        let с = |сек: u64| сек * 1000;
        assert_eq!(д.шаг(true, с(0)), Шаг::Ничего, "видео только началось — не повод");
        assert_eq!(д.шаг(true, с(20)), Шаг::Ничего);
        assert_eq!(д.шаг(true, с(30)), Шаг::Придержать);
        assert_eq!(д.шаг(true, с(40)), Шаг::Ничего, "держим — раз");
        assert_eq!(д.шаг(false, с(50)), Шаг::Ничего, "одна пауза — не отпуск");
        assert_eq!(д.шаг(false, с(71)), Шаг::Ничего, "21 с без видео — держим (13.11 20:32:39)");
        assert_eq!(д.шаг(true, с(80)), Шаг::Ничего, "видео вернулось — придержка та же");
        assert_eq!(д.шаг(false, с(90)), Шаг::Ничего);
        assert_eq!(д.шаг(false, с(179)), Шаг::Ничего);
        assert_eq!(д.шаг(false, с(180)), Шаг::Отпустить, "1,5 мин без видео");
        assert_eq!(д.шаг(false, с(300)), Шаг::Ничего);
        assert_eq!(д.шаг(true, с(310)), Шаг::Ничего, "ролик на 10 с — копию не трогаем");
        assert_eq!(д.шаг(false, с(320)), Шаг::Ничего);
    }

    /// 🟥 Возврат — ровно его скорость: из строки запуска rclone (его скрипт 13.11) — расписание на этот миг, а не то, что
    /// переписано в «Механик»; строки нет — что было до придержки. КРАСНЫЙ на старом коде: возвращалось своё расписание
    /// «08:00–23:30 1M», что бы ни стояло в скрипте.
    #[test]
    fn возврат_ровно_его_скорости() {
        let строка = r#""C:\Users\U\AppData\Local\bars\rclone\rclone.exe" --config C:\Users\U\AppData\Local\bars\rclone\rclone.conf copy C:\dev boxpc:bars-pc/C/dev --backup-dir boxpc:bars-pc-old/2026-11-13/C/dev --rc --rc-addr 127.0.0.1:5573 --rc-no-auth --filter-from C:\dev\overseer\scripts\kopiya-pk-mimo.txt --bwlimit "08:00,1M 23:30,off" --transfers 4 --checkers 8 --local-encoding Slash,LtGt,DoubleQuote,Colon,Question,Asterisk,Pipe,BackSlash,Ctl,InvalidUtf8,Dot --log-file C:\x.log --log-level NOTICE --stats 0"#;
        let с = слова_строки(строка);
        assert_eq!(с[0], r"C:\Users\U\AppData\Local\bars\rclone\rclone.exe");
        assert_eq!(значение(&с, "--bwlimit").as_deref(), Some("08:00,1M 23:30,off"));
        let п = пульт_из_слов(&с).unwrap().unwrap();
        assert_eq!((п.адрес, п.вход.clone()), ("127.0.0.1:5573".parse().unwrap(), None));
        let р = "08:00,1M 23:30,off";
        assert_eq!(по_расписанию_rclone(р, 3, 7, 59).as_deref(), Some("off"), "раньше первой точки — вчерашняя последняя");
        assert_eq!(по_расписанию_rclone(р, 3, 8, 0).as_deref(), Some("1M"));
        assert_eq!(по_расписанию_rclone(р, 3, 23, 29).as_deref(), Some("1M"));
        assert_eq!(по_расписанию_rclone(р, 3, 23, 30).as_deref(), Some("off"));
        // поменяли скрипт — возвращаем новое, а не зашитое
        assert_eq!(по_расписанию_rclone("07:00,2M 22:00,512k", 0, 23, 0).as_deref(), Some("512k"));
        assert_eq!(по_расписанию_rclone("10M:100k", 0, 12, 0).as_deref(), Some("10M:100k"), "одна скорость на весь день");
        let нед = "Mon-00:00,512 Fri-23:59,10M Sat-10:00,1M Sun-20:00,off";
        assert_eq!(по_расписанию_rclone(нед, 2, 12, 0).as_deref(), Some("512"));
        assert_eq!(по_расписанию_rclone(нед, 5, 9, 0).as_deref(), Some("10M"));
        assert_eq!(по_расписанию_rclone(нед, 6, 21, 0).as_deref(), Some("off"));
        assert_eq!(по_расписанию_rclone("25:00,1M", 0, 1, 0), None);
        let х = Держу { pid: 1, создан: 2, прежняя_байт: 1 << 20, расписание: Some(р.into()), с_мс: 0 };
        assert_eq!(вернуть_скорость(&х, 3, 12, 0), ("1M".to_string(), "по расписанию самого rclone"));
        assert_eq!(вернуть_скорость(&х, 3, 23, 45).0, "off", "23:30 прошло, пока держали, — его расписание, а не прежняя");
        let без = Держу { расписание: None, ..х.clone() };
        assert_eq!(вернуть_скорость(&без, 3, 12, 0), ("1M".to_string(), "как было до придержки"));
        assert_eq!((скорость_строкой(-1), скорость_строкой(3 << 20), скорость_строкой(512 * 1024), скорость_строкой(1500)),
            ("off".into(), "3M".into(), "512k".into(), "1k".into()));
        assert_eq!(Держу::из_json(&х.в_json()), Some(х), "придержка — на диск и обратно");
    }

    /// Пульт — только на этом ПК (офлайн): чужой адрес и имена не разрешаем; вход — из строки запуска.
    #[test]
    fn пульт_только_свой() {
        let с = |т: &str| слова_строки(т);
        assert_eq!(пульт_из_слов(&с("rclone copyto a b --bwlimit 1M")), Ok(None), "без --rc — пульта нет");
        assert_eq!(пульт_из_слов(&с("rclone --rc")).unwrap().unwrap().адрес, "127.0.0.1:5572".parse().unwrap());
        assert_eq!(пульт_из_слов(&с("rclone --rc --rc-addr=[::1]:6000")).unwrap().unwrap().адрес, "[::1]:6000".parse().unwrap());
        assert_eq!(пульт_из_слов(&с("rclone --rc --rc-addr :5573")).unwrap().unwrap().адрес, "127.0.0.1:5573".parse().unwrap());
        assert!(пульт_из_слов(&с("rclone --rc --rc-addr 10.0.0.5:5573")).is_err(), "чужой адрес — не ходим");
        assert!(пульт_из_слов(&с("rclone --rc --rc-addr box.example:5573")).is_err(), "имя — не разрешаем");
        let п = пульт_из_слов(&с(r#"rclone --rc --rc-addr 127.0.0.1:5573 --rc-user mekhanik --rc-pass "a b""#)).unwrap().unwrap();
        assert_eq!(п.вход, Some(("mekhanik".into(), "a b".into())));
        assert_eq!(base64(b"mekhanik:a b"), "bWVraGFuaWs6YSBi");
    }

    #[test]
    fn строка_запуска_как_у_windows() {
        assert_eq!(слова_строки(r#"a "b c" d"#), vec!["a", "b c", "d"]);
        assert_eq!(слова_строки(r#"a\\\"b "c\\" d"#), vec![r#"a\"b"#, r"c\", "d"]);
        assert_eq!(слова_строки(r"C:\dev\x  --y"), vec![r"C:\dev\x", "--y"]);
        assert_eq!(слова_строки(r#""a""b""#), vec![r#"a"b"#]);
        assert!(слова_строки("   ").is_empty());
    }

    #[test]
    fn ответ_пульта() {
        let ок = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 72\r\n\r\n{\"bytesPerSecond\":1048576,\"bytesPerSecondRx\":1048576,\"rate\":\"1Mi\"}";
        assert!(тело_ответа(ок).unwrap().contains("1048576"));
        let куски = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n2\r\n1}\r\n0\r\n\r\n";
        assert_eq!(тело_ответа(куски).unwrap(), "{\"a\":1}");
        assert_eq!(тело_ответа("HTTP/1.1 401 Unauthorized\r\n\r\n"), Err("HTTP/1.1 401 Unauthorized".to_string()));
    }

    /// Живая проба на своём процессе: строка запуска читается (только чтение).
    #[test]
    fn своя_строка_запуска() {
        let с = командная_строка(std::process::id()).expect("своя строка запуска");
        let имя = std::env::current_exe().unwrap().file_stem().unwrap().to_string_lossy().to_string();
        assert!(с.contains(&имя), "{с}");
    }
}
