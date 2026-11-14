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
    fn OpenProcess(доступ: u32, наследовать: i32, pid: u32) -> isize;
    fn TerminateProcess(h: isize, код: u32) -> i32;
}

/// 🟥 ЧИСТАЯ (видно в тесте): окно прежней версии — процесс «Механика» из ДРУГОЙ папки `versii\<версия>\` (не мой exe и
/// не пенёк «запуска» `Programs\Mekhanik\mekhanik.exe`). Сравнение путей — без регистра.
pub fn окно_прежней_версии(путь: &str, мой: &str) -> bool {
    let п = путь.to_lowercase().replace('/', "\\");
    let м = мой.to_lowercase().replace('/', "\\");
    п != м && п.contains("\\versii\\") && п.ends_with("\\mekhanik.exe")
}

/// 🟥 14.11 (живьём: после самообновления 0.6 → 0.9 значок остался от 0.6 — окно никто не сменил): при старте Мозг
/// закрывает окно ПРЕЖНЕЙ версии «Механика» (своё же окно — в нём нет ничего, что можно потерять: всё состояние у Мозга
/// и в файлах), чтобы `поднять_окно` поднял окно своей версии. Чужие процессы не трогает — только «Механик» из
/// `versii\` другой версии.
pub fn сменить_старое_окно() {
    let Some(мой) = std::env::current_exe().ok().map(|п| п.to_string_lossy().into_owned()) else { return };
    let я = std::process::id();
    for (pid, _, имя) in crate::okna::процессы() {
        if pid == я || !имя.eq_ignore_ascii_case("mekhanik.exe") { continue; }
        let Some(путь) = crate::okna::путь_процесса(pid) else { continue };
        if !окно_прежней_версии(&путь, &мой) { continue; }
        // PROCESS_TERMINATE = 0x0001
        let h = unsafe { OpenProcess(0x0001, 0, pid) };
        if h == 0 { continue; }
        let ок = unsafe { TerminateProcess(h, 0) } != 0;
        unsafe { CloseHandle(h) };
        crate::svyaz::журнал("механик_окно_прежней_версии_закрыто", &json!({ "pid": pid, "путь": путь, "закрыто": ок }));
    }
}

/// 🟥 14.11: окно пропало (закрыли, упало) — Мозг поднимает его снова раз в 30 с (если владелец не на паузе).
/// Раньше окно поднималось только при старте Мозга.
pub fn сторожить_окно() {
    loop {
        std::thread::sleep(Duration::from_secs(30));
        if crate::svyaz::просят_выйти() { return; }
        поднять_окно();
    }
}

/// Имя мьютекса окна (значка у часов): окно его держит, Мозг по нему видит, живо ли окно.
pub const ОКНО_МЬЮТЕКС: &str = "Local\\mekhanik-okno";
/// Имя мьютекса Мозга (один на сеанс) — такое же, как в `okna::единственный`. Зовёт окно (`vid`) при сведении.
#[allow(dead_code)]
pub const МОЗГ_МЬЮТЕКС: &str = "Local\\mekhanik-odin";
/// 🟥 Сведение (13.11) включило: `vid::работать_окно` пришёл — Мозг поднимает окно (значок у часов).
const ОКНО_ГОТОВО: bool = true;

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

    /// 🟥 14.11: окно прежней версии — только «Механик» из другой папки versii; пенёк «запуска», мой exe и чужие — нет.
    #[test]
    fn окно_прежней_версии_только_из_versii() {
        let мой = r"C:\Users\a\AppData\Local\Programs\Mekhanik\versii\0.9.0\mekhanik.exe";
        assert!(окно_прежней_версии(r"C:\Users\a\AppData\Local\Programs\Mekhanik\versii\0.6.0\mekhanik.exe", мой));
        assert!(окно_прежней_версии(r"c:\users\a\appdata\local\programs\mekhanik\VERSII\0.6.0\MEKHANIK.EXE", мой), "регистр");
        assert!(!окно_прежней_версии(мой, мой), "мой exe — не трогать");
        assert!(!окно_прежней_версии(r"C:\Users\a\AppData\Local\Programs\Mekhanik\mekhanik.exe", мой), "пенёк «запуска»");
        assert!(!окно_прежней_версии(r"C:\Tools\versii\0.1\other.exe", мой), "чужая программа");
    }

    #[test]
    fn подъём_окна() {
        assert!(нужно_поднять(false, false), "окна нет, паузы нет — поднять");
        assert!(!нужно_поднять(true, false), "окно есть — не поднимать");
        assert!(!нужно_поднять(false, true), "владелец на паузе — не поднимать");
        assert!(!нужно_поднять(true, true));
    }
}
