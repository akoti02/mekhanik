//! 🟥 13.11 (вид «Механика»): ФАЙЛЫ СОСТОЯНИЯ — что окно читает и что пишет. Всё — в `%LOCALAPPDATA%\mekhanik\`
//! (`svyaz::папка()`), ключи и значения — латиницей (их читают и скрипты PowerShell), записи целиком (во временный,
//! потом подменой). Нет файла или он битый — окно пишет «нет данных», а не падает и не выдумывает.
//!
//! | файл | кто пишет | когда | что |
//! |---|---|---|---|
//! | `znachok.json` | Мозг | при смене, не чаще раза в 5 с | цвет значка, строка, причины, итоги дня (`Значок`) |
//! | `sejchas.json` | Мозг | раз в 2 с, пока `okno_zhivo.json` свежее 10 с | всё «сейчас» для окна (`Сводка`) |
//! | `okno_zhivo.json` | окно | раз в 5 с, пока окно видно; спрятано — стёрт | какая страница открыта (`ОкноЖиво`) |
//! | `prosba_okna.json` | окно | по нажатию в меню | ЖЕЛАЕМОЕ: пауза, остановка до входа, запуск при входе (`Просьба`) |
//! | `otvet_okna.json` | Мозг | исполнил просьбу | что принял и что сейчас (`Ответ`) |
//! | `komandy\<мс>-<n>.json` | окно | по кнопке | разовая команда (загрузки: вернуть, в корзину…; `Команда`) |
//! | `otvety_okna\<id>.json` | исполнитель | на команду | `ok`, `why` |
//! | `uvedomleniya\<мс>-<n>.json` | любой процесс «Механика» | окна нет — уведомление ждёт его | `Уведомление` |
//! | `okno.json` | окно | по смене | своё: место и размер окна, страница |
//! | `okno_zhurnal.jsonl` | окно | редко | свой журнал окна (Мозг журнал пишет сам, окно в его файл не пишет) |
//! | `stop_okno.flag` | выкладка | на время выкладки | окно выходит и Мозга не поднимает |
//!
//! Окно ещё читает (не пишет никогда): `barsu.json` (жив ли Мозг — `ms` раз в 2 с), `ot_barsa.json`, `izmeneniya.json`,
//! `zamershie.json`, `razum_nizhe.json`, `opyt.json`, `nastroyki.json`, `proba.json`, `stop.flag`, журнал
//! (`zhurnal.jsonl` или дневные `zhurnal*.jsonl`), `zagruzki\istoriya.json` и `zagruzki\sostoyanie.json`, папку
//! `soobshcheniya\` (сколько ждёт БАРСа), `C:\ProgramData\Mekhanik\videokarta.json` и `zazhim_itog.json`.

use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const ЗНАЧОК: &str = "znachok.json";
pub const СВОДКА: &str = "sejchas.json";
pub const ОКНО_ЖИВО: &str = "okno_zhivo.json";
pub const ПРОСЬБА: &str = "prosba_okna.json";
pub const ОТВЕТ: &str = "otvet_okna.json";
pub const КОМАНДЫ: &str = "komandy";
pub const ОТВЕТЫ_КОМАНД: &str = "otvety_okna";
pub const УВЕДОМЛЕНИЯ: &str = "uvedomleniya";
pub const МЕСТО_ОКНА: &str = "okno.json";
pub const ЖУРНАЛ_ОКНА: &str = "okno_zhurnal.jsonl";
pub const СТОП_ОКНА: &str = "stop_okno.flag";

/// Отметке Мозга (`barsu.json`, раз в 2 с) старше — Мозг молчит (как БАРС: `mekhanik_svyaz.rs`, 15 с).
pub const МОЗГ_СВЕЖ_МС: u64 = 15_000;
/// `okno_zhivo.json` свежее — окно открыто, Мозг пишет `sejchas.json`.
pub const ОКНО_ЖИВО_МС: u64 = 10_000;
/// Сводке старше не верим: окно показывает «нет данных», а не старые числа.
pub const СВОДКА_СВЕЖА_МС: u64 = 10_000;
/// Журнал: при первом чтении — не больше стольких байт с конца; события — не старше 48 ч.
const ЖУРНАЛ_ХВОСТ_БАЙТ: u64 = 4 << 20;
const ЖУРНАЛ_ХРАНИТЬ_МС: u64 = 48 * 3600 * 1000;
const ЖУРНАЛ_СОБЫТИЙ_МАКС: usize = 20_000;
/// Очередь уведомлений без окна — не больше стольких файлов и не старше суток.
pub const УВЕДОМЛЕНИЙ_МАКС: usize = 30;
pub const УВЕДОМЛЕНИЕ_ЖИВЁТ_МС: u64 = 24 * 3600 * 1000;

pub fn сейчас_мс() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|д| д.as_millis() as u64).unwrap_or(0)
}

/// Записать файл целиком: во временный, потом подменой — читатель не увидит половину.
pub fn записать(путь: &Path, текст: &str) -> std::io::Result<()> {
    if let Some(п) = путь.parent() { std::fs::create_dir_all(п)?; }
    let врем = путь.with_extension("tmp-okno");
    std::fs::write(&врем, текст)?;
    std::fs::rename(&врем, путь)
}

fn прочитать(путь: &Path) -> Option<Value> {
    std::fs::read_to_string(путь).ok().and_then(|т| serde_json::from_str(т.trim_start_matches('\u{feff}')).ok())
}

fn u64_(v: &Value, к: &str) -> Option<u64> { v.get(к).and_then(Value::as_u64) }
fn f64_(v: &Value, к: &str) -> Option<f64> { v.get(к).and_then(Value::as_f64) }
fn str_(v: &Value, к: &str) -> Option<String> { v.get(к).and_then(Value::as_str).map(str::to_string) }
fn bool_(v: &Value, к: &str) -> Option<bool> { v.get(к).and_then(Value::as_bool) }

// ───────────────────────────── значок: znachok.json ─────────────────────────────

/// Состояние, которое называет Мозг. «Молчит» и «пауза» окно видит само (отметка жизни, своя просьба).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Состояние { #[default] Спокойно, Работает, Внимание, Тревога }

impl Состояние {
    pub fn ключ(self) -> &'static str {
        match self { Состояние::Спокойно => "ok", Состояние::Работает => "work", Состояние::Внимание => "attention",
            Состояние::Тревога => "alarm" }
    }
    pub fn из(с: &str) -> Option<Состояние> {
        match с { "ok" => Some(Состояние::Спокойно), "work" => Some(Состояние::Работает),
            "attention" => Some(Состояние::Внимание), "alarm" => Some(Состояние::Тревога), _ => None }
    }
}

/// Почему значок такой — для карточки «Нужна твоя рука» и подсказки.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Причина { pub состояние: Состояние, pub ключ: String, pub текст: String }

/// Итоги дня — для подсказки значка («Сегодня: тормозов 12, лечил 3, всё вернул»).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Сегодня { pub тормозов: u64, pub при_нём: u64, pub вылечил: u64, pub само: u64, pub нечего: u64,
    pub всё_вернул: bool }

/// `znachok.json` v1 (пишет Мозг при смене, не чаще раза в 5 с):
/// `{"v":1,"ms":…,"state":"ok|work|attention|alarm","line":"…","reasons":[{"state":"attention","key":"disk_c",
/// "text":"…"}],"today":{"brakes":12,"at_pc":7,"cured":3,"self":7,"nothing":2,"returned_all":true},"pid":…,
/// "version":"0.6.0"}`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Значок {
    pub мс: u64, pub состояние: Состояние, pub строка: String, pub причины: Vec<Причина>, pub сегодня: Option<Сегодня>,
    pub pid: u32, pub версия: String,
}

impl Значок {
    pub fn из_json(v: &Value) -> Option<Значок> {
        if u64_(v, "v").unwrap_or(1) != 1 { return None; }
        let состояние = Состояние::из(v.get("state")?.as_str()?)?;
        let причины = v.get("reasons").and_then(Value::as_array).into_iter().flatten().filter_map(|п| Some(Причина {
            состояние: п.get("state").and_then(Value::as_str).and_then(Состояние::из).unwrap_or(Состояние::Внимание),
            ключ: str_(п, "key").unwrap_or_default(), текст: str_(п, "text")?,
        })).collect();
        let сегодня = v.get("today").filter(|с| с.is_object()).map(|с| Сегодня {
            тормозов: u64_(с, "brakes").unwrap_or(0), при_нём: u64_(с, "at_pc").unwrap_or(0),
            вылечил: u64_(с, "cured").unwrap_or(0), само: u64_(с, "self").unwrap_or(0),
            нечего: u64_(с, "nothing").unwrap_or(0), всё_вернул: bool_(с, "returned_all").unwrap_or(true),
        });
        Some(Значок { мс: u64_(v, "ms").unwrap_or(0), состояние, строка: str_(v, "line").unwrap_or_default(), причины,
            сегодня, pid: u64_(v, "pid").unwrap_or(0) as u32, версия: str_(v, "version").unwrap_or_default() })
    }
    pub fn в_json(&self) -> Value {
        let mut v = json!({ "v": 1, "ms": self.мс, "state": self.состояние.ключ(), "line": self.строка,
            "reasons": self.причины.iter().map(|п| json!({ "state": п.состояние.ключ(), "key": п.ключ, "text": п.текст }))
                .collect::<Vec<_>>(),
            "pid": self.pid, "version": self.версия });
        if let Some(с) = &self.сегодня {
            v["today"] = json!({ "brakes": с.тормозов, "at_pc": с.при_нём, "cured": с.вылечил, "self": с.само,
                "nothing": с.нечего, "returned_all": с.всё_вернул });
        }
        v
    }
}

/// Для Мозга (сведение): записать `znachok.json`.
#[allow(dead_code)] // зовёт Мозг — подключит сведение
pub fn записать_значок(папка: &Path, з: &Значок) -> std::io::Result<()> { записать(&папка.join(ЗНАЧОК), &з.в_json().to_string()) }

// ───────────────────────────── просьба окна: prosba_okna.json ─────────────────────────────

/// `prosba_okna.json` v1 — ЖЕЛАЕМОЕ состояние, которое просит владелец из меню значка (пишет только окно; Мозг читает
/// раз в 2 с и исполняет, пока желаемое не сбылось; то же прочтение — то же действие, повтор ничего не ломает):
/// `{"v":1,"ms":…,"id":"<мс>-<n>","pause_until_ms":0,"stop_logon_ms":0,"autostart":null}`
/// - `pause_until_ms` — пауза до этого мига: Мозг всё вернул и никого не трогает (ни лечения, ни уступки экрану, ни
///   заморозки, ни приоритета разуму, ни копии ПК); замер, журнал и `barsu.json` идут. 0 — паузы нет.
/// - `stop_logon_ms` — «остановить до входа»: вход сеанса (`win32::вход_сеанса_мс`), при котором он попросил. Мозг с
///   тем же входом всё возвращает и выходит; поднятый тем же входом (БАРС зовёт задачу «Mekhanik») — выходит сразу.
///   Новый вход — просьба устарела. 0 — не останавливать.
/// - `autostart` — `true`/`false`: включить или выключить задачу «Mekhanik» при входе (задача его, права не нужны);
///   `null` — не трогать.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Просьба { pub мс: u64, pub id: String, pub пауза_до_мс: u64, pub стоп_вход_мс: u64, pub при_входе: Option<bool> }

impl Просьба {
    pub fn из_json(v: &Value) -> Просьба {
        Просьба { мс: u64_(v, "ms").unwrap_or(0), id: str_(v, "id").unwrap_or_default(),
            пауза_до_мс: u64_(v, "pause_until_ms").unwrap_or(0), стоп_вход_мс: u64_(v, "stop_logon_ms").unwrap_or(0),
            при_входе: bool_(v, "autostart") }
    }
    pub fn в_json(&self) -> Value {
        json!({ "v": 1, "ms": self.мс, "id": self.id, "pause_until_ms": self.пауза_до_мс, "stop_logon_ms": self.стоп_вход_мс,
            "autostart": self.при_входе })
    }
    /// Чистая (видно в тесте): пауза идёт.
    pub fn пауза_идёт(&self, сейчас: u64) -> bool { self.пауза_до_мс > сейчас }
    /// Чистая (видно в тесте): остановлен до входа — и вход тот же. Нет входа (не узнать) — не останавливаем: лучше
    /// работать, чем молча стоять до перезагрузки.
    pub fn остановлен(&self, вход_сейчас: Option<u64>) -> bool {
        self.стоп_вход_мс != 0 && вход_сейчас.is_some_and(|в| в == self.стоп_вход_мс)
    }
}

pub fn прочитать_просьбу(папка: &Path) -> Просьба { прочитать(&папка.join(ПРОСЬБА)).map(|v| Просьба::из_json(&v)).unwrap_or_default() }

/// `otvet_okna.json` v1 (пишет Мозг, исполнив просьбу): `{"v":1,"ms":…,"id":"<id просьбы>","ok":true,"why":"…",
/// "pause_until_ms":…,"autostart":true|false|null,"stopping":false}` — `autostart` — как задача стоит на деле.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ответ { pub мс: u64, pub id: String, pub ок: bool, pub почему: String, pub пауза_до_мс: u64,
    pub при_входе: Option<bool>, pub выходит: bool }

impl Ответ {
    pub fn из_json(v: &Value) -> Ответ {
        Ответ { мс: u64_(v, "ms").unwrap_or(0), id: str_(v, "id").unwrap_or_default(), ок: bool_(v, "ok").unwrap_or(false),
            почему: str_(v, "why").unwrap_or_default(), пауза_до_мс: u64_(v, "pause_until_ms").unwrap_or(0),
            при_входе: bool_(v, "autostart"), выходит: bool_(v, "stopping").unwrap_or(false) }
    }
    pub fn в_json(&self) -> Value {
        json!({ "v": 1, "ms": self.мс, "id": self.id, "ok": self.ок, "why": self.почему, "pause_until_ms": self.пауза_до_мс,
            "autostart": self.при_входе, "stopping": self.выходит })
    }
}

/// Для Мозга (сведение): ответить окну.
#[allow(dead_code)] // зовёт Мозг — подключит сведение
pub fn записать_ответ(папка: &Path, о: &Ответ) -> std::io::Result<()> { записать(&папка.join(ОТВЕТ), &о.в_json().to_string()) }

/// `okno_zhivo.json` v1: `{"v":1,"ms":…,"pid":…,"page":"seychas"}`.
pub fn окно_живо_json(страница: &str, сейчас: u64) -> Value {
    json!({ "v": 1, "ms": сейчас, "pid": std::process::id(), "page": страница })
}

/// Чистая по сути (время — аргументом). Для Мозга (сведение): окно открыто — писать `sejchas.json` раз в 2 с.
#[allow(dead_code)] // зовёт Мозг — подключит сведение
pub fn окно_открыто(папка: &Path, сейчас: u64) -> bool {
    прочитать(&папка.join(ОКНО_ЖИВО)).and_then(|v| u64_(&v, "ms")).is_some_and(|м| сейчас.saturating_sub(м) <= ОКНО_ЖИВО_МС)
}

// ───────────────────────────── команды окна: komandy\ ─────────────────────────────

/// Разовая команда окна (по кнопке): `komandy\<мс>-<n>.json` `{"v":1,"id":"<мс>-<n>","ms":…,"what":"…","arg":"…"}`.
/// `what`: `zagruzki_vernut`, `zagruzki_v_korzinu`, `zagruzki_prosmotreno` (`arg` — `id` записи сторожа загрузок; пути
/// решает тот, кто держит карантин, а не окно), `videokarta_snova` (попросить лечение зажима ещё раз). Исполнитель
/// стирает файл и отвечает в `otvety_okna\<id>.json` `{"ok":…,"why":"…"}`. Окно не умеет и уметь не будет: закрыть или
/// убить процесс, тронуть БАРС, писать БАРСу, менять настройки Windows.
pub fn положить_команду(папка: &Path, что: &str, арг: &str, номер: u64) -> Option<String> {
    let м = сейчас_мс();
    let id = format!("{м}-{номер}");
    let тело = json!({ "v": 1, "id": id, "ms": м, "what": что, "arg": арг });
    записать(&папка.join(КОМАНДЫ).join(format!("{id}.json")), &тело.to_string()).ok().map(|_| id)
}

/// Ответ на команду: `Some((сделано, почему))`, пока нет — `None`.
#[allow(dead_code)] // окну — когда исполнители команд появятся (сведение)
pub fn ответ_команды(папка: &Path, id: &str) -> Option<(bool, String)> {
    let путь = папка.join(ОТВЕТЫ_КОМАНД).join(format!("{id}.json"));
    let v = прочитать(&путь)?;
    let _ = std::fs::remove_file(&путь);
    Some((bool_(&v, "ok").unwrap_or(false), str_(&v, "why").unwrap_or_default()))
}

// ───────────────────────────── уведомления без окна: uvedomleniya\ ─────────────────────────────

/// Вид уведомления: сведение — тихо; внимание и тревога — со звуком, если он за ПК.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Вид { #[default] Сведение, Внимание, Тревога }

impl Вид {
    pub fn ключ(self) -> &'static str { match self { Вид::Сведение => "info", Вид::Внимание => "attention", Вид::Тревога => "alarm" } }
    pub fn из(с: &str) -> Вид { match с { "alarm" => Вид::Тревога, "attention" => Вид::Внимание, _ => Вид::Сведение } }
}

/// Уведомление: `{"v":1,"ms":…,"title":"…","text":"…","kind":"info|attention|alarm","page":"zagruzki"}` — и в
/// `WM_COPYDATA` окну (тот же JSON), и в очередь `uvedomleniya\`, когда окна нет.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Уведомление { pub мс: u64, pub заголовок: String, pub текст: String, pub вид: Вид, pub страница: Option<String> }

impl Уведомление {
    pub fn из_json(v: &Value) -> Option<Уведомление> {
        Some(Уведомление { мс: u64_(v, "ms").unwrap_or(0), заголовок: str_(v, "title")?, текст: str_(v, "text").unwrap_or_default(),
            вид: Вид::из(v.get("kind").and_then(Value::as_str).unwrap_or("")), страница: str_(v, "page") })
    }
    pub fn в_json(&self) -> Value {
        json!({ "v": 1, "ms": self.мс, "title": self.заголовок, "text": self.текст, "kind": self.вид.ключ(), "page": self.страница })
    }
}

/// Положить в очередь (окна нет). Очередь полна — не кладём: уведомления не копятся без конца.
pub fn в_очередь(папка: &Path, у: &Уведомление, номер: u64) -> bool {
    let п = папка.join(УВЕДОМЛЕНИЯ);
    // 🟥 14.11 (аудит): считать только уведомления (*.json) — недописанные `.tmp-okno` после сбоя занимали места
    // в очереди навсегда, и при 30 таких очередь молча отказывала (и тревоге о карантине тоже); их — подмести
    let есть = std::fs::read_dir(&п).map(|д| д.filter_map(Result::ok).filter(|ф| {
        let путь = ф.path();
        if путь.extension().is_some_and(|р| р == "tmp-okno") {
            if ф.metadata().ok().and_then(|м| м.modified().ok()).and_then(|в| в.elapsed().ok())
                .is_some_and(|д| д > std::time::Duration::from_secs(60)) { let _ = std::fs::remove_file(&путь); }
            return false;
        }
        путь.extension().is_some_and(|р| р == "json")
    }).count()).unwrap_or(0);
    if есть >= УВЕДОМЛЕНИЙ_МАКС { return false; }
    записать(&п.join(format!("{}-{номер}.json", у.мс)), &у.в_json().to_string()).is_ok()
}

/// Забрать очередь (окно поднялось): свежие — по порядку, старше суток — стереть молча.
pub fn забрать_очередь(папка: &Path, сейчас: u64) -> Vec<Уведомление> {
    let Ok(д) = std::fs::read_dir(папка.join(УВЕДОМЛЕНИЯ)) else { return Vec::new() };
    let mut файлы: Vec<PathBuf> = д.filter_map(Result::ok).map(|ф| ф.path())
        .filter(|п| п.extension().is_some_and(|р| р == "json")).collect();
    файлы.sort();
    let mut итог = Vec::new();
    for ф in файлы {
        let у = прочитать(&ф).and_then(|v| Уведомление::из_json(&v));
        let _ = std::fs::remove_file(&ф);
        if let Some(у) = у { if сейчас.saturating_sub(у.мс) < УВЕДОМЛЕНИЕ_ЖИВЁТ_МС { итог.push(у); } }
    }
    итог
}

// ───────────────────────────── сводка «сейчас»: sejchas.json ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Верх { pub pid: u32, pub имя: String, pub доля: f64, pub барс: bool, pub видеопамять_мб: Option<u64> }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Сейчас {
    pub гпу_мс: Vec<f64>, pub цп_опоздал_мс: Option<f64>, pub цп_весь: Option<f64>, pub озу_своб_мб: Option<u64>,
    pub озу_всего_мб: Option<u64>, pub выделено_своб_мб: Option<u64>, pub выделено_предел_мб: Option<u64>,
    pub подкачка_с: Option<f64>, pub драйверы: Option<f64>, pub диск_занят: Option<f64>, pub c_своб_гб: Option<u64>,
    pub за_пк: Option<bool>, pub ввод_с: Option<u64>, pub видео: Option<bool>, pub проба: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Тормоз { pub с_мс: u64, pub признаки: Vec<String>, pub причина: String, pub барс: bool, pub окон: u64 }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Лечение { pub класс: String, pub ступень: String, pub кому: String, pub с_мс: u64, pub заранее: bool }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Нвидиа {
    pub от_сети: Option<bool>, pub видеопамять_мб: Option<u64>, pub предел_вт: Option<f64>, pub обычный_вт: Option<f64>,
    pub ступень: Option<String>, pub зажата_с: Option<u64>, pub просил: u64, pub установлено: Option<bool>,
    pub итог: Option<String>, pub итог_мс: Option<u64>, pub верх: Vec<Верх>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Час { pub доля_ядра: Option<f64>, pub тормозов: u64, pub лечений: u64, pub помогло: u64, pub уступок: u64 }

/// Процесс глазами Мозга (`kto`, если есть): класс и род — для «Кто ест».
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ПроцессМозга { pub pid: u32, pub создан: u64, pub имя: String, pub класс: Option<КлассПроцесса>,
    pub род: Option<String>, pub почему: Option<String>, pub экран: Option<f64>, pub нвидиа: Option<f64>,
    pub видеопамять_мб: Option<u64>, pub метка: Option<String> }

/// `sejchas.json` v1 (пишет Мозг раз в 2 с, пока окно открыто):
/// `{"v":1,"ms":…,"pid":…,"version":"…","since_ms":<старт Мозга>,`
/// ` "now":{"gpu_wait_ms":[…],"cpu_late_ms":…,"cpu_all":0.23,"ram_free_mb":…,"ram_total_mb":…,"commit_free_mb":…,`
/// `        "commit_limit_mb":…,"pagein_s":…,"drivers_pct":…,"disk_busy_pct":…,"c_free_gb":…,"at_pc":true,"input_s":3,`
/// `        "video":false,"proba":false},`
/// ` "brake":{"since_ms":…,"signs":["видеокарта"],"culprit":"…","bars":false,"windows":5}|null,`
/// ` "cure":{"class":"…","step":"…","who":"…","since_ms":…,"ahead":false}|null,`
/// ` "hold":{"builds_low":7,"copy_slow":false,"engines_gpu_low":2},`
/// ` "screen":{"adapter":"Intel(R) Arc(TM) 140T GPU","top":[{"pid":…,"name":"dwm.exe","pct":9.0,"bars":false}]},`
/// ` "nvidia":{"on_ac":true,"vram_mb":…,"limit_w":96,"default_w":80,"pstate":"P0","clamped_s":null,"asked":0,`
/// `           "installed":true,"result":"ne_nuzhno","result_ms":…,"top":[{"pid":…,"name":"…","pct":…,"vram_mb":…}]},`
/// ` "hour":{"core_pct":0.66,"brakes":6,"cures":9,"helped":3,"yields":77},`
/// ` "chart":{"step_s":2,"gpu_max":[…]},`
/// ` "procs":[{"pid":…,"created":…,"name":"…","class":"on|work|bars|bg|system|self","kind":"сборка","why":"…",`
/// `           "gpu_screen_pct":…,"gpu_nvidia_pct":…,"vram_mb":…,"mark":"…"}]}`
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Сводка {
    pub мс: u64, pub pid: u32, pub версия: String, pub с_мс: Option<u64>, pub сейчас: Сейчас, pub тормоз: Option<Тормоз>,
    pub лечение: Option<Лечение>, pub сборкам_ниже: u64, pub копия_медленнее: bool, pub движкам_ниже: u64,
    pub экран_имя: Option<String>, pub экран_верх: Vec<Верх>, pub нвидиа: Option<Нвидиа>, pub час: Option<Час>,
    pub график: Vec<f64>, pub график_шаг_с: u64, pub процессы: Vec<ПроцессМозга>,
}

fn верх(v: Option<&Value>) -> Vec<Верх> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(|п| Some(Верх { pid: u64_(п, "pid").unwrap_or(0) as u32,
        имя: str_(п, "name")?, доля: f64_(п, "pct").unwrap_or(0.0), барс: bool_(п, "bars").unwrap_or(false),
        видеопамять_мб: u64_(п, "vram_mb") })).collect()
}

fn числа(v: Option<&Value>) -> Vec<f64> { v.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_f64).collect() }
fn строки(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect()
}

impl Сводка {
    pub fn из_json(v: &Value) -> Option<Сводка> {
        if u64_(v, "v").unwrap_or(1) != 1 { return None; }
        let мс = u64_(v, "ms")?;
        let н = v.get("now").cloned().unwrap_or_default();
        let сейчас = Сейчас { гпу_мс: числа(н.get("gpu_wait_ms")), цп_опоздал_мс: f64_(&н, "cpu_late_ms"),
            цп_весь: f64_(&н, "cpu_all"), озу_своб_мб: u64_(&н, "ram_free_mb"), озу_всего_мб: u64_(&н, "ram_total_mb"),
            выделено_своб_мб: u64_(&н, "commit_free_mb"), выделено_предел_мб: u64_(&н, "commit_limit_mb"),
            подкачка_с: f64_(&н, "pagein_s"), драйверы: f64_(&н, "drivers_pct"), диск_занят: f64_(&н, "disk_busy_pct"),
            c_своб_гб: u64_(&н, "c_free_gb"), за_пк: bool_(&н, "at_pc"), ввод_с: u64_(&н, "input_s"),
            видео: bool_(&н, "video"), проба: bool_(&н, "proba").unwrap_or(false) };
        let тормоз = v.get("brake").filter(|т| т.is_object()).map(|т| Тормоз { с_мс: u64_(т, "since_ms").unwrap_or(мс),
            признаки: строки(т.get("signs")), причина: str_(т, "culprit").unwrap_or_default(),
            барс: bool_(т, "bars").unwrap_or(false), окон: u64_(т, "windows").unwrap_or(0) });
        let лечение = v.get("cure").filter(|т| т.is_object()).map(|т| Лечение { класс: str_(т, "class").unwrap_or_default(),
            ступень: str_(т, "step").unwrap_or_default(), кому: str_(т, "who").unwrap_or_default(),
            с_мс: u64_(т, "since_ms").unwrap_or(мс), заранее: bool_(т, "ahead").unwrap_or(false) });
        let д = v.get("hold").cloned().unwrap_or_default();
        let э = v.get("screen").cloned().unwrap_or_default();
        let нвидиа = v.get("nvidia").filter(|т| т.is_object()).map(|т| Нвидиа { от_сети: bool_(т, "on_ac"),
            видеопамять_мб: u64_(т, "vram_mb"), предел_вт: f64_(т, "limit_w"), обычный_вт: f64_(т, "default_w"),
            ступень: str_(т, "pstate"), зажата_с: u64_(т, "clamped_s"), просил: u64_(т, "asked").unwrap_or(0),
            установлено: bool_(т, "installed"), итог: str_(т, "result"), итог_мс: u64_(т, "result_ms"), верх: верх(т.get("top")) });
        let час = v.get("hour").filter(|т| т.is_object()).map(|т| Час { доля_ядра: f64_(т, "core_pct"),
            тормозов: u64_(т, "brakes").unwrap_or(0), лечений: u64_(т, "cures").unwrap_or(0),
            помогло: u64_(т, "helped").unwrap_or(0), уступок: u64_(т, "yields").unwrap_or(0) });
        let г = v.get("chart").cloned().unwrap_or_default();
        let процессы = v.get("procs").and_then(Value::as_array).into_iter().flatten().filter_map(|п| Some(ПроцессМозга {
            pid: u64_(п, "pid")? as u32, создан: u64_(п, "created").unwrap_or(0), имя: str_(п, "name")?,
            класс: п.get("class").and_then(Value::as_str).and_then(КлассПроцесса::из), род: str_(п, "kind"),
            почему: str_(п, "why"), экран: f64_(п, "gpu_screen_pct"), нвидиа: f64_(п, "gpu_nvidia_pct"),
            видеопамять_мб: u64_(п, "vram_mb"), метка: str_(п, "mark") })).collect();
        Some(Сводка { мс, pid: u64_(v, "pid").unwrap_or(0) as u32, версия: str_(v, "version").unwrap_or_default(),
            с_мс: u64_(v, "since_ms"), сейчас, тормоз, лечение, сборкам_ниже: u64_(&д, "builds_low").unwrap_or(0),
            копия_медленнее: bool_(&д, "copy_slow").unwrap_or(false), движкам_ниже: u64_(&д, "engines_gpu_low").unwrap_or(0),
            экран_имя: str_(&э, "adapter"), экран_верх: верх(э.get("top")), нвидиа, час, график: числа(г.get("gpu_max")),
            график_шаг_с: u64_(&г, "step_s").unwrap_or(2), процессы })
    }
}

// ───────────────────────────── жив ли Мозг ─────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ЖизньМозга {
    /// Отметка свежа.
    Жив,
    /// Процесс есть (мьютекс), а отметки нет столько секунд — завис или стоит.
    Молчит(u64),
    /// Нет ни процесса, ни свежей отметки.
    #[default]
    НеЗапущен,
    /// Он сам остановил до входа (`prosba_okna.json`).
    Остановлен,
    /// Идёт выкладка (`stop.flag` или `stop_okno.flag`) — Мозга не поднимаем.
    Выкладка,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Мозг { pub жизнь: ЖизньМозга, pub pid: Option<u32>, pub версия: Option<String>, pub отметка_мс: u64,
    pub тормозит: bool }

/// Чистая (видно в тесте): жив ли Мозг — по отметке `barsu.json`, мьютексу и просьбам.
pub fn жизнь_мозга(отметка_мс: u64, мьютекс: bool, остановлен: bool, выкладка: bool, сейчас: u64) -> ЖизньМозга {
    let свежа = отметка_мс != 0 && сейчас.saturating_sub(отметка_мс) <= МОЗГ_СВЕЖ_МС;
    if свежа { return ЖизньМозга::Жив; }
    if выкладка { return ЖизньМозга::Выкладка; }
    if остановлен { return ЖизньМозга::Остановлен; }
    if мьютекс {
        let молчит = if отметка_мс == 0 { 0 } else { сейчас.saturating_sub(отметка_мс) / 1000 };
        return ЖизньМозга::Молчит(молчит);
    }
    ЖизньМозга::НеЗапущен
}

/// Поднимать Мозг: молчит или не запущен дольше `МОЛЧИТ_ПОДНЯТЬ_С`; не чаще раза в `ПОДНИМАТЬ_РАЗ_В_С`; после
/// `ПОПЫТОК_МАКС` неудач — не поднимаем, тревога (сказать ему: «Механик не работает»).
pub const МОЛЧИТ_ПОДНЯТЬ_С: u64 = 60;
pub const ПОДНИМАТЬ_РАЗ_В_С: u64 = 300;
pub const ПОПЫТОК_МАКС: u32 = 3;

/// Чистая (видно в тесте): поднять ли Мозг сейчас. `молчит_с` — сколько он уже не отзывается (по нашим наблюдениям);
/// `попыток` — подряд без успеха; `прошлая_с` — сколько секунд с прошлой попытки (`None` — не было).
pub fn поднять_ли(жизнь: ЖизньМозга, молчит_с: u64, попыток: u32, прошлая_с: Option<u64>) -> bool {
    let можно = matches!(жизнь, ЖизньМозга::Молчит(_) | ЖизньМозга::НеЗапущен);
    можно && молчит_с >= МОЛЧИТ_ПОДНЯТЬ_С && попыток < ПОПЫТОК_МАКС && прошлая_с.is_none_or(|с| с >= ПОДНИМАТЬ_РАЗ_В_С)
}

// ───────────────────────────── что держит: izmeneniya, zamershie, razum_nizhe ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Изменение { pub pid: u32, pub имя: String, pub что: String, pub разработка: bool }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Держит { pub изменения: Vec<Изменение>, pub замершие: Vec<u32>, pub замершие_с_мс: Option<u64>,
    pub разум_ниже: Option<u32> }

impl Держит {
    pub fn пусто(&self) -> bool { self.изменения.is_empty() && self.замершие.is_empty() && self.разум_ниже.is_none() }
}

/// Чистая (видно в тесте): разобрать три файла книги (тексты; `None` — файла нет).
pub fn разобрать_держит(изменения: Option<&str>, замершие: Option<&str>, разум: Option<&str>) -> Держит {
    let mut д = Держит::default();
    if let Some(v) = изменения.and_then(|т| serde_json::from_str::<Value>(т).ok()) {
        д.изменения = v.as_array().into_iter().flatten().map(|з| Изменение { pid: u64_(з, "pid").unwrap_or(0) as u32,
            имя: str_(з, "name").unwrap_or_default(), что: str_(з, "what").unwrap_or_default(),
            разработка: bool_(з, "dev").unwrap_or(false) }).collect();
    }
    if let Some(v) = замершие.and_then(|т| serde_json::from_str::<Value>(т).ok()) {
        д.замершие = v.get("pids").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).map(|п| п as u32).collect();
        д.замершие_с_мс = v.get("когда").and_then(Value::as_str).and_then(мс_из_rfc3339);
    }
    д.разум_ниже = разум.and_then(|т| serde_json::from_str::<Value>(т).ok()).and_then(|v| u64_(&v, "pid")).map(|п| п as u32);
    д
}

// ───────────────────────────── опыт: opyt.json ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ОпытСтупени { pub класс: String, pub ступень: String, pub раз: u64, pub помогло: u64, pub выключена: bool }

/// Чистая (видно в тесте): `opyt.json` — «класс / ступень» → раз, помогло, с какого мига; выключена — 6 раз без пользы
/// за сутки (`lechenie::ОПЫТ_ПОПЫТОК`, `ОПЫТ_СУТКИ`).
pub fn разобрать_опыт(текст: &str, сейчас: u64) -> Vec<ОпытСтупени> {
    let v: Value = serde_json::from_str(текст).unwrap_or_default();
    let mut итог: Vec<ОпытСтупени> = v.as_object().into_iter().flatten().map(|(к, з)| {
        let (класс, ступень) = к.split_once(" / ").map(|(а, б)| (а.to_string(), б.to_string())).unwrap_or((к.clone(), String::new()));
        let (раз, помогло, с) = (u64_(з, "раз").unwrap_or(0), u64_(з, "помогло").unwrap_or(0), u64_(з, "с_мс").unwrap_or(0));
        ОпытСтупени { класс, ступень, раз, помогло, выключена: раз >= 6 && помогло == 0 && сейчас.saturating_sub(с) < 24 * 3600 * 1000 }
    }).collect();
    итог.sort_by(|а, б| (&а.класс, &а.ступень).cmp(&(&б.класс, &б.ступень)));
    итог
}

// ───────────────────────────── журнал ─────────────────────────────

/// Строка журнала Мозга: когда, событие, данные.
#[derive(Clone, Debug, PartialEq)]
pub struct Событие { pub мс: u64, pub имя: String, pub д: Value }

pub fn мс_из_rfc3339(с: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(с).ok().map(|т| т.timestamp_millis()).filter(|м| *м > 0).map(|м| м as u64)
}

/// Какие события окну нужны (прочие — мимо: журнал до 999 строк в день, держать всё незачем).
fn нужно_событие(имя: &str) -> bool {
    имя.starts_with("механик_") || имя.starts_with("тормоз_") || имя.starts_with("видеокарта_")
        || matches!(имя, "пк_спал" | "утечка_пк" | "диск_почти_полон" | "сигнал_разуму" | "сигнал_разуму_проба"
            | "уведомление_windows" | "сторож_тормозов_час")
}

/// Чистая (видно в тесте): разобрать строку журнала. У большого `тормоз_пк` берём только то, что показывает окно.
pub fn разобрать_строку(с: &str) -> Option<Событие> {
    let v: Value = serde_json::from_str(с.trim()).ok()?;
    let имя = v.get("event")?.as_str()?.to_string();
    if !нужно_событие(&имя) { return None; }
    let мс = мс_из_rfc3339(v.get("ts")?.as_str()?)?;
    let mut д = v.get("data").cloned().unwrap_or_else(|| json!({}));
    if имя == "тормоз_пк" {
        let мало: Map<String, Value> = ["за_пк", "виновник", "ввод_с", "видео", "барс"].iter()
            .filter_map(|к| д.get(*к).map(|з| (к.to_string(), з.clone()))).collect();
        д = Value::Object(мало);
    }
    Some(Событие { мс, имя, д })
}

/// Хвост журнала: первый раз — до 4 МБ с конца, потом — только дописанное. Журнал сменился (стал короче или другой
/// файл) — читаем заново.
#[derive(Default)]
pub struct ХвостЖурнала { путь: Option<PathBuf>, прочитано: u64, неполное: Vec<u8>, pub события: Arc<Vec<Событие>> }

/// Файл журнала: `zhurnal.jsonl`; нет его (журнал по дням) — самый свежий `zhurnal*.jsonl` в папке или в `zhurnal\`.
pub fn файл_журнала(папка: &Path) -> Option<PathBuf> {
    let основной = папка.join("zhurnal.jsonl");
    if основной.exists() { return Some(основной); }
    let mut лучший: Option<(std::time::SystemTime, PathBuf)> = None;
    for п in [папка.to_path_buf(), папка.join("zhurnal")] {
        let Ok(д) = std::fs::read_dir(&п) else { continue };
        for ф in д.filter_map(Result::ok) {
            let путь = ф.path();
            let имя = путь.file_name().and_then(|и| и.to_str()).unwrap_or("").to_lowercase();
            if !(имя.starts_with("zhurnal") && имя.ends_with(".jsonl")) { continue; }
            let Ok(когда) = ф.metadata().and_then(|м| м.modified()) else { continue };
            if лучший.as_ref().is_none_or(|(к, _)| когда > *к) { лучший = Some((когда, путь)); }
        }
    }
    лучший.map(|(_, п)| п)
}

impl ХвостЖурнала {
    /// Дочитать; `true` — появилось новое.
    pub fn обновить(&mut self, папка: &Path, сейчас: u64) -> bool {
        use std::io::{Read, Seek, SeekFrom};
        let Some(путь) = файл_журнала(папка) else { return false };
        let длина = std::fs::metadata(&путь).map(|м| м.len()).unwrap_or(0);
        let другой = self.путь.as_ref() != Some(&путь) || длина < self.прочитано;
        if !другой && длина == self.прочитано { return false; }
        let mut события: Vec<Событие> = if другой { Vec::new() } else { (*self.события).clone() };
        let mut от = if другой { длина.saturating_sub(ЖУРНАЛ_ХВОСТ_БАЙТ) } else { self.прочитано };
        if другой { self.неполное.clear(); }
        let Ok(mut ф) = std::fs::File::open(&путь) else { return false };
        if ф.seek(SeekFrom::Start(от)).is_err() { return false; }
        let mut буф = Vec::with_capacity((длина - от) as usize);
        if ф.take(длина - от).read_to_end(&mut буф).is_err() { return false; }
        // начали с середины файла — первая строка неполная
        let mut куски: Vec<u8> = std::mem::take(&mut self.неполное);
        if другой && от > 0 {
            match буф.iter().position(|&б| б == b'\n') { Some(н) => { буф.drain(..=н); } None => буф.clear() }
        }
        куски.extend_from_slice(&буф);
        let конец = куски.iter().rposition(|&б| б == b'\n').map(|н| н + 1).unwrap_or(0);
        self.неполное = куски[конец..].to_vec();
        for строка in куски[..конец].split(|&б| б == b'\n') {
            if строка.is_empty() { continue; }
            if let Some(с) = std::str::from_utf8(строка).ok().and_then(разобрать_строку) { события.push(с); }
        }
        от = длина;
        let граница = сейчас.saturating_sub(ЖУРНАЛ_ХРАНИТЬ_МС);
        события.retain(|с| с.мс >= граница);
        if события.len() > ЖУРНАЛ_СОБЫТИЙ_МАКС { let лишних = события.len() - ЖУРНАЛ_СОБЫТИЙ_МАКС; события.drain(..лишних); }
        self.путь = Some(путь);
        self.прочитано = от;
        self.события = Arc::new(события);
        true
    }
}

// ───────────────────────────── случаи: тормоз → что сделал → чем кончилось ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Исход {
    /// Ещё идёт (или журнал оборвался).
    #[default]
    Идёт,
    /// Помогло через столько секунд (доказано: тормоз не моложе 20 с).
    Помогло(u64),
    /// Молодой тормоз прошёл после действия — не доказано, что от «Механика».
    ПрошлоПосле,
    /// Не помогло — пробовал дальше.
    НеПомогло,
    /// Делал, а больше сделать было нечего.
    Исчерпал,
    /// Лечить было нечего (93 % «исчерпал» — пустая лестница).
    Нечего,
    /// Прошло само (лечение ещё не начиналось).
    Само,
    /// Его не было за ПК — не лечил.
    БезНего,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Случай {
    pub начало_мс: u64, pub конец_мс: Option<u64>, pub длилось_с: Option<u64>, pub заранее: bool,
    pub при_нём: Option<bool>, pub признаки: Vec<String>, pub причина: String, pub сделано: Vec<String>,
    pub кому: Vec<String>, pub исход: Исход, pub вернул: Vec<(u64, String)>,
}

/// Вернул спустя не больше — к случаю.
const ВЕРНУЛ_К_СЛУЧАЮ_МС: u64 = 30 * 60 * 1000;

/// Чистая (видно в тесте): склеить строки журнала в случаи (новые — в конце). `тормоз_пк` → `механик_лечит` →
/// `механик_помогло` / `_прошло_после` / `_не_помогло` / `_исчерпал` → `механик_вернул` → `тормоз_пк_прошёл`.
pub fn случаи(события: &[Событие]) -> Vec<Случай> {
    let mut итог: Vec<Случай> = Vec::new();
    // открытый случай — номер в `итог`
    let mut открыт: Option<usize> = None;
    for с in события {
        let д = &с.д;
        match с.имя.as_str() {
            "тормоз_пк" => {
                открыт = Some(итог.len());
                итог.push(Случай { начало_мс: с.мс, при_нём: bool_(д, "за_пк"), причина: str_(д, "виновник").unwrap_or_default(),
                    ..Default::default() });
            }
            "механик_лечит" => {
                let заранее = bool_(д, "заранее").unwrap_or(false);
                let н = match открыт {
                    Some(н) if !(заранее && итог[н].конец_мс.is_some()) => н,
                    _ => {
                        открыт = Some(итог.len());
                        итог.push(Случай { начало_мс: с.мс, заранее: true, при_нём: Some(true), ..Default::default() });
                        итог.len() - 1
                    }
                };
                let ступень = str_(д, "ступень").unwrap_or_default();
                let кого = str_(д, "кого").unwrap_or_default();
                итог[н].сделано.push(if кого.is_empty() { ступень } else { format!("{ступень}: {кого}") });
                if let Some(к) = д.get("подробно").and_then(|п| п.get("кого")).and_then(Value::as_array) {
                    итог[н].кому.extend(к.iter().filter_map(|x| x.as_str().map(str::to_string)).filter(|к| !к.is_empty()));
                } else if !кого.is_empty() { итог[н].кому.push(кого); }
                if итог[н].признаки.is_empty() { if let Some(к) = str_(д, "класс") { итог[н].признаки.push(к); } }
                итог[н].исход = Исход::Идёт;
            }
            "механик_помогло" | "механик_прошло_после" | "механик_не_помогло" | "механик_исчерпал" | "механик_прошло_само"
            | "механик_ждёт" => {
                let Some(н) = открыт else { continue };
                let ничего = д.get("сделано").and_then(Value::as_array).is_some_and(|с| с.is_empty());
                итог[н].исход = match с.имя.as_str() {
                    "механик_помогло" => Исход::Помогло(u64_(д, "через_с").unwrap_or(0)),
                    "механик_прошло_после" => Исход::ПрошлоПосле,
                    "механик_не_помогло" => Исход::НеПомогло,
                    "механик_исчерпал" if ничего && итог[н].сделано.is_empty() => Исход::Нечего,
                    "механик_исчерпал" => Исход::Исчерпал,
                    "механик_прошло_само" => Исход::Само,
                    _ => Исход::БезНего,
                };
                if итог[н].признаки.is_empty() { if let Some(к) = str_(д, "класс") { итог[н].признаки.push(к); } }
                // «заранее» кончается своим итогом — тормоза-то не было
                if итог[н].заранее && с.имя != "механик_не_помогло" {
                    итог[н].конец_мс = Some(с.мс);
                    открыт = None;
                }
            }
            "тормоз_пк_прошёл" => {
                let длилось = u64_(д, "длилось_с");
                let н = match открыт {
                    Some(н) if !итог[н].заранее => н,
                    _ => {
                        // хвост журнала начался посреди тормоза
                        итог.push(Случай { начало_мс: с.мс.saturating_sub(длилось.unwrap_or(0) * 1000), ..Default::default() });
                        итог.len() - 1
                    }
                };
                let сл = &mut итог[н];
                сл.конец_мс = Some(с.мс);
                сл.длилось_с = длилось;
                сл.признаки = строки(д.get("признаки"));
                if let Some(в) = str_(д, "виновник") { сл.причина = в; }
                if сл.исход == Исход::Идёт && сл.сделано.is_empty() { сл.исход = Исход::Само; }
                открыт = None;
            }
            "механик_вернул" => {
                let кому = str_(д, "кому").unwrap_or_default();
                let почему = str_(д, "почему").unwrap_or_default();
                let к_случаю = итог.iter().rposition(|сл| с.мс.saturating_sub(сл.начало_мс) <= ВЕРНУЛ_К_СЛУЧАЮ_МС
                    // 🟥 14.11 (аудит): пустое «кому» в случае ловило бы ЛЮБОЙ возврат («...».contains("") — всегда да)
                    && сл.кому.iter().filter(|к| !к.is_empty())
                        .any(|к| к.eq_ignore_ascii_case(&кому) || кому.contains(к.as_str())));
                if let Some(н) = к_случаю { итог[н].вернул.push((с.мс, format!("{кому} — {почему}"))); }
            }
            _ => {}
        }
    }
    итог
}

/// Итоги дня (с полуночи по его времени) — по строкам журнала.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ИтогиДня { pub тормозов: u64, pub при_нём: u64, pub вылечил: u64, pub прошло_после: u64, pub нечего: u64,
    pub само: u64, pub вернул: u64, pub сигналов: u64 }

/// Чистая (видно в тесте): итоги с мига `с_мс`.
pub fn итоги(события: &[Событие], с_мс: u64) -> ИтогиДня {
    let сегодня: Vec<Событие> = события.iter().filter(|с| с.мс >= с_мс).cloned().collect();
    let mut и = ИтогиДня::default();
    for с in &сегодня {
        match с.имя.as_str() {
            "тормоз_пк" => { и.тормозов += 1; if bool_(&с.д, "за_пк").unwrap_or(false) { и.при_нём += 1; } }
            "механик_помогло" => и.вылечил += 1,
            "механик_прошло_после" => и.прошло_после += 1,
            "механик_вернул" => и.вернул += 1,
            "сигнал_разуму" => и.сигналов += 1,
            _ => {}
        }
    }
    for сл in случаи(&сегодня) {
        match сл.исход { Исход::Нечего => и.нечего += 1, Исход::Само => и.само += 1, _ => {} }
    }
    и
}

/// Полночь сегодня по местному времени, мс.
pub fn полночь_мс(сейчас: u64) -> u64 {
    use chrono::{Local, TimeZone};
    let Some(т) = Local.timestamp_millis_opt(сейчас as i64).single() else { return сейчас };
    т.date_naive().and_hms_opt(0, 0, 0).and_then(|п| Local.from_local_datetime(&п).earliest())
        .map(|п| п.timestamp_millis() as u64).unwrap_or(сейчас)
}

// ───────────────────────────── видеокарта ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Видеокарта {
    /// Лечение зажима стоит (`C:\ProgramData\Mekhanik\videokarta.json`): когда поставлено.
    pub установлено: Option<String>,
    /// Последний итог задачи (`zazhim_itog.json`): итог, когда, предел до и после, обычный.
    pub итог: Option<(String, u64, Option<u64>, Option<u64>, Option<u64>)>,
    /// Последний замер из журнала: предел, обычный, ступень, видеопамять, зажата.
    pub замер: Option<(Option<f64>, Option<f64>, Option<String>, Option<u64>, bool, u64)>,
    /// Эпизоды зажима из журнала: начало, конец, попыток лечения, итог словами.
    pub эпизоды: Vec<(u64, Option<u64>, u64, String, bool)>,
}

/// Чистая (видно в тесте): видеокарта по файлам системы и журналу.
pub fn разобрать_видеокарту(установка: Option<&str>, итог: Option<&str>, события: &[Событие]) -> Видеокарта {
    let mut в = Видеокарта::default();
    if let Some(v) = установка.and_then(|т| serde_json::from_str::<Value>(т.trim_start_matches('\u{feff}')).ok()) {
        в.установлено = Some(str_(&v, "ustanovleno").unwrap_or_default());
    }
    if let Some(v) = итог.and_then(|т| serde_json::from_str::<Value>(т.trim_start_matches('\u{feff}')).ok()) {
        if let Some(и) = str_(&v, "itog") {
            в.итог = Some((и, v.get("kogda").and_then(Value::as_str).and_then(мс_из_rfc3339).unwrap_or(0),
                u64_(&v, "predel_do"), u64_(&v, "predel_posle"), u64_(&v, "obychnyy")));
        }
    }
    let mut открыт: Option<usize> = None;
    for с in события {
        let данные = с.д.get("данные").unwrap_or(&с.д);
        let проба = bool_(&с.д, "проба").unwrap_or(false);
        match с.имя.as_str() {
            "видеокарта_замер" => в.замер = Some((f64_(данные, "предел_вт"), f64_(данные, "обычный_вт"),
                str_(данные, "ступень"), u64_(данные, "видеопамять_мб"), bool_(данные, "зажата").unwrap_or(false), с.мс)),
            "видеокарта_зажата" => {
                // на пробе (`proba_zazhim.json`) числа выдуманы — замером их не показываем
                if !проба {
                    в.замер = Some((f64_(данные, "предел_вт"), f64_(данные, "обычный_вт"), str_(данные, "ступень"), None, true, с.мс));
                }
                открыт = Some(в.эпизоды.len());
                в.эпизоды.push((с.мс, None, 0, String::new(), проба));
            }
            "видеокарта_лечение_запрошено" => { if let Some(н) = открыт { в.эпизоды[н].2 += 1; } }
            "видеокарта_лечение" => {
                if let Some(н) = открыт { в.эпизоды[н].3 = str_(&с.д, "итог").unwrap_or_default(); }
            }
            "видеокарта_лечение_не_установлено" => { if let Some(н) = открыт { в.эпизоды[н].3 = "ne_ustanovleno".into(); } }
            "видеокарта_отпущена" | "видеокарта_проба_кончилась" => {
                if let Some(н) = открыт.take() { в.эпизоды[н].1 = Some(с.мс); }
                if с.имя == "видеокарта_отпущена" { if let Some(з) = в.замер.as_mut() { з.4 = false; } }
            }
            _ => {}
        }
    }
    в
}

// ───────────────────────────── загрузки: zagruzki\istoriya.json ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Загрузка {
    pub id: String, pub файл: String, pub состояние: String, pub заголовок: String, pub причина: String,
    pub когда_мс: u64, pub размер: Option<u64>, pub сайт: Option<String>, pub род: Option<String>, pub тест: bool,
    pub просмотрено: bool, pub карантин: Option<String>, pub шаги: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Загрузки { pub записи: Vec<Загрузка>, pub папки: Vec<String>, pub проверки: Vec<String>, pub очередь: u64,
    pub сейчас: Option<String> }

/// Чистая (видно в тесте): `zagruzki\istoriya.json` (массив или `{"items":[…]}`; запись: `id`, `file`, `state`
/// (`checking|clean|held|danger|returned|trash|missing`), `title`, `reason`, `steps[]`, `at_ms`, `size`, `site`,
/// `kind`, `test`, `seen`, `quar`) и `zagruzki\sostoyanie.json` (`folders[]`, `checks[]`, `queue`, `now`).
pub fn разобрать_загрузки(история: &str, состояние: Option<&str>) -> Option<Загрузки> {
    let v: Value = serde_json::from_str(история.trim_start_matches('\u{feff}')).ok()?;
    let записи = v.as_array().or_else(|| v.get("items").and_then(Value::as_array))?;
    let mut з: Vec<Загрузка> = записи.iter().filter_map(|з| Some(Загрузка {
        id: str_(з, "id")?, файл: str_(з, "file").unwrap_or_default(), состояние: str_(з, "state").unwrap_or_default(),
        заголовок: str_(з, "title").unwrap_or_default(), причина: str_(з, "reason").unwrap_or_default(),
        когда_мс: u64_(з, "at_ms").unwrap_or(0), размер: u64_(з, "size"), сайт: str_(з, "site"), род: str_(з, "kind"),
        тест: bool_(з, "test").unwrap_or(false), просмотрено: bool_(з, "seen").unwrap_or(false), карантин: str_(з, "quar"),
        шаги: з.get("steps").and_then(Value::as_array).into_iter().flatten().filter_map(|ш| ш.as_str().map(str::to_string)
            .or_else(|| str_(ш, "text"))).collect(),
    })).collect();
    з.sort_by(|а, б| б.когда_мс.cmp(&а.когда_мс));
    let с: Value = состояние.and_then(|т| serde_json::from_str(т.trim_start_matches('\u{feff}')).ok()).unwrap_or_default();
    Some(Загрузки { записи: з, папки: строки(с.get("folders")),
        проверки: с.get("checks").and_then(Value::as_array).into_iter().flatten()
            .filter_map(|ч| ч.as_str().map(str::to_string).or_else(|| str_(ч, "name"))).collect(),
        очередь: u64_(&с, "queue").unwrap_or(0), сейчас: str_(&с, "now") })
}

// ───────────────────────────── БАРС: ot_barsa.json ─────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct БарсОкна {
    pub отметка_мс: u64, pub окно: Option<u32>, pub голова: Option<u32>, pub движки: Vec<u32>, pub счёт: Vec<u32>,
    pub работа: Vec<String>, pub разум_загружен: Option<bool>, pub разум_простой_с: Option<u64>, pub видео: Option<bool>,
    pub режим: Option<String>, pub разум_pid: Option<u32>, pub слово: Option<bool>,
    /// Каких полей договора нет (их «Механик» читает, а БАРС не пишет).
    pub нет_полей: Vec<&'static str>,
}

/// Чистая (видно в тесте): `ot_barsa.json` как есть — окно показывает и старую отметку (с её возрастом).
pub fn разобрать_барса(текст: &str) -> Option<БарсОкна> {
    let v: Value = serde_json::from_str(текст).ok()?;
    let номер = |к: &str| u64_(&v, к).map(|п| п as u32).filter(|п| *п != 0);
    let номера = |к: &str| v.get(к).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64)
        .map(|п| п as u32).filter(|п| *п != 0).collect::<Vec<_>>();
    let нет_полей = ["mind_pid", "word"].into_iter().filter(|к| v.get(*к).is_none()).collect();
    Some(БарсОкна { отметка_мс: u64_(&v, "ms").unwrap_or(0), окно: номер("window"), голова: номер("head"),
        движки: номера("engines"), счёт: номера("compute"), работа: строки(v.get("work")),
        разум_загружен: bool_(&v, "mind_loaded"), разум_простой_с: u64_(&v, "mind_idle_s"), видео: bool_(&v, "video"),
        режим: str_(&v, "mode"), разум_pid: номер("mind_pid"), слово: bool_(&v, "word"), нет_полей })
}

impl БарсОкна {
    pub fn все_pid(&self) -> Vec<u32> {
        let mut в: Vec<u32> = self.окно.into_iter().chain(self.голова).chain(self.движки.iter().copied())
            .chain(self.счёт.iter().copied()).chain(self.разум_pid).collect();
        в.sort_unstable();
        в.dedup();
        в
    }
}

// ───────────────────────────── «Кто ест»: классы в его порядке ─────────────────────────────

/// Классы в его порядке ресурсов: он → его работа → БАРС → фон; Windows и сам «Механик» — отдельно.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum КлассПроцесса { ПередТобой, ТвояРабота, Барс, Фон, Система, Механик }

impl КлассПроцесса {
    pub const ВСЕ: [КлассПроцесса; 6] = [КлассПроцесса::ПередТобой, КлассПроцесса::ТвояРабота, КлассПроцесса::Барс,
        КлассПроцесса::Фон, КлассПроцесса::Система, КлассПроцесса::Механик];
    pub fn из(с: &str) -> Option<КлассПроцесса> {
        match с { "on" => Some(КлассПроцесса::ПередТобой), "work" => Some(КлассПроцесса::ТвояРабота),
            "bars" => Some(КлассПроцесса::Барс), "bg" => Some(КлассПроцесса::Фон), "system" => Some(КлассПроцесса::Система),
            "self" => Some(КлассПроцесса::Механик), _ => None }
    }
    pub fn слово(self) -> &'static str {
        match self { КлассПроцесса::ПередТобой => "Перед тобой", КлассПроцесса::ТвояРабота => "Твоя работа",
            КлассПроцесса::Барс => "БАРС", КлассПроцесса::Фон => "Твои программы в фоне",
            КлассПроцесса::Система => "Windows и службы", КлассПроцесса::Механик => "Механик" }
    }
}

/// Системные — по имени (как неприкасаемые лечения; окно только показывает, ничего не трогает).
const СИСТЕМНЫЕ: &[&str] = &["system", "idle", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe",
    "services.exe", "lsass.exe", "svchost.exe", "dwm.exe", "explorer.exe", "audiodg.exe", "fontdrvhost.exe", "sihost.exe",
    "ctfmon.exe", "msmpeng.exe", "securityhealthservice.exe", "securityhealthsystray.exe", "taskhostw.exe",
    "runtimebroker.exe", "shellexperiencehost.exe", "startmenuexperiencehost.exe", "textinputhost.exe", "searchhost.exe",
    "lockapp.exe", "logonui.exe", "conhost.exe", "memory compression", "secure system", "vmmem", "dllhost.exe",
    "wmiprvse.exe", "searchindexer.exe", "nissrv.exe", "spoolsv.exe", "widgets.exe", "phoneexperiencehost.exe"];
/// Сборщики — по имени (подсказка поверх поведения; как `storozh::СБОРКИ`).
const СБОРЩИКИ: &[&str] = &["rustc.exe", "cargo.exe", "link.exe", "rust-lld.exe", "lld-link.exe", "cl.exe", "java.exe",
    "ninja.exe", "cmake.exe", "msbuild.exe", "qemu-system-x86_64.exe", "emulator.exe", "node.exe", "python.exe"];
/// Тяжёлое по поведению — его работа (САПР, рендер, расчёт, игра…): ядер или ОЗУ не меньше.
pub const ТЯЖЁЛОЕ_ЯДЕР: f64 = 1.0;
pub const ТЯЖЁЛОЕ_ОЗУ_МБ: u64 = 1500;
/// 🟥 Его слово 13.11: «Фильмы, сериалы и что-либо подобное я не считаю за тяжёлые процессы» — проигрыватели и браузеры
/// (там видео) «тяжёлой работой» по поведению не становятся никогда; впереди — «Перед тобой», сзади — фон.
const ВИДЕО_И_БРАУЗЕРЫ: &[&str] = &["vivaldi.exe", "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "brave.exe",
    "vlc.exe", "mpc-hc64.exe", "mpc-be64.exe", "potplayermini64.exe", "mpv.exe", "wmplayer.exe", "microsoft.media.player.exe",
    "video.ui.exe", "kodi.exe", "plex.exe", "netflix.exe"];

/// Процесс глазами окна (свой снимок раз в 2 с, пока «Кто ест» видна).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ПроцессОкна { pub pid: u32, pub родитель: u32, pub имя: String, pub ядер: f64, pub озу_мб: u64,
    pub выделено_мб: u64, pub диск_мб_с: f64, pub свой_сеанс: bool }

pub struct Окружение<'а> { pub передний: Option<u32>, pub барс: &'а [u32], pub окна_разработки: &'а [String], pub свой_pid: u32 }

/// Потомки набора (до 8 колен).
fn потомки(все: &[ПроцессОкна], корни: &HashSet<u32>) -> HashSet<u32> {
    let mut набор = корни.clone();
    for _ in 0..8 {
        let дети: Vec<u32> = все.iter().filter(|п| !набор.contains(&п.pid) && набор.contains(&п.родитель) && п.родитель > 4)
            .map(|п| п.pid).collect();
        if дети.is_empty() { break; }
        набор.extend(дети);
    }
    набор
}

/// Чистая (видно в тесте): класс каждого процесса — сам «Механик» → Windows → перед тобой (окно впереди, его потомки и
/// тёзки) → БАРС (по отметке, с потомками) → твоя работа (из окон разработки, сборщики, тяжёлое по поведению) → фон.
pub fn классы(все: &[ПроцессОкна], о: &Окружение) -> HashMap<u32, КлассПроцесса> {
    let передний_имя = о.передний.and_then(|п| все.iter().find(|x| x.pid == п)).map(|x| x.имя.to_lowercase());
    let передние = потомки(все, &о.передний.into_iter().collect());
    let барс = потомки(все, &о.барс.iter().copied().collect());
    let окна_раз: HashSet<u32> = все.iter().filter(|п| о.окна_разработки.iter().any(|и| и.eq_ignore_ascii_case(&п.имя)))
        .map(|п| п.pid).collect();
    let разработка = потомки(все, &окна_раз);
    все.iter().map(|п| {
        let н = п.имя.to_lowercase();
        let к = if п.pid == о.свой_pid || н == "mekhanik.exe" { КлассПроцесса::Механик }
            else if !п.свой_сеанс || п.pid <= 4 || СИСТЕМНЫЕ.contains(&н.as_str()) { КлассПроцесса::Система }
            else if передние.contains(&п.pid) || передний_имя.as_deref() == Some(н.as_str()) { КлассПроцесса::ПередТобой }
            else if барс.contains(&п.pid) { КлассПроцесса::Барс }
            else if разработка.contains(&п.pid) || СБОРЩИКИ.contains(&н.as_str())
                || !ВИДЕО_И_БРАУЗЕРЫ.contains(&н.as_str()) && (п.ядер >= ТЯЖЁЛОЕ_ЯДЕР || п.озу_мб >= ТЯЖЁЛОЕ_ОЗУ_МБ) {
                КлассПроцесса::ТвояРабота
            }
            else { КлассПроцесса::Фон };
        (п.pid, к)
    }).collect()
}

/// Строка «Кто ест»: программа (тёзки вместе) внутри класса.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct СтрокаКто { pub имя: String, pub штук: u32, pub ядер: f64, pub озу_мб: u64, pub выделено_мб: u64,
    pub диск_мб_с: f64, pub экран: Option<f64>, pub нвидиа: Option<f64>, pub видеопамять_мб: Option<u64>,
    pub метка: Option<String>, pub род: Option<String> }

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ГруппаКто { pub класс: Option<КлассПроцесса>, pub строки: Vec<СтрокаКто>, pub итог: СтрокаКто }

/// Чистая (видно в тесте): разложить процессы по классам и программам; классы — в его порядке, внутри — кто больше ест.
/// `мозг` — процессы глазами Мозга (`kto`): если там класс или род есть — они главнее своих.
pub fn разложить(все: &[ПроцессОкна], о: &Окружение, мозг: &[ПроцессМозга], держит: &Держит) -> Vec<ГруппаКто> {
    let свои = классы(все, о);
    let по_мозгу: HashMap<u32, &ПроцессМозга> = мозг.iter().map(|п| (п.pid, п)).collect();
    let mut группы: Vec<ГруппаКто> = КлассПроцесса::ВСЕ.iter().map(|к| ГруппаКто { класс: Some(*к), ..Default::default() }).collect();
    let mut по_имени: HashMap<(КлассПроцесса, String), СтрокаКто> = HashMap::new();
    for п in все {
        let м = по_мозгу.get(&п.pid);
        let класс = м.and_then(|м| м.класс).or_else(|| свои.get(&п.pid).copied()).unwrap_or(КлассПроцесса::Фон);
        let с = по_имени.entry((класс, п.имя.to_lowercase())).or_insert_with(|| СтрокаКто { имя: п.имя.clone(), ..Default::default() });
        с.штук += 1;
        с.ядер += п.ядер;
        с.озу_мб += п.озу_мб;
        с.выделено_мб += п.выделено_мб;
        с.диск_мб_с += п.диск_мб_с;
        if let Some(м) = м {
            if let Some(э) = м.экран { *с.экран.get_or_insert(0.0) += э; }
            if let Some(н) = м.нвидиа { *с.нвидиа.get_or_insert(0.0) += н; }
            if let Some(в) = м.видеопамять_мб { *с.видеопамять_мб.get_or_insert(0) += в; }
            if с.метка.is_none() { с.метка = м.метка.clone(); }
            if с.род.is_none() { с.род = м.род.clone(); }
        }
        if с.метка.is_none() {
            с.метка = метка_из_книги(п.pid, держит).or(match класс {
                КлассПроцесса::ПередТобой | КлассПроцесса::Система => Some("не трогает".into()),
                КлассПроцесса::Механик => Some("сам".into()),
                _ => None,
            });
        }
    }
    for ((класс, _), строка) in по_имени {
        if let Some(г) = группы.iter_mut().find(|г| г.класс == Some(класс)) { г.строки.push(строка); }
    }
    for г in &mut группы {
        г.строки.sort_by(|а, б| б.ядер.total_cmp(&а.ядер).then(б.озу_мб.cmp(&а.озу_мб)).then(а.имя.cmp(&б.имя)));
        let mut и = СтрокаКто { имя: г.класс.map(|к| к.слово().to_string()).unwrap_or_default(), ..Default::default() };
        for с in &г.строки {
            и.штук += с.штук; и.ядер += с.ядер; и.озу_мб += с.озу_мб; и.выделено_мб += с.выделено_мб; и.диск_мб_с += с.диск_мб_с;
        }
        г.итог = и;
    }
    группы.retain(|г| !г.строки.is_empty());
    группы
}

/// Что «Механик» сделал с процессом — по книге возврата (словами для окна).
fn метка_из_книги(pid: u32, д: &Держит) -> Option<String> {
    if д.замершие.contains(&pid) { return Some("придержан (пауза, не сбой)".into()); }
    if д.разум_ниже == Some(pid) { return Some("разум думает в промежутках".into()); }
    // коротко: столбец «Механик» узкий
    д.изменения.iter().find(|з| з.pid == pid).map(|з| match з.что.as_str() {
        "ПриоритетФона" => "процессор — во 2-ю очередь".to_string(),
        "ДискФона" => "диск — в последнюю очередь".to_string(),
        "ВидеокартаФона" => "видеокарта — во 2-ю очередь".to_string(),
        д => д.to_string(),
    })
}

// ───────────────────────────── вся картина для окна ─────────────────────────────

/// Свои замеры окна — дешёвые, без Мозга (чтобы «Сейчас» не пустовала, пока Мозг не публикует сводку).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Свои { pub озу_своб_мб: Option<u64>, pub озу_всего_мб: Option<u64>, pub выделено_своб_мб: Option<u64>,
    pub выделено_предел_мб: Option<u64>, pub c_своб_гб: Option<u64>, pub ввод_с: Option<u64>, pub цп_весь: Option<f64> }

#[derive(Clone, Debug, Default)]
pub struct Картина {
    pub мс: u64,
    pub мозг: Мозг,
    pub значок: Option<Значок>,
    pub сводка: Option<Сводка>,
    pub просьба: Просьба,
    pub ответ: Option<Ответ>,
    pub свои: Свои,
    pub держит: Держит,
    pub опыт: Vec<ОпытСтупени>,
    pub события: Arc<Vec<Событие>>,
    pub видеокарта: Видеокарта,
    pub загрузки: Option<Загрузки>,
    pub сторож_отправки: Option<u64>,
    pub барс: Option<БарсОкна>,
    pub имена: Arc<HashMap<u32, String>>,
    pub сообщений_барсу: Option<usize>,
    pub окна_разработки: Vec<String>,
    pub проба: bool,
    /// Когда Мозг запущен — по журналу (`механик_запущен`), пока сводка не говорит сама.
    pub мозг_с_мс: Option<u64>,
    pub кто: Option<Arc<Vec<ГруппаКто>>>,
    /// Возраст файлов связи, с (для «Настроек»): имя → секунд назад.
    pub возраст: Vec<(&'static str, Option<u64>)>,
    /// Окно трижды поднимало Мозг — без толку (тревога и уведомление «Механик не работает»).
    pub не_поднимается: bool,
}

/// Чистая (видно в тесте): `nastroyki.json` — окна разработки.
pub fn окна_разработки(текст: &str) -> Vec<String> {
    serde_json::from_str::<Value>(текст).ok().map(|v| строки(v.get("dev_windows"))).unwrap_or_default()
}

/// Сколько секунд назад менялся файл.
pub fn возраст_с(путь: &Path) -> Option<u64> {
    std::fs::metadata(путь).and_then(|м| м.modified()).ok().and_then(|т| т.elapsed().ok()).map(|д| д.as_secs())
}

/// Папка «рук» с правами системы (`C:\ProgramData\Mekhanik`).
pub fn папка_системы() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData")).join("Mekhanik")
}

/// Сторож загрузок «Отправки» жив (`guard-state.json` свежий) — сколько секунд назад отзывался.
pub fn сторож_отправки_с() -> Option<u64> {
    let п = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?.join("KinoKiry.Otpravka").join("guard-state.json");
    возраст_с(&п).filter(|с| *с < 600)
}

#[cfg(test)]
mod тесты {
    use super::*;

    fn событие(мс: u64, имя: &str, д: Value) -> Событие { Событие { мс, имя: имя.into(), д } }

    #[test]
    fn значок_туда_и_обратно() {
        let з = Значок { мс: 5, состояние: Состояние::Внимание, строка: "диск C: свободно 17 ГБ".into(),
            причины: vec![Причина { состояние: Состояние::Внимание, ключ: "disk_c".into(), текст: "что удалить — решаешь ты".into() }],
            сегодня: Some(Сегодня { тормозов: 12, при_нём: 7, вылечил: 3, само: 7, нечего: 2, всё_вернул: true }),
            pid: 19508, версия: "0.6.0".into() };
        assert_eq!(Значок::из_json(&з.в_json()), Some(з.clone()));
        let v: Value = serde_json::from_str(r#"{"v":1,"ms":1,"state":"alarm","line":"опасный файл"}"#).unwrap();
        let р = Значок::из_json(&v).expect("без причин и итогов — тоже значок");
        assert_eq!((р.состояние, р.строка.as_str(), р.сегодня.is_none()), (Состояние::Тревога, "опасный файл", true));
        assert!(Значок::из_json(&json!({"v":1,"state":"красный"})).is_none(), "чужое состояние — не верим");
        assert!(Значок::из_json(&json!({"v":2,"state":"ok"})).is_none(), "чужая версия — не верим");
        assert!(Значок::из_json(&json!({})).is_none());
    }

    #[test]
    fn просьба_пауза_и_остановка() {
        let п = Просьба { мс: 1, id: "1-1".into(), пауза_до_мс: 10_000, стоп_вход_мс: 777, при_входе: Some(false) };
        assert_eq!(Просьба::из_json(&п.в_json()), п);
        assert!(п.пауза_идёт(9_999) && !п.пауза_идёт(10_000));
        assert!(п.остановлен(Some(777)), "тот же вход — стоим");
        assert!(!п.остановлен(Some(778)), "новый вход — просьба устарела");
        assert!(!п.остановлен(None), "вход не узнать — работаем, а не стоим молча");
        assert!(!Просьба::default().остановлен(Some(0)), "0 — не останавливать");
        assert_eq!(Просьба::из_json(&json!({"autostart": null})).при_входе, None, "null — не трогать");
        let о = Ответ { мс: 2, id: "1-1".into(), ок: true, почему: "пауза до 23:15".into(), пауза_до_мс: 10_000,
            при_входе: Some(true), выходит: false };
        assert_eq!(Ответ::из_json(&о.в_json()), о);
    }

    #[test]
    fn файлы_просьбы_окна_и_очереди() {
        let п = std::env::temp_dir().join(format!("mekhanik-vid-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&п);
        std::fs::create_dir_all(&п).unwrap();
        assert_eq!(прочитать_просьбу(&п), Просьба::default(), "нет файла — ничего не просили");
        assert!(!окно_открыто(&п, 1_000));
        записать(&п.join(ОКНО_ЖИВО), &окно_живо_json("kto", 1_000).to_string()).unwrap();
        assert!(окно_открыто(&п, 1_000 + ОКНО_ЖИВО_МС) && !окно_открыто(&п, 1_001 + ОКНО_ЖИВО_МС));
        let у = Уведомление { мс: сейчас_мс(), заголовок: "Опасный файл в карантине".into(), текст: "setup.exe".into(),
            вид: Вид::Тревога, страница: Some("zagruzki".into()) };
        assert!(в_очередь(&п, &у, 1) && в_очередь(&п, &Уведомление { мс: 1, ..у.clone() }, 2));
        let взято = забрать_очередь(&п, сейчас_мс());
        assert_eq!(взято, vec![у], "старше суток — стёрто молча");
        assert!(забрать_очередь(&п, сейчас_мс()).is_empty(), "забрали — пусто");
        let id = положить_команду(&п, "zagruzki_vernut", "ab12", 7).expect("команда");
        let т = std::fs::read_to_string(п.join(КОМАНДЫ).join(format!("{id}.json"))).unwrap();
        assert!(т.contains("\"what\":\"zagruzki_vernut\"") && т.contains("\"arg\":\"ab12\""));
        assert_eq!(ответ_команды(&п, &id), None);
        записать(&п.join(ОТВЕТЫ_КОМАНД).join(format!("{id}.json")), r#"{"ok":true,"why":"вернул"}"#).unwrap();
        assert_eq!(ответ_команды(&п, &id), Some((true, "вернул".into())));
        let _ = std::fs::remove_dir_all(&п);
    }

    #[test]
    fn сводка_по_схеме() {
        let т = r#"{"v":1,"ms":100,"pid":19508,"version":"0.6.0","since_ms":50,
            "now":{"gpu_wait_ms":[1.0,1.1,2.0],"cpu_late_ms":3,"cpu_all":0.23,"ram_free_mb":8900,"ram_total_mb":32150,
                   "commit_free_mb":36100,"commit_limit_mb":73100,"c_free_gb":67,"at_pc":true,"input_s":3,"video":false},
            "brake":{"since_ms":90,"signs":["видеокарта"],"culprit":"vivaldi.exe 63 %","bars":false,"windows":5},
            "cure":null,
            "hold":{"builds_low":7,"copy_slow":true,"engines_gpu_low":2},
            "screen":{"adapter":"Intel(R) Arc(TM) 140T GPU","top":[{"pid":13588,"name":"dwm.exe","pct":9.0,"bars":false}]},
            "nvidia":{"on_ac":true,"vram_mb":1200,"limit_w":96,"default_w":80,"pstate":"P0","asked":0,"installed":true,
                      "result":"ne_nuzhno"},
            "hour":{"core_pct":0.66,"brakes":6},
            "chart":{"step_s":2,"gpu_max":[1,2,3]},
            "procs":[{"pid":5,"name":"Fusion360.exe","class":"work","kind":"САПР","vram_mb":6100},{"name":"без номера"}]}"#;
        let с = Сводка::из_json(&serde_json::from_str(т).unwrap()).expect("сводка");
        assert_eq!((с.pid, с.с_мс, с.сейчас.гпу_мс.len(), с.сейчас.озу_всего_мб), (19508, Some(50), 3, Some(32150)));
        assert_eq!(с.тормоз.as_ref().map(|т| (т.признаки.clone(), т.окон)), Some((vec!["видеокарта".to_string()], 5)));
        assert!(с.лечение.is_none(), "null — не лечит");
        assert_eq!((с.сборкам_ниже, с.копия_медленнее, с.движкам_ниже), (7, true, 2));
        assert_eq!(с.экран_верх[0].имя, "dwm.exe");
        assert_eq!(с.нвидиа.as_ref().and_then(|н| н.предел_вт), Some(96.0));
        assert_eq!(с.час.as_ref().map(|ч| (ч.доля_ядра, ч.лечений)), Some((Some(0.66), 0)), "нет поля — ноль");
        assert_eq!(с.график, vec![1.0, 2.0, 3.0]);
        assert_eq!(с.процессы.len(), 1, "процесс без номера — мимо");
        assert_eq!((с.процессы[0].класс, с.процессы[0].род.as_deref()), (Some(КлассПроцесса::ТвояРабота), Some("САПР")));
        assert!(Сводка::из_json(&json!({"v":1})).is_none(), "без ms — не сводка");
        assert!(Сводка::из_json(&json!({"v":2,"ms":1})).is_none());
        assert!(Сводка::из_json(&json!({"ms":1})).is_some(), "минимальная — тоже сводка: всё «нет данных»");
    }

    #[test]
    fn жив_ли_мозг() {
        assert_eq!(жизнь_мозга(100_000, true, false, false, 110_000), ЖизньМозга::Жив);
        assert_eq!(жизнь_мозга(100_000, true, false, false, 140_000), ЖизньМозга::Молчит(40), "процесс есть, отметки нет 40 с");
        assert_eq!(жизнь_мозга(100_000, false, false, false, 140_000), ЖизньМозга::НеЗапущен);
        assert_eq!(жизнь_мозга(0, false, true, false, 140_000), ЖизньМозга::Остановлен);
        assert_eq!(жизнь_мозга(0, false, true, true, 140_000), ЖизньМозга::Выкладка, "выкладка главнее");
        assert_eq!(жизнь_мозга(139_000, false, true, true, 140_000), ЖизньМозга::Жив, "свежая отметка главнее всего");
        // поднимать: молчит минуту, раз в 5 мин, не больше трёх раз
        assert!(!поднять_ли(ЖизньМозга::Молчит(59), 59, 0, None));
        assert!(поднять_ли(ЖизньМозга::Молчит(60), 60, 0, None));
        assert!(поднять_ли(ЖизньМозга::НеЗапущен, 61, 2, Some(300)));
        assert!(!поднять_ли(ЖизньМозга::НеЗапущен, 61, 1, Some(299)), "раз в 5 мин");
        assert!(!поднять_ли(ЖизньМозга::НеЗапущен, 600, 3, Some(900)), "три неудачи — тревога, а не четвёртая");
        assert!(!поднять_ли(ЖизньМозга::Остановлен, 600, 0, None), "он остановил — не поднимаем");
        assert!(!поднять_ли(ЖизньМозга::Выкладка, 600, 0, None), "выкладка — не мешать копированию exe");
        assert!(!поднять_ли(ЖизньМозга::Жив, 600, 0, None));
    }

    #[test]
    fn книга_и_опыт() {
        let д = разобрать_держит(
            Some(r#"[{"pid":10,"created":1,"name":"vivaldi.exe","what":"ВидеокартаФона","was":2,"dev":false}]"#),
            Some(r#"{"pids":[20,21],"когда":"2026-11-13T21:00:00+00:00","почему":"…"}"#),
            Some(r#"{"pid":30,"created":2,"was":16384}"#));
        assert_eq!((д.изменения.len(), д.замершие.clone(), д.разум_ниже), (1, vec![20, 21], Some(30)));
        assert_eq!(д.замершие_с_мс, Some(1_794_603_600_000));
        assert!(!д.пусто() && разобрать_держит(None, None, None).пусто(), "нет файлов — ничего не держит");
        assert!(разобрать_держит(Some("не json"), None, None).пусто());
        let т = r#"{"зачисленная память / сборки замереть":{"помогло":0,"раз":6,"с_мс":1000},
                    "видеокарта / фону — видеокарта ниже":{"помогло":1,"раз":1,"с_мс":1000}}"#;
        let о = разобрать_опыт(т, 2000);
        assert_eq!(о.len(), 2);
        assert_eq!((о[0].класс.as_str(), о[0].ступень.as_str(), о[0].выключена), ("видеокарта", "фону — видеокарта ниже", false));
        assert!(о[1].выключена, "6 раз без пользы — выключена на сутки");
        assert!(!разобрать_опыт(т, 1000 + 24 * 3600 * 1000)[1].выключена, "сутки прошли — снова берётся");
    }

    #[test]
    fn строка_журнала() {
        let т = r#"{"data":{"барс":false,"виновник":"vivaldi.exe 47 %","за_пк":true,"память":[1,2,3]},"event":"тормоз_пк","ts":"2026-11-13T21:24:22.942+00:00"}"#;
        let с = разобрать_строку(т).expect("строка");
        // 2026-11-13 00:00 UTC — 1 794 528 000 000 мс; плюс 21:24:22,942
        assert_eq!((с.имя.as_str(), с.мс), ("тормоз_пк", 1_794_605_062_942));
        assert!(с.д.get("память").is_none(), "лишнее большого тормоза — не держим");
        assert_eq!(с.д.get("за_пк"), Some(&json!(true)));
        assert!(разобрать_строку(r#"{"event":"сторож_тормозов_очередь","ts":"2026-11-13T21:24:22+00:00","data":{}}"#).is_none(),
            "окну не нужно — мимо");
        assert!(разобрать_строку("{\"event\":\"тормоз_пк\",\"ts\":\"вчера\"}").is_none());
        assert!(разобрать_строку("{\"data\":{\"ба").is_none(), "оборванная строка");
    }

    #[test]
    fn хвост_журнала_дочитывает() {
        let п = std::env::temp_dir().join(format!("mekhanik-vid-zhurnal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&п);
        std::fs::create_dir_all(&п).unwrap();
        let строка = |имя: &str, сек: u32| format!("{{\"data\":{{}},\"event\":\"{имя}\",\"ts\":\"2026-11-13T21:00:{сек:02}+00:00\"}}\n");
        let mut х = ХвостЖурнала::default();
        assert!(!х.обновить(&п, 1_794_603_700_000), "журнала нет");
        let путь = п.join("zhurnal.jsonl");
        std::fs::write(&путь, строка("механик_запущен", 1) + &строка("тормоз_пк", 2) + "{\"data\":{},\"ev").unwrap();
        assert!(х.обновить(&п, 1_794_603_700_000));
        assert_eq!(х.события.len(), 2, "неполная строка ждёт конца");
        let mut ф = std::fs::OpenOptions::new().append(true).open(&путь).unwrap();
        use std::io::Write;
        ф.write_all(b"ent\":\"\xD0\xBF\xD0\xBA_\xD1\x81\xD0\xBF\xD0\xB0\xD0\xBB\",\"ts\":\"2026-11-13T21:00:03+00:00\"}\n").unwrap();
        drop(ф);
        assert!(х.обновить(&п, 1_794_603_700_000));
        assert_eq!(х.события.iter().map(|с| с.имя.as_str()).collect::<Vec<_>>(), ["механик_запущен", "тормоз_пк", "пк_спал"]);
        assert!(!х.обновить(&п, 1_794_603_700_000), "ничего нового");
        // журнал по дням: основного нет — самый свежий zhurnal*.jsonl
        std::fs::remove_file(&путь).unwrap();
        std::fs::create_dir_all(п.join("zhurnal")).unwrap();
        std::fs::write(п.join("zhurnal").join("zhurnal-2026-11-13.jsonl"), строка("тормоз_пк_прошёл", 4)).unwrap();
        assert!(х.обновить(&п, 1_794_603_700_000));
        assert_eq!(х.события.len(), 1, "другой файл — читаем заново");
        // через 49 ч дописано новое — старое забыто
        let день = п.join("zhurnal").join("zhurnal-2026-11-13.jsonl");
        let mut ф = std::fs::OpenOptions::new().append(true).open(&день).unwrap();
        ф.write_all(b"{\"data\":{},\"event\":\"\xD0\xBC\xD0\xB5\xD1\x85\xD0\xB0\xD0\xBD\xD0\xB8\xD0\xBA_\xD0\xB2\xD1\x8B\xD1\x88\xD0\xB5\xD0\xBB\",\"ts\":\"2026-11-15T22:00:00+00:00\"}\n").unwrap();
        drop(ф);
        assert!(х.обновить(&п, 1_794_780_000_000));
        assert_eq!(х.события.iter().map(|с| с.имя.as_str()).collect::<Vec<_>>(), ["механик_вышел"], "старше 48 ч — забыто");
        let _ = std::fs::remove_dir_all(&п);
    }

    #[test]
    fn случаи_из_журнала() {
        let с = [
            событие(1_000, "тормоз_пк", json!({"за_пк": true, "виновник": "видеокарта: vivaldi.exe 63 %"})),
            событие(21_000, "механик_лечит", json!({"класс": "видеокарта", "ступень": "фону — видеокарта ниже", "кого": "vivaldi.exe", "заранее": false})),
            событие(29_000, "механик_помогло", json!({"через_с": 8, "класс": "видеокарта"})),
            событие(30_000, "тормоз_пк_прошёл", json!({"длилось_с": 29, "признаки": ["видеокарта"], "виновник": "видеокарта: vivaldi.exe 63 %"})),
            событие(200_000, "механик_вернул", json!({"кому": "vivaldi.exe", "почему": "затихла"})),
            // пустая лестница
            событие(300_000, "тормоз_пк", json!({"за_пк": true, "виновник": "dwm.exe 19 %"})),
            событие(305_000, "механик_исчерпал", json!({"класс": "видеокарта", "сделано": []})),
            событие(310_000, "тормоз_пк_прошёл", json!({"длилось_с": 10, "признаки": ["видеокарта"]})),
            // прошло само
            событие(400_000, "тормоз_пк", json!({"за_пк": false})),
            событие(405_000, "тормоз_пк_прошёл", json!({"длилось_с": 5, "признаки": ["процессор"]})),
            // заранее
            событие(500_000, "механик_лечит", json!({"класс": "память (ОЗУ)", "ступень": "выжать память простаивающих",
                "кого": "2 программы", "заранее": true, "подробно": {"кого": ["claude.exe", "rclone.exe"]}})),
            событие(502_000, "механик_прошло_после", json!({})),
            // хвост начался посреди тормоза
            событие(600_000, "тормоз_пк_прошёл", json!({"длилось_с": 7, "признаки": ["диск"]})),
        ];
        let сл = случаи(&с);
        assert_eq!(сл.len(), 5);
        assert_eq!(сл[0].исход, Исход::Помогло(8));
        assert_eq!((сл[0].длилось_с, сл[0].при_нём), (Some(29), Some(true)));
        assert_eq!(сл[0].сделано, vec!["фону — видеокарта ниже: vivaldi.exe".to_string()]);
        assert_eq!(сл[0].вернул, vec![(200_000, "vivaldi.exe — затихла".to_string())], "возврат — к своему случаю");
        assert_eq!(сл[1].исход, Исход::Нечего, "пустая лестница — «лечить было нечего», не провал");
        assert_eq!(сл[2].исход, Исход::Само);
        assert!(сл[3].заранее && сл[3].исход == Исход::ПрошлоПосле && сл[3].конец_мс == Some(502_000));
        assert_eq!(сл[3].кому, vec!["claude.exe".to_string(), "rclone.exe".to_string()]);
        assert_eq!((сл[4].начало_мс, сл[4].признаки.clone()), (593_000, vec!["диск".to_string()]));
        let и = итоги(&с, 0);
        assert_eq!((и.тормозов, и.при_нём, и.вылечил, и.прошло_после, и.нечего, и.само, и.вернул), (3, 2, 1, 1, 1, 2, 1));
        assert_eq!(итоги(&с, 350_000).тормозов, 1, "только с полуночи");
    }

    #[test]
    fn видеокарта_по_журналу() {
        let с = [
            событие(1_000, "видеокарта_замер", json!({"видеопамять_мб": 1202, "зажата": false, "обычный_вт": 80, "предел_вт": 115, "ступень": "P8"})),
            событие(2_000, "видеокарта_зажата", json!({"данные": {"зажата_с": 60, "обычный_вт": 80, "предел_вт": 33, "ступень": "P5"}, "проба": true})),
            событие(3_000, "видеокарта_лечение_запрошено", json!({"данные": {"попытка": 1}, "проба": true})),
            событие(4_000, "видеокарта_лечение", json!({"итог": "snyat"})),
            событие(5_000, "видеокарта_отпущена", json!({})),
        ];
        let в = разобрать_видеокарту(
            Some("\u{feff}{\"versiya\":1,\"zadacha\":\"Mekhanik-Videokarta\",\"ustanovleno\":\"2026-11-13T19:48:12.5999681+00:00\"}"),
            Some(r#"{"kogda":"2026-11-13T20:00:36.9576285Z","shag":0,"itog":"ne_nuzhno","predel_do":96,"predel_posle":96,"obychnyy":80}"#),
            &с);
        assert_eq!(в.установлено.as_deref(), Some("2026-11-13T19:48:12.5999681+00:00"), "BOM PowerShell не мешает");
        assert_eq!(в.итог.as_ref().map(|и| (и.0.as_str(), и.1 > 0, и.2)), Some(("ne_nuzhno", true, Some(96))));
        assert_eq!(в.эпизоды, vec![(2_000, Some(5_000), 1, "snyat".to_string(), true)]);
        // 13.11 19:50 (живой журнал): проба зажима писала «33 из 80 Вт» — это не замер, окно не должно выдавать его за правду
        assert_eq!(в.замер.as_ref().map(|з| (з.0, з.4, з.5)), Some((Some(115.0), false, 1_000)), "проба — не замер");
        let настоящий = [
            событие(1_000, "видеокарта_зажата", json!({"данные": {"обычный_вт": 80, "предел_вт": 33, "ступень": "P5"}, "проба": false})),
        ];
        let в = разобрать_видеокарту(None, None, &настоящий);
        assert_eq!(в.замер.as_ref().map(|з| (з.0, з.4)), Some((Some(33.0), true)), "настоящий зажим — зажата");
        let в = разобрать_видеокарту(None, None, &[настоящий[0].clone(), событие(2_000, "видеокарта_отпущена", json!({}))]);
        assert_eq!(в.замер.as_ref().map(|з| з.4), Some(false), "отпустила — уже не зажата");
        assert_eq!(разобрать_видеокарту(None, None, &[]), Видеокарта::default());
    }

    #[test]
    fn загрузки_и_барс() {
        let з = разобрать_загрузки(r#"{"items":[{"id":"a","file":"setup.exe","state":"held","at_ms":5,"steps":["подписи нет",{"text":"Защитник: чисто"}]},
            {"id":"b","file":"кино.mp4","state":"clean","at_ms":9},{"file":"без id"}]}"#,
            Some(r#"{"folders":["Загрузки"],"checks":[{"name":"Защитник Windows"},"подпись издателя"],"queue":2}"#)).expect("загрузки");
        assert_eq!(з.записи.iter().map(|з| з.id.as_str()).collect::<Vec<_>>(), ["b", "a"], "новые сверху; без id — мимо");
        assert_eq!(з.записи[1].шаги, vec!["подписи нет".to_string(), "Защитник: чисто".to_string()]);
        assert_eq!((з.проверки.len(), з.очередь), (2, 2));
        assert!(разобрать_загрузки("[]", None).is_some_and(|з| з.записи.is_empty()));
        assert!(разобрать_загрузки("{}", None).is_none());
        let б = разобрать_барса(r#"{"compute":[24028,40092],"engines":[24028,40092,40392],"head":18492,"mind_idle_s":null,
            "mind_loaded":false,"mode":"СВОБОДЕН","ms":1794607430668,"video":false,"window":20324,"work":[]}"#).expect("БАРС");
        assert_eq!(б.нет_полей, vec!["mind_pid", "word"], "живой файл 13.11 — без двух полей договора");
        assert_eq!(б.все_pid(), vec![18492, 20324, 24028, 40092, 40392]);
        assert_eq!(б.разум_простой_с, None);
        assert!(разобрать_барса(r#"{"ms":1,"mind_pid":5,"word":false}"#).is_some_and(|б| б.нет_полей.is_empty()));
        assert_eq!(окна_разработки(r#"{"dev_windows":["claude.exe"]}"#), vec!["claude.exe".to_string()]);
        assert!(окна_разработки("не json").is_empty());
    }

    #[test]
    fn кто_ест_по_классам() {
        let п = |pid: u32, родитель: u32, имя: &str, ядер: f64, озу: u64| ПроцессОкна { pid, родитель, имя: имя.into(), ядер,
            озу_мб: озу, выделено_мб: озу, диск_мб_с: 0.0, свой_сеанс: true };
        let все = vec![
            п(4, 0, "System", 0.1, 1), ПроцессОкна { свой_сеанс: false, ..п(900, 4, "Fusion360.exe", 3.0, 6000) },
            п(100, 1, "vivaldi.exe", 0.2, 900), п(101, 100, "vivaldi.exe", 0.1, 300), п(102, 50, "vivaldi.exe", 0.0, 100),
            п(200, 1, "claude.exe", 0.1, 400), п(201, 200, "cargo.exe", 0.5, 100), п(202, 201, "rustc.exe", 6.0, 4000),
            п(300, 1, "bars.exe", 0.2, 250), п(301, 300, "llama-server.exe", 2.9, 17000),
            п(400, 1, "SLDWORKS.exe", 1.2, 3000), п(500, 1, "Telegram.exe", 0.0, 200), п(600, 1, "mekhanik.exe", 0.01, 18),
            п(700, 1, "dwm.exe", 0.3, 200), п(800, 1, "chrome.exe", 1.6, 2400), п(801, 1, "vlc.exe", 1.1, 300),
        ];
        let барс = [300u32];
        let окна = ["claude.exe".to_string()];
        let о = Окружение { передний: Some(100), барс: &барс, окна_разработки: &окна, свой_pid: 600 };
        let к = классы(&все, &о);
        assert_eq!(к[&102], КлассПроцесса::ПередТобой, "тёзка окна впереди — тоже он");
        assert_eq!(к[&202], КлассПроцесса::ТвояРабота, "сборка из окна разработки");
        assert_eq!(к[&200], КлассПроцесса::ТвояРабота, "окно разработки — его работа");
        assert_eq!(к[&301], КлассПроцесса::Барс, "движок БАРСа — по отметке и потомкам");
        assert_eq!(к[&400], КлассПроцесса::ТвояРабота, "САПР без списка имён — по поведению (ядер ≥ 1)");
        assert_eq!(к[&500], КлассПроцесса::Фон);
        // его слово 13.11: фильмы и сериалы — не тяжёлая работа, даже если браузер или проигрыватель едят ядро и гигабайты
        assert_eq!((к[&800], к[&801]), (КлассПроцесса::Фон, КлассПроцесса::Фон), "видео — не «твоя работа»");
        assert_eq!((к[&4], к[&900], к[&700]), (КлассПроцесса::Система, КлассПроцесса::Система, КлассПроцесса::Система),
            "ядро, чужой сеанс и dwm — Windows");
        assert_eq!(к[&600], КлассПроцесса::Механик);
        let держит = Держит { замершие: vec![202], ..Default::default() };
        let мозг = [ПроцессМозга { pid: 400, класс: Some(КлассПроцесса::ТвояРабота), род: Some("САПР".into()),
            видеопамять_мб: Some(2048), ..Default::default() }];
        let г = разложить(&все, &о, &мозг, &держит);
        assert_eq!(г.iter().map(|г| г.класс.unwrap()).collect::<Vec<_>>(), КлассПроцесса::ВСЕ.to_vec(), "его порядок классов");
        let работа = &г[1];
        assert_eq!(работа.строки[0].имя, "rustc.exe", "кто больше ест — выше");
        assert_eq!(работа.строки[0].метка.as_deref(), Some("придержан (пауза, не сбой)"));
        let сапр = работа.строки.iter().find(|с| с.имя == "SLDWORKS.exe").unwrap();
        assert_eq!((сапр.род.as_deref(), сапр.видеопамять_мб), (Some("САПР"), Some(2048)), "род и видеопамять — от Мозга");
        let перед = &г[0];
        assert_eq!((перед.строки.len(), перед.строки[0].штук, перед.итог.озу_мб), (1, 3, 1300), "тёзки — одной строкой");
        assert_eq!(перед.строки[0].метка.as_deref(), Some("не трогает"));
    }

    #[test]
    fn полночь() {
        let с = 1_794_604_662_942;
        let п = полночь_мс(с);
        assert!(п <= с && с - п < 24 * 3600 * 1000);
        assert_eq!(полночь_мс(п), п);
    }
}
