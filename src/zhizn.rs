//! 🟥 13.11 (волна 2а, пакет «жизнь»): обычная, прозрачная жизнь программы на стандартных средствах Windows.
//! Его слово 13.11: «чтобы Механик работал сам максимально просто и удобно». Отчёт `samostoyatelnost.md` §5.4,
//! `izobretatel_avtonomiya.md` И1.
//! Здесь: свой приоритет (обычный, без экономного режима), смотритель потоков с перезапуском, ловушка паники с записью
//! на диск сразу, взаимный подъём с окном (значок у часов). Обновление и «запуск» — в `obnovlenie.rs`.

use serde_json::{json, Value};
use std::ffi::c_void;
use std::io::Write;
use std::time::Duration;

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn SetPriorityClass(h: isize, класс: u32) -> i32;
    fn SetProcessInformation(h: isize, класс: u32, данные: *const c_void, размер: u32) -> i32;
    fn CreateMutexW(атрибуты: *const c_void, взять: i32, имя: *const u16) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn GetLastError() -> u32;
}

/// Имя мьютекса окна (значка у часов): окно его держит, Мозг по нему видит, живо ли окно.
pub const ОКНО_МЬЮТЕКС: &str = "Local\\mekhanik-okno";
/// Имя мьютекса Мозга (один на сеанс) — такое же, как в `okna::единственный`. Зовёт окно (`vid`) при сведении.
#[allow(dead_code)]
pub const МОЗГ_МЬЮТЕКС: &str = "Local\\mekhanik-odin";
/// 🟥 Сведение включит, когда появится `vid::работать_окно`: до тех пор Мозг окно не поднимает (нечего поднимать).
const ОКНО_ГОТОВО: bool = false;

#[repr(C)]
struct ДроссельПитания {
    версия: u32,
    маска_управления: u32,
    маска_состояния: u32,
}

/// 🟥 При старте Мозг ставит себе обычную жизнь: обычный приоритет процесса (не «ниже обычного», как давала задача с
/// приоритетом 7 — `samostoyatelnost.md` §2.5) и выключает себе экономный режim (EcoQoS), чтобы Windows не увела
/// сторожа на медленные ядра. Память и диск при обычном классе процесса — тоже обычные.
pub fn обычная_жизнь() {
    const NORMAL_PRIORITY_CLASS: u32 = 0x20;
    // ProcessPowerThrottling = 4; PROCESS_POWER_THROTTLING_EXECUTION_SPEED = 1; версия = 1
    const PROCESS_POWER_THROTTLING: u32 = 4;
    let h = unsafe { GetCurrentProcess() };
    let приоритет_ок = unsafe { SetPriorityClass(h, NORMAL_PRIORITY_CLASS) } != 0;
    let дроссель = ДроссельПитания { версия: 1, маска_управления: 1, маска_состояния: 0 };
    let эко_выкл = unsafe {
        SetProcessInformation(
            h,
            PROCESS_POWER_THROTTLING,
            &дроссель as *const ДроссельПитания as *const c_void,
            std::mem::size_of::<ДроссельПитания>() as u32,
        )
    } != 0;
    crate::svyaz::журнал("механик_обычная_жизнь", &json!({ "приоритет_обычный": приоритет_ок, "эко_выключен": эко_выкл }));
}

/// 🟥 Ловушка паники: любая паника пишет строку на диск СРАЗУ (в `padeniya.log`, без serde и без БАРСа — на случай,
/// если паникует как раз запись в журнал), и затем, по возможности, в свой журнал.
pub fn ловушка_паники() {
    std::panic::set_hook(Box::new(|инфо| {
        let место = инфо.location().map(|л| format!("{}:{}", л.file(), л.line())).unwrap_or_default();
        let поток = std::thread::current().name().unwrap_or("?").to_string();
        let текст = format!(
            "{}  поток={поток} {место}  {инфо}",
            chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false)
        );
        if let Ok(mut ф) = std::fs::OpenOptions::new().create(true).append(true).open(crate::svyaz::папка().join("padeniya.log")) {
            let _ = writeln!(ф, "{текст}");
        }
        crate::svyaz::журнал("механик_паника", &json!({ "поток": поток, "место": место }));
    }));
}

/// 🟥 Смотритель потоков: запустить поток под `catch_unwind`; паника или неожиданный выход — строка в журнал и
/// перезапуск через `пауза`. Останавливается, когда просят выйти (`stop.flag`). Так падение одного потока (замер,
/// видеокарта, обновление) не роняет весь процесс.
pub fn поток<Т>(имя: &'static str, пауза: Duration, тело: Т) -> std::thread::JoinHandle<()>
where
    Т: Fn() + Send + 'static,
{
    std::thread::Builder::new()
        .name(имя.to_string())
        .spawn(move || {
            let mut раз = 0u32;
            loop {
                let итог = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| тело()));
                if crate::svyaz::просят_выйти() {
                    return;
                }
                раз += 1;
                let как = if итог.is_err() { "паника" } else { "вернулся" };
                crate::svyaz::журнал("механик_поток_перезапуск", &json!({ "поток": имя, "как": как, "раз": раз }));
                std::thread::sleep(пауза);
            }
        })
        .expect("не создать поток")
}

fn широкая(с: &str) -> Vec<u16> {
    с.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Именованный мьютекс уже кем-то держится? (не держим сами — только смотрим). ERROR_ALREADY_EXISTS = 183.
pub fn занято(имя: &str) -> bool {
    let ш = широкая(имя);
    let h = unsafe { CreateMutexW(std::ptr::null(), 0, ш.as_ptr()) };
    let есть = h != 0 && unsafe { GetLastError() } == 183;
    if h != 0 {
        unsafe { CloseHandle(h) };
    }
    есть
}

/// Чистая (видно в тесте): владелец поставил паузу (`pauza.json` `{"do_ms": …}` — до какого мига). Срок вышел — нет.
pub fn пауза_активна(текст: &str, сейчас: u64) -> bool {
    serde_json::from_str::<Value>(текст)
        .ok()
        .and_then(|v| v.get("do_ms").and_then(Value::as_u64))
        .is_some_and(|до| до > сейчас)
}

/// Владелец поставил паузу (меню значка «Пауза на час»)? Читает `pauza.json` в папке данных.
pub fn на_паузе() -> bool {
    std::fs::read_to_string(crate::svyaz::папка().join("pauza.json"))
        .map(|т| пауза_активна(&т, crate::svyaz::сейчас_мс()))
        .unwrap_or(false)
}

/// Чистая (видно в тесте): поднимать ли окно — его нет и владелец не поставил паузу.
pub fn нужно_поднять(окно_работает: bool, на_паузе: bool) -> bool {
    !окно_работает && !на_паузе
}

fn запустить_роль(аргументы: &[&str]) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe).args(аргументы).spawn().is_ok()
}

/// 🟥 Взаимный подъём: Мозг поднимает окно (значок у часов), если его нет и владелец не на паузе.
/// До сведения (`vid::работать_окно` ещё нет) — только отметка: нечего поднимать.
pub fn поднять_окно() {
    if !ОКНО_ГОТОВО {
        crate::svyaz::журнал("механик_окно_ждёт_сведения", &json!({ "почему": "vid::работать_окно появится при сведении" }));
        return;
    }
    if нужно_поднять(занято(ОКНО_МЬЮТЕКС), на_паузе()) && запустить_роль(&["--okno"]) {
        crate::svyaz::журнал("механик_окно_поднято", &json!({}));
    }
}

/// 🟥 Для окна при сведении: поднять Мозг, если его нет (окно зовёт, заметив, что Мозг пропал). Запас к «запуску».
#[allow(dead_code)] // зовёт окно (`vid::работать_окно`) при сведении
pub fn поднять_мозг() -> bool {
    if занято(МОЗГ_МЬЮТЕКС) {
        return false;
    }
    let ок = запустить_роль(&[]);
    crate::svyaz::журнал("механик_мозг_поднят_окном", &json!({ "запущен": ок }));
    ок
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn пауза() {
        assert!(пауза_активна(r#"{"do_ms": 2000}"#, 1000), "срок впереди — пауза");
        assert!(!пауза_активна(r#"{"do_ms": 1000}"#, 2000), "срок вышел — нет");
        assert!(!пауза_активна("не json", 1), "мусор — нет паузы");
        assert!(!пауза_активна(r#"{"иное": 5}"#, 1), "нет поля — нет паузы");
    }

    #[test]
    fn подъём_окна() {
        assert!(нужно_поднять(false, false), "окна нет, паузы нет — поднять");
        assert!(!нужно_поднять(true, false), "окно есть — не поднимать");
        assert!(!нужно_поднять(false, true), "владелец на паузе — не поднимать");
        assert!(!нужно_поднять(true, true));
    }
}
