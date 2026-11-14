//! 🟥 ЧЁРНЫЙ ЯЩИК «Механика» 1.0 (волна 2б, разбор `predvoskhishchenie.md` §7.2, `izobretatel_avtonomiya.md` И11): последние
//! 10 минут жизни ПК — по кадру раз в секунду. В кадре: питание и заряд, доступная и зачисленная память, видеопамять
//! NVIDIA, занятость диска, верх процессов с классами `kto`, его внимание (передний процесс) и сколько советов дало
//! предвидение. Кольцо держится в файле `%LOCALAPPDATA%\mekhanik\yashchik\live.jsonl`, сбрасывается на диск раз в 5 с
//! (замер диск не ждёт). После ВНЕЗАПНОГО выключения ПК — одна строка с причиной из журнала Windows «System»
//! (Kernel-Power 41, 6008, BugCheck 1001); журнал только читаем.
//!
//! Зачем (его слово «понимать, что это»): 7 внезапных выключений за 19 дней прошли мимо «Механика»; ящик их объяснит и
//! даст предвидению записанные дни для честного повтора (`examples/povtor.rs`). Цена — копейки: кадр мал, запись раз в
//! 5 с с низкой очередью диска.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Сколько кадров держим (10 минут по кадру в секунду).
pub const КАДРОВ: usize = 600;
/// Как часто сбрасывать кольцо на диск.
pub const СБРОС_МС: u64 = 5000;

/// Один процесс в верхе кадра.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Верх {
    pub класс: String, // «его работа», «БАРС», «фон»… — `Чей::слово`
    pub имя: String,
    pub мб: u64,
    pub ядер: f64,
    pub видео_нв_мб: u64,
}

/// Кадр чёрного ящика — что было на ПК в этот миг.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Кадр {
    pub когда_мс: u64,
    pub от_сети: bool,
    pub заряд: u8,
    pub доступно_мб: u64,
    pub зачислено_своб_мб: u64,
    pub видеопамять_нв_своб_мб: u64,
    pub диск_занят: f64,
    pub передний_pid: u32,
    pub за_пк: bool,
    pub советов: u8,
    pub верх: Vec<Верх>,
}

impl Кадр {
    fn в_json(&self) -> Value {
        json!({
            "ms": self.когда_мс, "ac": self.от_сети, "bat": self.заряд,
            "ram_mb": self.доступно_мб, "commit_free_mb": self.зачислено_своб_мб, "vram_nv_free_mb": self.видеопамять_нв_своб_мб,
            "disk": self.диск_занят, "fg": self.передний_pid, "at_pc": self.за_пк, "advice": self.советов,
            "top": self.верх.iter().map(|в| json!({
                "cls": в.класс, "name": в.имя, "mb": в.мб, "cores": в.ядер, "vram_nv_mb": в.видео_нв_мб
            })).collect::<Vec<_>>(),
        })
    }
    fn из_json(v: &Value) -> Option<Кадр> {
        Some(Кадр {
            когда_мс: v.get("ms")?.as_u64()?,
            от_сети: v.get("ac").and_then(Value::as_bool).unwrap_or(false),
            заряд: v.get("bat").and_then(Value::as_u64).unwrap_or(0) as u8,
            доступно_мб: v.get("ram_mb").and_then(Value::as_u64).unwrap_or(0),
            зачислено_своб_мб: v.get("commit_free_mb").and_then(Value::as_u64).unwrap_or(0),
            видеопамять_нв_своб_мб: v.get("vram_nv_free_mb").and_then(Value::as_u64).unwrap_or(0),
            диск_занят: v.get("disk").and_then(Value::as_f64).unwrap_or(0.0),
            передний_pid: v.get("fg").and_then(Value::as_u64).unwrap_or(0) as u32,
            за_пк: v.get("at_pc").and_then(Value::as_bool).unwrap_or(false),
            советов: v.get("advice").and_then(Value::as_u64).unwrap_or(0) as u8,
            верх: v.get("top").and_then(Value::as_array).map(|а| а.iter().filter_map(|т| Some(Верх {
                класс: т.get("cls")?.as_str()?.to_string(),
                имя: т.get("name")?.as_str()?.to_string(),
                мб: т.get("mb").and_then(Value::as_u64).unwrap_or(0),
                ядер: т.get("cores").and_then(Value::as_f64).unwrap_or(0.0),
                видео_нв_мб: т.get("vram_nv_mb").and_then(Value::as_u64).unwrap_or(0),
            })).collect()).unwrap_or_default(),
        })
    }
}

/// Причина внезапного выключения по событию журнала Windows System.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Причина { БезПитания41, НекорректноеЗавершение6008, СинийЭкран1001 }

impl Причина {
    pub fn слово(self) -> &'static str {
        match self {
            Причина::БезПитания41 => "ПК выключился без корректного завершения (Kernel-Power 41)",
            Причина::НекорректноеЗавершение6008 => "предыдущее завершение работы было неожиданным (6008)",
            Причина::СинийЭкран1001 => "синий экран (BugCheck 1001)",
        }
    }
    /// Сопоставить номер события журнала System с причиной (только эти три — его список).
    pub fn из_события(id: u32) -> Option<Причина> {
        match id {
            41 => Some(Причина::БезПитания41),
            6008 => Some(Причина::НекорректноеЗавершение6008),
            1001 => Some(Причина::СинийЭкран1001),
            _ => None,
        }
    }
}

/// Выбрать причину выключения из набора событий (id, когда_мс), случившихся ПОСЛЕ последнего кадра ящика. Самый ясный —
/// синий экран, затем 6008, затем 41. Чистая (видно в тесте).
pub fn причина_выключения(события: &[(u32, u64)], последний_кадр_мс: u64) -> Option<(Причина, u64)> {
    let подходящие: Vec<(Причина, u64)> = события.iter()
        .filter(|(_, когда)| *когда >= последний_кадр_мс)
        .filter_map(|&(id, когда)| Причина::из_события(id).map(|п| (п, когда))).collect();
    // приоритет: синий экран → 6008 → 41
    for искомая in [Причина::СинийЭкран1001, Причина::НекорректноеЗавершение6008, Причина::БезПитания41] {
        if let Some(&(п, когда)) = подходящие.iter().find(|(п, _)| *п == искомая) { return Some((п, когда)); }
    }
    None
}

/// Чёрный ящик: кольцо кадров в памяти с периодическим сбросом в файл.
pub struct Ящик {
    кольцо: VecDeque<Кадр>,
    папка: Option<PathBuf>,
    последний_сброс_мс: u64,
    грязно: bool,
}

/// Папка чёрного ящика: `%LOCALAPPDATA%\mekhanik\yashchik\`.
pub fn папка_ящика() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|д| Path::new(&д).join("mekhanik").join("yashchik"))
}

impl Ящик {
    pub fn новый(папка: Option<PathBuf>) -> Ящик {
        Ящик { кольцо: VecDeque::with_capacity(КАДРОВ), папка, последний_сброс_мс: 0, грязно: false }
    }

    pub fn сколько(&self) -> usize { self.кольцо.len() }
    pub fn кадры(&self) -> impl Iterator<Item = &Кадр> { self.кольцо.iter() }
    pub fn последний(&self) -> Option<&Кадр> { self.кольцо.back() }

    /// Добавить кадр; держать последние `КАДРОВ`.
    pub fn кадр(&mut self, к: Кадр) {
        self.кольцо.push_back(к);
        while self.кольцо.len() > КАДРОВ { self.кольцо.pop_front(); }
        self.грязно = true;
    }

    /// Пора ли сбрасывать (раз в 5 с).
    pub fn сброс_пора(&self, сейчас_мс: u64) -> bool {
        self.грязно && сейчас_мс.saturating_sub(self.последний_сброс_мс) >= СБРОС_МС
    }

    fn текст(&self) -> String {
        let mut s = String::with_capacity(self.кольцо.len() * 160);
        for к in &self.кольцо {
            s.push_str(&к.в_json().to_string());
            s.push('\n');
        }
        s
    }

    /// Сбросить кольцо в `live.jsonl` (надёжно: во временный файл и переименование). `Ok(false)` — нечего/некуда.
    pub fn сбросить(&mut self, сейчас_мс: u64) -> std::io::Result<bool> {
        if !self.грязно { return Ok(false); }
        let Some(папка) = self.папка.clone() else { return Ok(false); };
        std::fs::create_dir_all(&папка)?;
        let файл = папка.join("live.jsonl");
        let врем = папка.join("live.jsonl.tmp");
        std::fs::write(&врем, self.текст())?;
        std::fs::rename(&врем, &файл)?;
        self.последний_сброс_мс = сейчас_мс;
        self.грязно = false;
        Ok(true)
    }

    /// Загрузить прошлый `live.jsonl` (для разбора падения при старте). Битые строки пропускаются.
    pub fn загрузить_прошлый(папка: &Path) -> Vec<Кадр> {
        let файл = папка.join("live.jsonl");
        let текст = std::fs::read_to_string(&файл).unwrap_or_default();
        текст.lines().filter_map(|строка| {
            serde_json::from_str::<Value>(строка).ok().and_then(|v| Кадр::из_json(&v))
        }).collect()
    }
}

/// При старте: если прошлый ящик обрывается, а в журнале Windows после него есть 41/6008/1001 — записать одну строку
/// `padenie\ГГГГ-ММ-ДД_ЧЧММСС.json` с причиной и последними кадрами. Возвращает причину, если падение разобрано.
/// `читать_события` — поставщик событий Windows (в живой сборке — `события_выключения`; в тестах — свои).
pub fn разобрать_падение(
    папка: &Path,
    читать_события: impl FnOnce() -> Vec<(u32, u64)>,
) -> std::io::Result<Option<(Причина, u64)>> {
    let кадры = Ящик::загрузить_прошлый(папка);
    let Some(последний) = кадры.last() else { return Ok(None); };
    let последний_мс = последний.когда_мс;
    let события = читать_события();
    let Some((причина, когда)) = причина_выключения(&события, последний_мс) else { return Ok(None); };

    let строка = json!({
        "ms": когда, "cause": причина.слово(),
        "last_frame_ms": последний_мс,
        "before": кадры.iter().rev().take(60).rev().map(Кадр::в_json).collect::<Vec<_>>(),
    });
    let папка_пад = папка.join("padenie");
    std::fs::create_dir_all(&папка_пад)?;
    let имя = format!("padenie_{когда}.json");
    let _ = std::fs::write(папка_пад.join(имя), serde_json::to_string_pretty(&строка).unwrap_or_default());
    Ok(Some((причина, когда)))
}

/// Прочитать из журнала Windows «System» события выключения (41, 6008, 1001) за последние сутки — только чтение, через
/// `wevtutil` (есть в Windows). Возвращает (id, когда_мс). Любая осечка — пустой список (ящик просто не разберёт падение).
#[cfg(windows)]
pub fn события_выключения() -> Vec<(u32, u64)> {
    use std::process::Command;
    // XPath: три источника; последние сутки
    let запрос = "*[System[(EventID=41 or EventID=6008 or EventID=1001) and TimeCreated[timediff(@SystemTime) <= 86400000]]]";
    let вывод = Command::new("wevtutil")
        .args(["qe", "System", "/q:", "/rd:true", "/f:text", "/c:20"])
        .arg(format!("/q:{запрос}"))
        .output();
    let Ok(вывод) = вывод else { return Vec::new(); };
    let текст = String::from_utf8_lossy(&вывод.stdout);
    разобрать_wevtutil(&текст)
}

#[cfg(not(windows))]
pub fn события_выключения() -> Vec<(u32, u64)> { Vec::new() }

/// Разобрать текстовый вывод `wevtutil qe … /f:text`: строки «Event ID: N» и «Date: ГГГГ-ММ-ДДTЧЧ:ММ:СС…». Чистая.
pub fn разобрать_wevtutil(текст: &str) -> Vec<(u32, u64)> {
    let mut в = Vec::new();
    let mut id: Option<u32> = None;
    let mut дата: Option<u64> = None;
    for строка in текст.lines() {
        let с = строка.trim();
        if с.starts_with("Event[") {
            // новая запись — порядок полей (Date/Event ID) в выводе Windows не гарантирован
            id = None; дата = None;
        } else if let Some(хвост) = с.strip_prefix("Event ID:") {
            id = хвост.trim().parse().ok();
        } else if let Some(хвост) = с.strip_prefix("Date:") {
            дата = разобрать_дату(хвост.trim());
        }
        if let (Some(i), Some(мс)) = (id, дата) {
            в.push((i, мс));
            id = None; дата = None;
        }
    }
    в
}

/// Разобрать дату вида `2026-11-08T23:42:52.000` в мс эпохи UTC (приблизительно — для сверки с кадром хватает).
fn разобрать_дату(с: &str) -> Option<u64> {
    use chrono::{DateTime, NaiveDateTime, Utc};
    if let Ok(dt) = с.parse::<DateTime<Utc>>() { return Some(dt.timestamp_millis() as u64); }
    let чистая = с.split('.').next().unwrap_or(с);
    NaiveDateTime::parse_from_str(чистая, "%Y-%m-%dT%H:%M:%S").ok()
        .map(|nd| nd.and_utc().timestamp_millis() as u64)
}

#[cfg(test)]
mod тесты {
    use super::*;

    fn кадр(мс: u64) -> Кадр {
        Кадр { когда_мс: мс, от_сети: true, заряд: 80, доступно_мб: 9000, зачислено_своб_мб: 44000,
            видеопамять_нв_своб_мб: 8000, диск_занят: 10.0, передний_pid: 100, за_пк: true, советов: 0,
            верх: vec![Верх { класс: "БАРС".into(), имя: "llama-server.exe".into(), мб: 4500, ядер: 4.0, видео_нв_мб: 10000 }] }
    }

    #[test]
    fn кольцо_держит_последние_10_минут() {
        let mut я = Ящик::новый(None);
        for с in 0..(КАДРОВ as u64 + 120) { я.кадр(кадр(с * 1000)); }
        assert_eq!(я.сколько(), КАДРОВ, "кольцо не растёт сверх 10 минут");
        assert_eq!(я.последний().unwrap().когда_мс, (КАДРОВ as u64 + 119) * 1000);
    }

    #[test]
    fn сброс_раз_в_5_с() {
        let mut я = Ящик::новый(None);
        я.кадр(кадр(1000));
        assert!(!я.сброс_пора(1000), "только что — рано (последний сброс 0, но 1000<5000)");
        assert!(я.сброс_пора(5000), "через 5 с — пора");
    }

    #[test]
    fn запись_чтение_кадров_файлом() {
        let папка = std::env::temp_dir().join(format!("mekh_yashchik_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&папка);
        let mut я = Ящик::новый(Some(папка.clone()));
        for с in 0..5 { я.кадр(кадр(с * 1000)); }
        assert!(я.сбросить(10_000).expect("сброс"));
        assert!(!я.сбросить(20_000).expect("нечего"), "без новых кадров не пишем");
        let прошлые = Ящик::загрузить_прошлый(&папка);
        assert_eq!(прошлые.len(), 5);
        assert_eq!(прошлые[0], кадр(0));
        assert_eq!(прошлые[4].верх[0].имя, "llama-server.exe");
        let _ = std::fs::remove_dir_all(&папка);
    }

    #[test]
    fn причина_выбирается_по_ясности_и_после_кадра() {
        // события: 41 (менее ясно) и 1001 (синий экран, яснее) — берём 1001
        let события = [(41u32, 100_000u64), (1001u32, 101_000u64)];
        let (п, когда) = причина_выключения(&события, 50_000).unwrap();
        assert_eq!(п, Причина::СинийЭкран1001);
        assert_eq!(когда, 101_000);
        // событие ДО последнего кадра не считается падением этого сеанса
        assert!(причина_выключения(&[(41, 10_000)], 50_000).is_none());
    }

    #[test]
    fn разбор_падения_пишет_строку() {
        let папка = std::env::temp_dir().join(format!("mekh_yashchik_pad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&папка);
        let mut я = Ящик::новый(Some(папка.clone()));
        for с in 0..3 { я.кадр(кадр(с * 1000)); }
        я.сбросить(10_000).unwrap();
        // после последнего кадра (2000) — синий экран на 5000
        let причина = разобрать_падение(&папка, || vec![(1001, 5000)]).unwrap();
        assert_eq!(причина.map(|(п, _)| п), Some(Причина::СинийЭкран1001));
        assert!(папка.join("padenie").exists());
        // чистого выхода (нет подходящих событий) — падения нет
        assert!(разобрать_падение(&папка, || vec![]).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&папка);
    }

    #[test]
    fn разбор_wevtutil_текста() {
        let текст = "\
Event[0]:
  Log Name: System
  Source: Microsoft-Windows-Kernel-Power
  Date: 2026-11-08T23:42:52.000
  Event ID: 41
  Level: Critical
Event[1]:
  Date: 2026-11-08T23:43:10.500
  Event ID: 6008
";
        let с = разобрать_wevtutil(текст);
        assert_eq!(с.len(), 2);
        assert_eq!(с[0].0, 41);
        assert_eq!(с[1].0, 6008);
        assert!(с[1].1 > с[0].1);
    }
}
