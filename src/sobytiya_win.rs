//! 🟥 13.11 (волна 2а, ключ `vnimanie`): скрытое окно сообщений и подписки Windows для шины событий (`sobytiya.rs`).
//! Один поток создаёт окно-только-для-сообщений (HWND_MESSAGE) и подписывается:
//! - `SetWinEventHook` (WINEVENT_OUTOFCONTEXT) — окно впереди, свёртывание;
//! - `WTSRegisterSessionNotification` — замок, вход, выход;
//! - `RegisterPowerSettingNotification` — сеть↔батарея, экран погас/горит, он есть/нет;
//! - сообщения окна `WM_POWERBROADCAST` (сон и пробуждение) и `WM_QUERYENDSESSION`/`WM_ENDSESSION` (выключение).
//! Потом крутит цикл `GetMessage` — через него Windows и доставляет хуки OUTOFCONTEXT, и шлёт сообщения окну. Обработчики
//! кладут разобранные события в общий канал (`sobytiya::послать`). В обработчиках ничего тяжёлого — только разбор и отсылка.
//!
//! Заставку своего события не имеет — её «Механик» смотрит дёшево опросом (`vnimanie::заставка_идёт`).

// Пока сведение не подключило шину к Мозгу, вызовы отсюда «не используются» — это не мёртвый код, а ожидание подключения.
#![allow(dead_code)]

use crate::sobytiya::{self, Событие};
use std::ffi::c_void;
use std::sync::mpsc::Receiver;

fn широкая(с: &str) -> Vec<u16> {
    с.encode_utf16().chain(std::iter::once(0)).collect()
}

#[repr(C)]
#[derive(PartialEq, Eq)]
struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

const GUID_ACDC_POWER_SOURCE: GUID = GUID {
    data1: 0x5d3e_9a59, data2: 0xe9d5, data3: 0x4b00, data4: [0xa6, 0xbd, 0xff, 0x34, 0xff, 0x51, 0x65, 0x48],
};
const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID {
    data1: 0x6fe6_9556, data2: 0x704a, data3: 0x47a0, data4: [0x8f, 0x24, 0xc2, 0x8d, 0x93, 0x6f, 0xda, 0x47],
};
const GUID_SESSION_USER_PRESENCE: GUID = GUID {
    data1: 0x3c0f_4548, data2: 0xc03f, data3: 0x4c4d, data4: [0xb9, 0xf2, 0x23, 0x7e, 0xde, 0x68, 0x63, 0x76],
};

#[repr(C)]
struct WndClass {
    style: u32,
    процедура: ОконПроц,
    cls_extra: i32,
    wnd_extra: i32,
    inst: isize,
    icon: isize,
    cursor: isize,
    фон: isize,
    меню: *const u16,
    имя: *const u16,
}

#[repr(C)]
struct Сообщение {
    окно: isize,
    номер: u32,
    wparam: usize,
    lparam: isize,
    время: u32,
    x: i32,
    y: i32,
    _частное: u32,
}

type ОконПроц = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
type ХукПроц = unsafe extern "system" fn(isize, u32, isize, i32, i32, u32, u32);

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(имя: *const u16) -> isize;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(класс: *const WndClass) -> u16;
    fn CreateWindowExW(ex: u32, класс: *const u16, имя: *const u16, стиль: u32, x: i32, y: i32, ш: i32, в: i32,
                       родитель: isize, меню: isize, inst: isize, параметр: *const c_void) -> isize;
    fn DefWindowProcW(окно: isize, номер: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(сообщение: *mut Сообщение, окно: isize, мин: u32, макс: u32) -> i32;
    fn TranslateMessage(сообщение: *const Сообщение) -> i32;
    fn DispatchMessageW(сообщение: *const Сообщение) -> isize;
    fn SetWinEventHook(мин: u32, макс: u32, модуль: isize, проц: ХукПроц, pid: u32, поток: u32, флаги: u32) -> isize;
    fn GetWindowThreadProcessId(окно: isize, pid: *mut u32) -> u32;
    fn RegisterPowerSettingNotification(получатель: isize, guid: *const GUID, флаги: u32) -> isize;
}

#[link(name = "wtsapi32")]
extern "system" {
    fn WTSRegisterSessionNotification(окно: isize, флаги: u32) -> i32;
}

/// Процесс окна по его ручке.
fn pid_окна(окно: isize) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(окно, &mut pid) };
    pid
}

/// Обратный вызов хука окон (OUTOFCONTEXT — зовётся в нашем цикле сообщений). Нас интересует само окно (OBJID_WINDOW=0,
/// CHILDID_SELF=0), не кнопки и меню внутри.
unsafe extern "system" fn хук_окон(_хук: isize, событие: u32, окно: isize, объект: i32, ребёнок: i32, _поток: u32, _время: u32) {
    if объект != 0 || ребёнок != 0 {
        return;
    }
    let pid = if событие == 0x0003 { pid_окна(окно) } else { 0 };
    if let Some(с) = sobytiya::окно_из(событие, pid) {
        sobytiya::послать(с);
    }
}

/// Разобрать смену настройки питания (lParam → POWERBROADCAST_SETTING: GUID, затем на смещении 20 — значение u32).
unsafe fn разобрать_питание(lparam: isize) {
    if lparam == 0 {
        return;
    }
    let p = lparam as *const u8;
    let guid = &*(p as *const GUID);
    let значение = *(p.add(20) as *const u32);
    let с = if *guid == GUID_ACDC_POWER_SOURCE {
        Some(Событие::Питание(sobytiya::питание_из(значение)))
    } else if *guid == GUID_CONSOLE_DISPLAY_STATE {
        Some(Событие::Экран(sobytiya::экран_из(значение)))
    } else if *guid == GUID_SESSION_USER_PRESENCE {
        Some(Событие::Присутствие(sobytiya::присутствие_из(значение)))
    } else {
        None
    };
    if let Some(с) = с {
        sobytiya::послать(с);
    }
}

unsafe extern "system" fn оконная_процедура(окно: isize, номер: u32, wparam: usize, lparam: isize) -> isize {
    match номер {
        // WM_WTSSESSION_CHANGE
        0x02B1 => {
            if let Some(с) = sobytiya::сеанс_из(wparam) {
                sobytiya::послать(с);
            }
            0
        }
        // WM_POWERBROADCAST
        0x0218 => {
            // PBT_POWERSETTINGCHANGE
            if wparam == 0x8013 {
                разобрать_питание(lparam);
            } else if let Some(с) = sobytiya::сон_из(wparam) {
                sobytiya::послать(с);
            }
            1 // TRUE
        }
        // WM_QUERYENDSESSION — выключение/перезагрузка: сказать и разрешить (не держим систему)
        0x0011 => {
            sobytiya::послать(Событие::КонецСеанса);
            1
        }
        // WM_ENDSESSION
        0x0016 => 0,
        _ => DefWindowProcW(окно, номер, wparam, lparam),
    }
}

const HWND_MESSAGE: isize = -3;

/// Создать окно и подписаться. `None` — не вышло создать окно (поток тогда не нужен).
unsafe fn создать_окно() -> Option<isize> {
    let inst = GetModuleHandleW(std::ptr::null());
    let имя_класса = широкая("MekhanikSobytiya");
    let класс = WndClass {
        style: 0,
        процедура: оконная_процедура,
        cls_extra: 0,
        wnd_extra: 0,
        inst,
        icon: 0,
        cursor: 0,
        фон: 0,
        меню: std::ptr::null(),
        имя: имя_класса.as_ptr(),
    };
    // класс мог остаться с прошлого запуска потока — RegisterClassW вернёт 0, окно всё равно создастся
    RegisterClassW(&класс);
    let окно = CreateWindowExW(0, имя_класса.as_ptr(), имя_класса.as_ptr(), 0, 0, 0, 0, 0, HWND_MESSAGE, 0, inst,
        std::ptr::null());
    (окно != 0).then_some(окно)
}

/// Тело потока: окно, подписки, цикл сообщений (крутится до конца процесса).
fn поток() {
    unsafe {
        let Some(окно) = создать_окно() else { return };
        // замок, вход, выход — только наш сеанс (NOTIFY_FOR_THIS_SESSION = 0)
        WTSRegisterSessionNotification(окно, 0);
        // питание, экран, присутствие (DEVICE_NOTIFY_WINDOW_HANDLE = 0)
        for guid in [&GUID_ACDC_POWER_SOURCE, &GUID_CONSOLE_DISPLAY_STATE, &GUID_SESSION_USER_PRESENCE] {
            RegisterPowerSettingNotification(окно, guid, 0);
        }
        // окно впереди (0x0003); свёртывание (0x0016) и развёртывание (0x0017) — одним хуком на диапазон
        // WINEVENT_OUTOFCONTEXT (0) | WINEVENT_SKIPOWNPROCESS (2)
        SetWinEventHook(0x0003, 0x0003, 0, хук_окон, 0, 0, 0x0002);
        SetWinEventHook(0x0016, 0x0017, 0, хук_окон, 0, 0, 0x0002);
        let mut с: Сообщение = std::mem::zeroed();
        // GetMessageW: 0 — WM_QUIT, -1 — ошибка; иначе крутим
        while GetMessageW(&mut с, 0, 0, 0) > 0 {
            TranslateMessage(&с);
            DispatchMessageW(&с);
        }
    }
}

/// Запустить шину: поднять поток событий, вернуть приёмник. Мозг читает из него и обновляет `vnimanie::Состояние`.
pub fn запустить() -> Receiver<Событие> {
    let (tx, rx) = std::sync::mpsc::channel();
    sobytiya::задать_отправитель(tx);
    let _ = std::thread::Builder::new().name("sobytiya".into()).spawn(поток);
    rx
}
