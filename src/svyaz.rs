//! 🟥 25.10 (Р-193, Р-194, его слово: «…отдельно от Bars, но… чтобы Bars знал об этой системе, а этой системой знал
//! Bars»). СВЯЗЬ С БАРСОМ — файлами в `%LOCALAPPDATA%\mekhanik\`, без сервера и без сети:
//! - `ot_barsa.json` — пишет БАРС раз в 2 с: его окно, голова, движки, движки счёта, что считает, загружен ли 27B,
//!   играет ли видео, режим ПК. Так «Механик» знает БАРС. Отметке старше `БАРС_СВЕЖ_МС` не верим: БАРСа нет —
//!   «Механик» работает без его частей;
//! - `barsu.json` — пишет «Механик» раз в 2 с: тормозит ли ПК сейчас (фон разума 27B ждёт) — так БАРС знает «Механика»;
//! - `soobshcheniya\*.json` — БАРСу: сигнал разуму (вид «пк»), строка в журнал БАРСа, просьба (выгрузить 27B);
//!   БАРС забирает и стирает; ответ на просьбу — `otvety\<номер>.json`;
//! - `zhurnal.jsonl` — свой журнал; `stop.flag` — выкладка просит выйти (всё вернуть и выйти).
//! Ключи файлов — латиницей: их читают и скрипты PowerShell.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Отметка БАРСа старше — БАРСа нет (окно упало или выкладывается).
pub const БАРС_СВЕЖ_МС: u64 = 15_000;
/// Повод вида «пк» голова режет до 400 знаков (`hub/src/signals.rs::повод_знаков`) — режем сами, по целому.
const ЗНАКОВ_ПОВОДА: usize = 400;
/// Свой журнал больше — в `.old`.
const ЖУРНАЛ_МАКС: u64 = 5 << 20;
/// Сообщений, которые БАРС не забрал, больше — строки журнала не копим (сигналы и просьбы — всегда).
const СООБЩЕНИЙ_МАКС: usize = 300;

pub fn папка() -> PathBuf {
    let база = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    база.join("mekhanik")
}

pub fn сейчас_мс() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|д| д.as_millis() as u64).unwrap_or(0) }

/// Записать файл целиком: во временный, потом подменой — читатель не увидит половину.
pub fn записать(путь: &PathBuf, текст: &str) -> std::io::Result<()> {
    let врем = путь.with_extension("tmp");
    std::fs::write(&врем, текст)?;
    std::fs::rename(&врем, путь)
}

// ───────────────────────────── что знаем о БАРСе ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Барс {
    /// Отметка свежа.
    pub жив: bool,
    pub окно: Option<u32>,
    pub голова: Option<u32>,
    /// Движки распорядителя (и браузеры БАРСа) — им очередь видеокарты ниже обычной; их процессор не «чужой».
    pub движки: Vec<u32>,
    /// Движки счёта (без браузеров) — их можно заморозить на шаг уступки экрану.
    pub счёт: Vec<u32>,
    /// Что БАРС делает: занятые движки словами («разум 27B», «судья экрана E4B»…).
    pub работа: Vec<String>,
    /// Разум 27B в памяти (загружен) — и сколько секунд простаивает (None — считает или не загружен).
    pub разум_загружен: bool,
    pub разум_простой_с: Option<u64>,
    /// Играет видео (декодер видеокарты) — у БАРСа свой датчик.
    pub видео: bool,
    /// Режим ПК по датчику БАРСа.
    pub режим: Option<String>,
}

/// Чистая (видно в тесте): разобрать `ot_barsa.json`; отметка старше `БАРС_СВЕЖ_МС` — БАРСа нет.
pub fn разобрать_барс(текст: &str, сейчас: u64) -> Барс {
    let v: Value = serde_json::from_str(текст).unwrap_or_default();
    let когда = v.get("ms").and_then(Value::as_u64).unwrap_or(0);
    if когда == 0 || сейчас.saturating_sub(когда) > БАРС_СВЕЖ_МС { return Барс::default(); }
    let номер = |к: &str| v.get(к).and_then(Value::as_u64).map(|п| п as u32).filter(|п| *п != 0);
    let номера = |к: &str| v.get(к).and_then(Value::as_array).into_iter().flatten()
        .filter_map(|x| x.as_u64().map(|п| п as u32)).filter(|п| *п != 0).collect::<Vec<_>>();
    Барс {
        жив: true, окно: номер("window"), голова: номер("head"), движки: номера("engines"), счёт: номера("compute"),
        работа: v.get("work").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|x| x.as_str().map(str::to_string)).collect(),
        разум_загружен: v.get("mind_loaded").and_then(Value::as_bool).unwrap_or(false),
        разум_простой_с: v.get("mind_idle_s").and_then(Value::as_u64),
        видео: v.get("video").and_then(Value::as_bool).unwrap_or(false),
        режим: v.get("mode").and_then(Value::as_str).map(str::to_string),
    }
}

/// 🟥 25.10 (Р-195): когда БАРС последний раз был жив — его нет давно, и сигналы идут уведомлением Windows.
static БАРС_ВИДЕН_МС: AtomicU64 = AtomicU64::new(0);
static СТАРТ_МС: OnceLock<u64> = OnceLock::new();
/// БАРСа нет дольше — говорить самому (уведомлением Windows).
const БАРСА_НЕТ_МС: u64 = 10 * 60 * 1000;

pub fn прочитать_барс() -> Барс {
    let _ = СТАРТ_МС.get_or_init(сейчас_мс);
    let б = std::fs::read_to_string(папка().join("ot_barsa.json")).map(|т| разобрать_барс(&т, сейчас_мс())).unwrap_or_default();
    if б.жив { БАРС_ВИДЕН_МС.store(сейчас_мс(), Ordering::Relaxed); }
    б
}

/// Чистая (видно в тесте): БАРСа нет давно — с последней встречи (или с нашего старта, если не видели вовсе).
pub fn барса_нет_давно_по(видели: u64, старт: u64, сейчас: u64) -> bool {
    сейчас.saturating_sub(if видели == 0 { старт } else { видели }) >= БАРСА_НЕТ_МС
}

fn барса_нет_давно() -> bool {
    барса_нет_давно_по(БАРС_ВИДЕН_МС.load(Ordering::Relaxed), *СТАРТ_МС.get_or_init(сейчас_мс), сейчас_мс())
}

// ───────────────────────────── что БАРС знает о нас ─────────────────────────────

/// Отметка для БАРСа: тормозит ли ПК сейчас.
pub fn отметить(тормозит: bool) {
    let _ = записать(&папка().join("barsu.json"), &json!({ "slow": тормозит, "ms": сейчас_мс(), "pid": std::process::id(),
        "version": env!("CARGO_PKG_VERSION") }).to_string());
}

static НОМЕР: AtomicU64 = AtomicU64::new(0);

fn сообщений() -> usize {
    std::fs::read_dir(папка().join("soobshcheniya")).map(|д| д.filter_map(Result::ok)
        .filter(|ф| ф.path().extension().is_some_and(|р| р == "json")).count()).unwrap_or(0)
}

/// Положить сообщение БАРСу; вернуть его номер.
fn сообщить(тело: Value, всегда: bool) -> Option<String> {
    let папка_с = папка().join("soobshcheniya");
    if std::fs::create_dir_all(&папка_с).is_err() { return None; }
    if !всегда && сообщений() >= СООБЩЕНИЙ_МАКС { return None; }
    let номер = format!("{}-{}", сейчас_мс(), НОМЕР.fetch_add(1, Ordering::Relaxed));
    let mut т = тело;
    if let Some(о) = т.as_object_mut() { о.insert("id".into(), json!(номер)); }
    записать(&папка_с.join(format!("{номер}.json")), &т.to_string()).ok().map(|_| номер)
}

/// Строка своего журнала.
pub fn журнал(событие: &str, данные: &Value) {
    let путь = папка().join("zhurnal.jsonl");
    if std::fs::metadata(&путь).is_ok_and(|м| м.len() > ЖУРНАЛ_МАКС) {
        let _ = std::fs::rename(&путь, путь.with_extension("jsonl.old"));
    }
    let строка = json!({ "ts": chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false),
        "event": событие, "data": данные });
    if let Ok(mut ф) = std::fs::OpenOptions::new().create(true).append(true).open(&путь) {
        let _ = writeln!(ф, "{строка}");
    }
}

/// Строка и в свой журнал, и в журнал БАРСа (под тем же именем события — история тормозов в БАРСе не рвётся).
pub fn журнал_барсу(событие: &str, данные: &Value) {
    журнал(событие, данные);
    // БАРС пишет строку, когда забрал (до 2 с позже, а без БАРСа — и часы): своё время — в данных
    let mut д = данные.clone();
    if let Some(о) = д.as_object_mut() {
        о.insert("когда".into(), json!(chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false)));
    }
    сообщить(json!({ "kind": "log", "event": событие, "data": д }), false);
}

/// Сигнал разуму через БАРС (вид «пк», ему говорит разум, в переписке) — тот же ключ не чаще `раз_в`.
pub fn сказать_разуму(сказано: &mut HashMap<String, Instant>, ключ: String, раз_в: Duration, заголовок: &str, подробности: String) {
    if сказано.get(&ключ).is_some_and(|к| к.elapsed() < раз_в) { return; }
    сказано.insert(ключ.clone(), Instant::now());
    let тело = json!({ "kind": "пк", "source": "Механик", "urgent": false, "title": заголовок,
        "detail": подробности.chars().take(ЗНАКОВ_ПОВОДА).collect::<String>(), "fingerprint": ключ });
    журнал("сигнал_разуму", &тело);
    сообщить(json!({ "kind": "signal", "body": тело }), true);
    // 🟥 25.10 (Р-195): «Механик» живёт и без БАРСа — тогда сказать ему самому, уведомлением Windows
    if барса_нет_давно() {
        let ок = crate::okna::уведомить(&format!("Механик: {заголовок}"), &подробности.chars().take(ЗНАКОВ_ПОВОДА).collect::<String>());
        журнал("уведомление_windows", &json!({ "заголовок": заголовок, "показано": ок, "почему": "БАРСа нет дольше 10 мин" }));
    }
}

/// Просьба БАРСу (`what`: `unload_mind` — выгрузить простаивающий 27B; `pause_min` — сколько фон 27B потом ждёт);
/// вернуть номер — по нему придёт ответ.
pub fn попросить(что: &str, почему: &str, пауза_мин: u64) -> Option<String> {
    сообщить(json!({ "kind": "request", "what": что, "why": почему, "pause_min": пауза_мин }), true)
}

/// Ответ БАРСа на просьбу: `Some((сделано, почему))`, пока ответа нет — `None`.
pub fn ответ(номер: &str) -> Option<(bool, String)> {
    let путь = папка().join("otvety").join(format!("{номер}.json"));
    let т = std::fs::read_to_string(&путь).ok()?;
    let _ = std::fs::remove_file(&путь);
    let v: Value = serde_json::from_str(&т).ok()?;
    Some((v.get("ok").and_then(Value::as_bool).unwrap_or(false), v.get("why").and_then(Value::as_str).unwrap_or("").to_string()))
}

/// Сообщения и ответы старше суток (БАРСа долго нет) — стереть.
pub fn убрать_старые() {
    for папка_с in [папка().join("soobshcheniya"), папка().join("otvety")] {
        let Ok(д) = std::fs::read_dir(папка_с) else { continue };
        for ф in д.filter_map(Result::ok) {
            let старое = ф.metadata().and_then(|м| м.modified()).ok()
                .and_then(|т| т.elapsed().ok()).is_some_and(|в| в > Duration::from_secs(24 * 3600));
            if старое { let _ = std::fs::remove_file(ф.path()); }
        }
    }
}

/// Выкладка просит выйти.
pub fn просят_выйти() -> bool { папка().join("stop.flag").exists() }
pub fn стоп_снят() { let _ = std::fs::remove_file(папка().join("stop.flag")); }

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn барс_по_отметке() {
        let т = r#"{"ms":100000,"window":10,"head":0,"engines":[20,30],"compute":[20],"work":["разум 27B"],
            "mind_loaded":true,"mind_idle_s":300,"video":true,"mode":"смотрит"}"#;
        let б = разобрать_барс(т, 110_000);
        assert!(б.жив && б.окно == Some(10) && б.голова.is_none(), "голова 0 — нет головы");
        assert_eq!((б.движки.clone(), б.счёт.clone(), б.работа.clone()), (vec![20, 30], vec![20], vec!["разум 27B".to_string()]));
        assert!(б.видео && б.режим.as_deref() == Some("смотрит") && б.разум_загружен && б.разум_простой_с == Some(300));
        assert_eq!(разобрать_барс(т, 116_000), Барс::default(), "отметке 16 с — БАРСа нет");
        assert_eq!(разобрать_барс("не json", 1), Барс::default());
    }

    #[test]
    fn барса_нет_давно() {
        assert!(!барса_нет_давно_по(0, 1_000, 1_000 + 60_000), "только запустились — ждём БАРС");
        assert!(барса_нет_давно_по(0, 1_000, 1_000 + БАРСА_НЕТ_МС), "не видели 10 мин со старта");
        assert!(!барса_нет_давно_по(500_000, 1_000, 500_000 + 30_000), "видели только что");
        assert!(барса_нет_давно_по(500_000, 1_000, 500_000 + БАРСА_НЕТ_МС));
    }
}
