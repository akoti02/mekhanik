//! 🟥 13.11 ОКНО «МЕХАНИКА» (vid.md §7): Win32 + GDI своими объявлениями, без новых крейтов и без сети. Рисунок — в
//! память, потом одним `BitBlt` (без мерцания; `WM_ERASEBKGND` не стирает); масштаб — по монитору (версия 2,
//! `WM_DPICHANGED` пересоздаёт шрифты); тема — как в Windows (у него тёмная), заголовок окна — тёмный через DWM; без
//! анимаций. Открывается только его щелчком, само не выскакивает. Крестик и Esc прячут; спрятано дольше минуты — окно
//! уничтожается (GDI освобождается), работает только значок. Тот же рисунок — и вне экрана, в PNG (проверка вида).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::sostoyanie::{self, Вид as ВидУведомления, Картина, Уведомление};
use super::stranicy::{self, Действие, Мерка, Обстановка, Ровно, Страница, Тон, Фигура, Шрифт};
use super::win32::*;
use super::znachok::{self, Лоток, Показ, ПунктМеню, Выдержка};

// ───────────────────────────── палитра и шрифты ─────────────────────────────

/// Палитра «Механика» (своя, не Кирина; vid.md §7.1): тёмная — по умолчанию, светлая — если Windows светлая.
pub fn цвет_тона(т: Тон, тёмная: bool) -> u32 {
    let rgb = match (т, тёмная) {
        (Тон::Фон, true) => 0x16181D, (Тон::Фон, false) => 0xF6F7F9,
        (Тон::Карточка, true) => 0x1F232A, (Тон::Карточка, false) => 0xFFFFFF,
        (Тон::Рамка, true) => 0x2B313A, (Тон::Рамка, false) => 0xE1E4E8,
        (Тон::Текст, true) => 0xE8EAED, (Тон::Текст, false) => 0x1F2328,
        (Тон::Приглушённый, true) => 0x9AA3AE, (Тон::Приглушённый, false) => 0x59636E,
        (Тон::Слабый, true) => 0x6B7480, (Тон::Слабый, false) => 0x8C959F,
        (Тон::Норма, true) => 0x3DD68C, (Тон::Норма, false) => 0x1A7F4B,
        (Тон::Работаю, true) => 0x4EA1FF, (Тон::Работаю, false) => 0x0969DA,
        (Тон::Внимание, true) => 0xF5C04E, (Тон::Внимание, false) => 0x9A6700,
        (Тон::Тревога, true) => 0xFF6B5E, (Тон::Тревога, false) => 0xCF222E,
        (Тон::Вернул, true) => 0xB392F0, (Тон::Вернул, false) => 0x8250DF,
        (Тон::Выбор, true) => 0x262C35, (Тон::Выбор, false) => 0xE6EAF0,
        (Тон::Наведение, true) => 0x20252D, (Тон::Наведение, false) => 0xEEF1F4,
    };
    цвет(rgb)
}

const ВИДЫ: [Шрифт; 6] = [Шрифт::Заголовок, Шрифт::Крупный, Шрифт::Обычный, Шрифт::Жирный, Шрифт::Мелкий, Шрифт::Знаки];
fn номер_шрифта(ш: Шрифт) -> usize { ВИДЫ.iter().position(|в| *в == ш).unwrap_or(2) }

/// Шрифты окна под масштаб: Segoe UI Variable (Windows 11), запасной — Segoe UI; знаки — Segoe Fluent Icons / MDL2.
pub struct Шрифты { ручки: [Ручка; 6], pub знаки_есть: bool }

fn создать_шрифт(имя: &str, px: i32, вес: i32) -> Ручка {
    let и = широкая(имя);
    // DEFAULT_CHARSET = 1, CLEARTYPE_QUALITY = 5
    unsafe { CreateFontW(-px.max(6), 0, 0, 0, вес, 0, 0, 0, 1, 0, 0, 5, 0, и.as_ptr()) }
}

/// Есть ли шрифт: Windows подставит другой молча — сверяем имя у выбранного.
fn шрифт_есть(имя: &str) -> bool {
    unsafe {
        let dc = CreateCompatibleDC(0);
        let ш = создать_шрифт(имя, 12, 400);
        let было = SelectObject(dc, ш);
        let mut буф = [0u16; 64];
        let н = GetTextFaceW(dc, 64, буф.as_mut_ptr());
        SelectObject(dc, было);
        DeleteObject(ш);
        DeleteDC(dc);
        н > 0 && String::from_utf16_lossy(&буф[..(н as usize).min(64)]).trim_end_matches('\0').eq_ignore_ascii_case(имя)
    }
}

fn первый_есть(имена: &[&'static str]) -> Option<&'static str> { имена.iter().copied().find(|и| шрифт_есть(и)) }

impl Шрифты {
    pub fn новые(м: f32) -> Шрифты {
        let px = |d: f32| (d * м).round() as i32;
        let текст = первый_есть(&["Segoe UI Variable Text", "Segoe UI"]).unwrap_or("Segoe UI");
        let крупный = первый_есть(&["Segoe UI Variable Display", "Segoe UI"]).unwrap_or("Segoe UI");
        let знаки = первый_есть(&["Segoe Fluent Icons", "Segoe MDL2 Assets"]);
        Шрифты { ручки: [создать_шрифт(крупный, px(20.0), 600), создать_шрифт(крупный, px(17.0), 600), создать_шрифт(текст, px(14.0), 400),
            создать_шрифт(текст, px(14.0), 600), создать_шрифт(текст, px(12.0), 400), создать_шрифт(знаки.unwrap_or(текст), px(15.0), 400)],
            знаки_есть: знаки.is_some() }
    }
    pub fn ручка(&self, ш: Шрифт) -> Ручка { self.ручки[номер_шрифта(ш)] }
}

impl Drop for Шрифты { fn drop(&mut self) { for р in self.ручки { if р != 0 { unsafe { DeleteObject(р) }; } } } }

/// Мерка текста — GDI на том же контексте, где рисуем.
pub struct МеркаGdi<'а> { dc: Ручка, ш: &'а Шрифты, высоты: [i32; 6] }

impl<'а> МеркаGdi<'а> {
    pub fn новая(dc: Ручка, ш: &'а Шрифты) -> МеркаGdi<'а> {
        let mut м = МеркаGdi { dc, ш, высоты: [0; 6] };
        for (н, в) in ВИДЫ.iter().enumerate() { м.высоты[н] = м.размер("Ёу", *в).в.max(1); }
        м
    }
    fn размер(&self, т: &str, ш: Шрифт) -> Размер {
        let ш_ = широкая(т);
        let mut р = Размер::default();
        unsafe {
            SelectObject(self.dc, self.ш.ручка(ш));
            GetTextExtentPoint32W(self.dc, ш_.as_ptr(), (ш_.len() - 1) as i32, &mut р);
        }
        р
    }
}

impl Мерка for МеркаGdi<'_> {
    fn ширина(&self, т: &str, ш: Шрифт) -> i32 { self.размер(т, ш).ш }
    fn высота(&self, ш: Шрифт) -> i32 { self.высоты[номер_шрифта(ш)] }
}

// ───────────────────────────── рисование ─────────────────────────────

/// Нарисовать фигуры со сдвигом; `наведена` — плашка кнопки под мышью (подсветить).
pub fn нарисовать_фигуры(dc: Ручка, фигуры: &[Фигура], dx: i32, dy: i32, тёмная: bool, шр: &Шрифты, наведена: Option<Прям>) {
    unsafe {
        SetBkMode(dc, 1);
        let пустое_перо = GetStockObject(8);
        for ф in фигуры {
            match ф {
                Фигура::Плашка { п, тон, рамка, радиус } => {
                    let п = п.сдвиг(dx, dy);
                    let тон = if наведена == Some(п) && *тон == Тон::Карточка { Тон::Наведение } else { *тон };
                    let кисть = CreateSolidBrush(цвет_тона(тон, тёмная));
                    if *радиус == 0 && рамка.is_none() {
                        FillRect(dc, &п, кисть);
                    } else {
                        let перо = рамка.map(|р| CreatePen(0, 1, цвет_тона(р, тёмная))).unwrap_or(0);
                        let было_п = SelectObject(dc, if перо != 0 { перо } else { пустое_перо });
                        let было_к = SelectObject(dc, кисть);
                        RoundRect(dc, п.лев, п.верх, п.прав, п.низ, радиус * 2, радиус * 2);
                        SelectObject(dc, было_п);
                        SelectObject(dc, было_к);
                        if перо != 0 { DeleteObject(перо); }
                    }
                    DeleteObject(кисть);
                }
                Фигура::Текст { п, т, шрифт, тон, ровно } => {
                    let mut п = п.сдвиг(dx, dy);
                    let ш = широкая(т);
                    SelectObject(dc, шр.ручка(*шрифт));
                    SetTextColor(dc, цвет_тона(*тон, тёмная));
                    let выр = match ровно { Ровно::Лево => DT_LEFT, Ровно::Центр => DT_CENTER, Ровно::Право => DT_RIGHT };
                    DrawTextW(dc, ш.as_ptr(), (ш.len() - 1) as i32, &mut п, выр | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
                }
                Фигура::Точка { x, y, р, тон } => {
                    let кисть = CreateSolidBrush(цвет_тона(*тон, тёмная));
                    let было_п = SelectObject(dc, пустое_перо);
                    let было_к = SelectObject(dc, кисть);
                    Ellipse(dc, x + dx - р, y + dy - р, x + dx + р + 1, y + dy + р + 1);
                    SelectObject(dc, было_п);
                    SelectObject(dc, было_к);
                    DeleteObject(кисть);
                }
            }
        }
    }
}

/// Что рисуем в кадре.
pub struct Кадр<'а> { pub картина: &'а Картина, pub страница: Страница, pub прокрутка: i32, pub мышь: Option<(i32, i32)>,
    pub тёмная: bool, pub dpi: u32, pub сейчас: u64, pub заметка: Option<&'а str> }

/// Что вышло: кнопки (в точках окна), высота листа и видимая высота, текст страницы (для «Копировать»).
#[derive(Default)]
pub struct ИтогКадра { pub кнопки: Vec<(Прям, Действие)>, pub высота_листа: i32, pub видимо: i32, pub прокрутка: i32,
    pub текст: Vec<String> }

pub const МЕНЮ_DIP: i32 = 196;

/// Всё окно: шапка, меню страниц слева, страница справа (с прокруткой). Одно и то же — на экране и вне экрана.
pub fn нарисовать_всё(dc: Ручка, ш: i32, в: i32, к: &Кадр, шр: &Шрифты) -> ИтогКадра {
    let м = к.dpi.max(96) as f32 / 96.0;
    let px = |d: i32| (d as f32 * м).round() as i32;
    let мерка = МеркаGdi::новая(dc, шр);
    let фон = unsafe { CreateSolidBrush(цвет_тона(Тон::Фон, к.тёмная)) };
    unsafe { FillRect(dc, &Прям { лев: 0, верх: 0, прав: ш, низ: в }, фон); DeleteObject(фон); }
    let шапка = stranicy::шапка(к.картина, ш, м, &мерка, к.сейчас, к.заметка);
    нарисовать_фигуры(dc, &шапка.фигуры, 0, 0, к.тёмная, шр, None);
    let ш_меню = px(МЕНЮ_DIP);
    let над_меню = к.мышь.and_then(|(x, y)| {
        stranicy::меню_страниц(к.страница, None, шапка.высота, ш_меню, м, &мерка, шр.знаки_есть).кнопки.into_iter()
            .find(|(п, _)| п.внутри(x, y)).and_then(|(_, д)| match д { Действие::Страница(с) => Some(с), _ => None })
    });
    let меню = stranicy::меню_страниц(к.страница, над_меню, шапка.высота, ш_меню, м, &мерка, шр.знаки_есть);
    нарисовать_фигуры(dc, &меню.фигуры, 0, 0, к.тёмная, шр, None);
    let черта = unsafe { CreateSolidBrush(цвет_тона(Тон::Рамка, к.тёмная)) };
    unsafe { FillRect(dc, &Прям { лев: ш_меню, верх: шапка.высота, прав: ш_меню + px(1).max(1), низ: в }, черта); DeleteObject(черта); }
    // страница
    let x0 = ш_меню + px(24);
    let ш_листа = (ш - x0 - px(24)).max(px(200));
    let y0 = шапка.высота + px(16);
    let видимо = (в - y0).max(1);
    let лист = stranicy::страница(к.страница, к.картина, ш_листа, &Обстановка { сейчас: к.сейчас, мерка: &мерка, м });
    let прокрутка = к.прокрутка.clamp(0, (лист.высота - видимо).max(0));
    let дy = y0 - прокрутка;
    let наведена = к.мышь.and_then(|(x, y)| лист.кнопки.iter().map(|(п, _)| п.сдвиг(x0, дy)).find(|п| п.внутри(x, y)
        && y >= шапка.высота));
    unsafe {
        SaveDC(dc);
        IntersectClipRect(dc, ш_меню + px(1).max(1), шапка.высота, ш, в);
    }
    нарисовать_фигуры(dc, &лист.фигуры, x0, дy, к.тёмная, шр, наведена);
    unsafe { RestoreDC(dc, -1); }
    // полоса прокрутки — тонкая, только если лист длиннее
    if лист.высота > видимо {
        let высота = ((видимо as i64 * видимо as i64) / лист.высота.max(1) as i64).max(px(24) as i64) as i32;
        let y = шапка.высота + px(4) + ((видимо - высота - px(8)).max(0) as i64 * прокрутка as i64 / (лист.высота - видимо).max(1) as i64) as i32;
        нарисовать_фигуры(dc, &[Фигура::Плашка { п: Прям::новый(ш - px(8), y, px(4), высота), тон: Тон::Рамка, рамка: None, радиус: px(2) }],
            0, 0, к.тёмная, шр, None);
    }
    let mut кнопки: Vec<(Прям, Действие)> = меню.кнопки;
    кнопки.extend(лист.кнопки.into_iter().map(|(п, д)| (п.сдвиг(x0, дy), д)).filter(|(п, _)| п.низ > шапка.высота && п.верх < в));
    let mut текст = шапка.текст;
    текст.push(String::new());
    текст.extend(лист.текст);
    ИтогКадра { кнопки, высота_листа: лист.высота, видимо, прокрутка, текст }
}

/// Рисунок окна в памяти: DIB 32 бита, сверху вниз. Итог — пиксели 0x00RRGGBB (снимки страниц для проверки вида).
#[cfg(test)]
pub fn нарисовать_в_память(ш: i32, в: i32, к: &Кадр) -> Option<(Vec<u32>, ИтогКадра)> {
    unsafe {
        let dc = CreateCompatibleDC(0);
        if dc == 0 { return None; }
        let з = ЗаголовокРисунка { размер: 40, ширина: ш, высота: -в, плоскостей: 1, бит: 32, сжатие: 0, байт: 0, _x: 0, _y: 0,
            _цветов: 0, _важных: 0 };
        let mut биты: *mut c_void = std::ptr::null_mut();
        let рис = CreateDIBSection(dc, &з, 0, &mut биты, 0, 0);
        if рис == 0 || биты.is_null() { DeleteDC(dc); return None; }
        let было = SelectObject(dc, рис);
        let шр = Шрифты::новые(к.dpi as f32 / 96.0);
        let итог = нарисовать_всё(dc, ш, в, к, &шр);
        GdiFlush();
        let пиксели = std::slice::from_raw_parts(биты as *const u32, (ш * в) as usize).iter().map(|п| п & 0x00FF_FFFF).collect();
        SelectObject(dc, было);
        drop(шр);
        DeleteObject(рис);
        DeleteDC(dc);
        Some((пиксели, итог))
    }
}

// ───────────────────────────── PNG без крейтов (снимки для проверки вида) ─────────────────────────────

#[cfg(test)]
fn crc32(данные: &[&[u8]]) -> u32 {
    let mut таблица = [0u32; 256];
    for (н, т) in таблица.iter_mut().enumerate() {
        let mut с = н as u32;
        for _ in 0..8 { с = if с & 1 != 0 { 0xEDB8_8320 ^ (с >> 1) } else { с >> 1 }; }
        *т = с;
    }
    let mut с = 0xFFFF_FFFFu32;
    for д in данные { for б in *д { с = таблица[((с ^ *б as u32) & 0xFF) as usize] ^ (с >> 8); } }
    с ^ 0xFFFF_FFFF
}

#[cfg(test)]
fn adler32(д: &[u8]) -> u32 {
    let (mut а, mut б) = (1u32, 0u32);
    for кусок in д.chunks(5552) {
        for х in кусок { а += *х as u32; б += а; }
        а %= 65521;
        б %= 65521;
    }
    (б << 16) | а
}

/// Чистая (видно в тесте): PNG (RGB, 8 бит) из пикселей 0x00RRGGBB; сжатия нет — блоки «как есть» (картинки для глаз).
#[cfg(test)]
pub fn в_png(пиксели: &[u32], ш: u32, в: u32) -> Vec<u8> {
    let mut сырые = Vec::with_capacity(((ш * 3 + 1) * в) as usize);
    for y in 0..в as usize {
        сырые.push(0);
        for п in &пиксели[y * ш as usize..(y + 1) * ш as usize] { сырые.extend_from_slice(&[(п >> 16) as u8, (п >> 8) as u8, *п as u8]); }
    }
    let mut z = vec![0x78, 0x01];
    let куски: Vec<&[u8]> = сырые.chunks(65_535).collect();
    if куски.is_empty() { z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]); }
    for (н, к) in куски.iter().enumerate() {
        z.push((н + 1 == куски.len()) as u8);
        let д = к.len() as u16;
        z.extend_from_slice(&д.to_le_bytes());
        z.extend_from_slice(&(!д).to_le_bytes());
        z.extend_from_slice(к);
    }
    z.extend_from_slice(&adler32(&сырые).to_be_bytes());
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut кусок = |вид: &[u8; 4], д: &[u8]| {
        png.extend_from_slice(&(д.len() as u32).to_be_bytes());
        png.extend_from_slice(вид);
        png.extend_from_slice(д);
        png.extend_from_slice(&crc32(&[вид, д]).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&ш.to_be_bytes());
    ihdr.extend_from_slice(&в.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    кусок(b"IHDR", &ihdr);
    кусок(b"IDAT", &z);
    кусок(b"IEND", &[]);
    png
}

// ───────────────────────────── вид: всё, что живёт в потоке окна ─────────────────────────────

struct Буфер { dc: Ручка, рис: Ручка, было: Ручка, ш: i32, в: i32 }
impl Drop for Буфер {
    fn drop(&mut self) { unsafe { SelectObject(self.dc, self.было); DeleteObject(self.рис); DeleteDC(self.dc); } }
}

pub struct Вид {
    pub папка: PathBuf,
    pub общее: Arc<super::Общее>,
    pub скрытое: Ручка,
    pub лоток: Лоток,
    pub окно: Ручка,
    pub страница: Страница,
    прокрутка: i32,
    мышь: Option<(i32, i32)>,
    следим_мышь: bool,
    кнопки: Vec<(Прям, Действие)>,
    высота_листа: i32,
    видимо: i32,
    текст: Vec<String>,
    pub картина: Картина,
    выдержка: Выдержка,
    иконка: Ручка,
    иконка_показ: Option<(Показ, bool)>,
    размер_иконки: i32,
    pub тёмная: bool,
    светлая_панель: bool,
    шрифты: Option<(u32, Шрифты)>,
    буфер: Option<Буфер>,
    подсказка: String,
    спрятано_с: Option<Instant>,
    страница_пузыря: Option<Страница>,
    сказано: HashMap<String, Instant>,
    пузырей: Vec<Instant>,
    pub выход_с: Option<Instant>,
    значки_окна: (Ручка, Ручка),
    pub номер: u64,
    заметка: Option<(String, Instant)>,
}

thread_local! { static ВИД: RefCell<Option<Вид>> = const { RefCell::new(None) }; }

/// Сделать что-то с видом, если он свободен (сообщение пришло, пока вид занят, — обработает Windows по умолчанию).
pub fn с_видом<Т>(ф: impl FnOnce(&mut Вид) -> Т) -> Option<Т> {
    ВИД.with(|в| в.try_borrow_mut().ok().and_then(|mut в| в.as_mut().map(ф)))
}

pub const КЛАСС_ЗНАЧКА: &str = "Mekhanik.Znachok";
const КЛАСС_ОКНА: &str = "Mekhanik.Okno";
/// Сообщения себе: картина готова, очередь уведомлений, открыть.
pub const КАРТИНА_ГОТОВА: u32 = WM_APP + 2;
pub const ОЧЕРЕДЬ_ГОТОВА: u32 = WM_APP + 3;
/// Метки `WM_COPYDATA`: уведомление от другого процесса «Механика»; второй запуск просит открыть окно.
pub const МЕТКА_УВЕДОМЛЕНИЯ: usize = 0x4D45_4B48;
pub const МЕТКА_ОТКРЫТЬ: usize = 0x4D45_4B4F;
const ТАЙМЕР_ВЫХОД: usize = 1;
const ТАЙМЕР_УБОРКА: usize = 2;
/// Спрятано дольше — окно уничтожить (значок остаётся).
const СПРЯТАНО_УНИЧТОЖИТЬ: Duration = Duration::from_secs(60);
/// Одно и то же уведомление — не чаще; всего пузырей — не больше стольких за 10 мин.
const ТО_ЖЕ_НЕ_ЧАЩЕ: Duration = Duration::from_secs(600);
const ПУЗЫРЕЙ_ЗА_10_МИН: usize = 6;
static ВЗЯТЬ_МЫШЬ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn модуль() -> Ручка { unsafe { GetModuleHandleW(std::ptr::null()) } }

/// Класс окна; уже зарегистрирован (второе окно, проба) — Windows откажет, и это годится.
fn зарегистрировать(класс: &str, процедура: Процедура) {
    let к = широкая(класс);
    let с = КлассОкна { размер: std::mem::size_of::<КлассОкна>() as u32, стиль: 3, процедура: Some(процедура), _доп_класса: 0,
        _доп_окна: 0, модуль: модуль(), значок: 0, курсор: unsafe { LoadCursorW(0, IDC_ARROW as *const u16) }, фон: 0,
        _меню: std::ptr::null(), имя: к.as_ptr(), значок_малый: 0 };
    unsafe { RegisterClassExW(&с) };
}

/// Скрытое окно значка: верхнего уровня (иначе `TaskbarCreated` не дойдёт), никогда не показывается.
pub fn создать_скрытое() -> Ручка {
    зарегистрировать(КЛАСС_ЗНАЧКА, процедура_значка);
    let (к, и) = (широкая(КЛАСС_ЗНАЧКА), широкая("Механик — значок"));
    unsafe { CreateWindowExW(0, к.as_ptr(), и.as_ptr(), WS_POPUP, 0, 0, 0, 0, 0, 0, модуль(), std::ptr::null()) }
}

impl Вид {
    pub fn новый(папка: PathBuf, общее: Arc<super::Общее>, скрытое: Ручка) -> Вид {
        let (светлые_прил, светлая_панель) = тема_светлая();
        let размер_иконки = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) }.max(16);
        let сейчас = sostoyanie::сейчас_мс();
        Вид { папка, общее, скрытое, лоток: Лоток::новый(скрытое), окно: 0, страница: Страница::Сейчас, прокрутка: 0, мышь: None,
            следим_мышь: false, кнопки: Vec::new(), высота_листа: 0, видимо: 0, текст: Vec::new(), картина: Картина::default(),
            выдержка: Выдержка::новая(Показ::Спокойно, сейчас), иконка: 0, иконка_показ: None, размер_иконки, тёмная: !светлые_прил,
            светлая_панель, шрифты: None, буфер: None, подсказка: String::new(), спрятано_с: None, страница_пузыря: None,
            сказано: HashMap::new(), пузырей: Vec::new(), выход_с: None, значки_окна: (0, 0), номер: 0, заметка: None }
    }

    /// Поставить значок у часов (при старте и после перезапуска Проводника).
    pub fn поставить_значок(&mut self) -> bool {
        let (п, строка) = znachok::показ(&self.картина, sostoyanie::сейчас_мс());
        self.выдержка = Выдержка::новая(п, sostoyanie::сейчас_мс());
        self.подсказка = znachok::подсказка(&self.картина, &строка);
        let и = znachok::нарисовать(self.размер_иконки, п, self.светлая_панель);
        if self.иконка != 0 { unsafe { DestroyIcon(self.иконка) }; }
        self.иконка = и;
        self.иконка_показ = Some((п, self.светлая_панель));
        let п_ = self.подсказка.clone();
        self.лоток.добавить(и, &п_)
    }

    /// Новая картина от рабочего потока: значок (с выдержкой), подсказка, окно — перерисовать.
    pub fn применить(&mut self, к: Картина) {
        self.картина = к;
        // значок не встал (панель задач ещё не готова при входе) — пробуем снова на каждой картине
        if !self.лоток.добавлен { self.поставить_значок(); }
        self.обновить_значок();
        if self.окно != 0 && unsafe { IsWindowVisible(self.окно) } != 0 { unsafe { InvalidateRect(self.окно, std::ptr::null(), 0) }; }
    }

    pub fn обновить_значок(&mut self) {
        let сейчас = sostoyanie::сейчас_мс();
        let (п, строка) = znachok::показ(&self.картина, сейчас);
        self.выдержка.шаг(п, сейчас);
        let показано = self.выдержка.показано;
        // строка — по тому, что показано (не опережает цвет); показано старое — строка ещё о нём
        let строка = if показано == п { строка } else { self.подсказка.trim_start_matches("Механик · ").lines().next().unwrap_or("").to_string() };
        let подсказка = znachok::подсказка(&self.картина, &строка);
        let нужна_картинка = self.иконка_показ != Some((показано, self.светлая_панель));
        if !нужна_картинка && подсказка == self.подсказка { return; }
        let новая = нужна_картинка.then(|| znachok::нарисовать(self.размер_иконки, показано, self.светлая_панель));
        if self.лоток.обновить(новая, &подсказка) {
            if let Some(н) = новая {
                if self.иконка != 0 { unsafe { DestroyIcon(self.иконка) }; }
                self.иконка = н;
                self.иконка_показ = Some((показано, self.светлая_панель));
            }
        } else if let Some(н) = новая { unsafe { DestroyIcon(н) }; }
        self.подсказка = подсказка;
    }

    /// Пузырь значка с правилами тишины: проба и проверочные копии — молчат; то же — не чаще раза в 10 мин; всего — не
    /// больше 6 за 10 мин; звук — только если он за ПК.
    pub fn сказать(&mut self, у: &Уведомление) -> (bool, &'static str) {
        if super::проверочная_копия() { return (false, "проверочная копия — не уведомляет"); }
        if self.картина.проба { return (false, "идёт проба — не уведомляет"); }
        let ключ = format!("{}\n{}", у.заголовок, у.текст);
        if self.сказано.get(&ключ).is_some_and(|к| к.elapsed() < ТО_ЖЕ_НЕ_ЧАЩЕ) { return (false, "то же уже говорил"); }
        self.пузырей.retain(|к| к.elapsed() < Duration::from_secs(600));
        if self.пузырей.len() >= ПУЗЫРЕЙ_ЗА_10_МИН { return (false, "много за 10 мин — остальное в окне"); }
        let звук = у.вид != ВидУведомления::Сведение && crate::okna::простой_ввода_с() < 120;
        if !self.лоток.сказать(&у.заголовок, &у.текст, у.вид, звук) { return (false, "значок не принял"); }
        self.сказано.insert(ключ, Instant::now());
        self.пузырей.push(Instant::now());
        self.страница_пузыря = у.страница.as_deref().and_then(Страница::из);
        (true, "показано")
    }

    fn масштаб(&self) -> u32 { if self.окно != 0 { unsafe { GetDpiForWindow(self.окно) }.max(96) } else { unsafe { GetDpiForSystem() }.max(96) } }

    fn шрифты(&mut self) -> &Шрифты {
        let dpi = self.масштаб();
        if self.шрифты.as_ref().is_none_or(|(д, _)| *д != dpi) { self.шрифты = Some((dpi, Шрифты::новые(dpi as f32 / 96.0))); }
        &self.шрифты.as_ref().expect("шрифты только что созданы").1
    }

    /// Нарисовать окно в `dc` (через свой буфер — одним `BitBlt`).
    fn нарисовать(&mut self, dc: Ручка) {
        let mut п = Прям::default();
        unsafe { GetClientRect(self.окно, &mut п) };
        let (ш, в) = (п.ширина().max(1), п.высота().max(1));
        if self.буфер.as_ref().is_none_or(|б| б.ш != ш || б.в != в) {
            self.буфер = None;
            unsafe {
                let бdc = CreateCompatibleDC(dc);
                let рис = CreateCompatibleBitmap(dc, ш, в);
                let было = SelectObject(бdc, рис);
                self.буфер = Some(Буфер { dc: бdc, рис, было, ш, в });
            }
        }
        let dpi = self.масштаб();
        let бdc = self.буфер.as_ref().map(|б| б.dc).unwrap_or(0);
        if бdc == 0 { return; }
        let заметка = self.заметка.as_ref().filter(|(_, к)| к.elapsed() < Duration::from_secs(6)).map(|(т, _)| т.clone());
        // шрифты под масштаб — создать заранее, дальше только читаем поля
        self.шрифты();
        let итог = {
            let Some((_, шр)) = self.шрифты.as_ref() else { return };
            let кадр = Кадр { картина: &self.картина, страница: self.страница, прокрутка: self.прокрутка, мышь: self.мышь,
                тёмная: self.тёмная, dpi, сейчас: sostoyanie::сейчас_мс(), заметка: заметка.as_deref() };
            нарисовать_всё(бdc, ш, в, &кадр, шр)
        };
        unsafe { BitBlt(dc, 0, 0, ш, в, бdc, 0, 0, SRCCOPY) };
        self.кнопки = итог.кнопки;
        self.высота_листа = итог.высота_листа;
        self.видимо = итог.видимо;
        self.прокрутка = итог.прокрутка;
        self.текст = итог.текст;
    }

    fn кнопка_под(&self, x: i32, y: i32) -> Option<usize> { self.кнопки.iter().position(|(п, _)| п.внутри(x, y)) }

    fn перерисовать(&self) { if self.окно != 0 { unsafe { InvalidateRect(self.окно, std::ptr::null(), 0) }; } }

    pub fn заметить(&mut self, т: impl Into<String>) { self.заметка = Some((т.into(), Instant::now())); self.перерисовать(); }

    fn к_странице(&mut self, с: Страница) {
        self.страница = с;
        self.прокрутка = 0;
        self.общее.страница.store(Страница::ВСЕ.iter().position(|п| *п == с).unwrap_or(0) as u8, Ordering::Relaxed);
        self.общее.обновить_сейчас.store(true, Ordering::Relaxed);
        self.перерисовать();
    }

    fn прокрутить(&mut self, на: i32) {
        let макс = (self.высота_листа - self.видимо).max(0);
        let было = self.прокрутка;
        self.прокрутка = (self.прокрутка + на).clamp(0, макс);
        if self.прокрутка != было { self.перерисовать(); }
    }

    /// Запомнить место окна (`okno.json`).
    fn запомнить_место(&self) {
        if self.окно == 0 { return; }
        let mut п = Прям::default();
        if unsafe { GetWindowRect(self.окно, &mut п) } == 0 || unsafe { IsIconic(self.окно) } != 0 { return; }
        let v = serde_json::json!({ "v": 1, "x": п.лев, "y": п.верх, "w": п.ширина(), "h": п.высота(), "page": self.страница.ключ() });
        let _ = sostoyanie::записать(&self.папка.join(sostoyanie::МЕСТО_ОКНА), &v.to_string());
    }
}

impl Drop for Вид {
    fn drop(&mut self) {
        self.лоток.убрать();
        unsafe {
            if self.иконка != 0 { DestroyIcon(self.иконка); }
            if self.значки_окна.0 != 0 { DestroyIcon(self.значки_окна.0); }
            if self.значки_окна.1 != 0 { DestroyIcon(self.значки_окна.1); }
        }
    }
}

pub fn поставить_вид(в: Вид) { ВИД.with(|я| *я.borrow_mut() = Some(в)); }
pub fn снять_вид() { let в = ВИД.with(|я| я.borrow_mut().take()); drop(в); }

// ───────────────────────────── главное окно ─────────────────────────────

/// Место окна по умолчанию: 940×720 DIP у правого края рабочей области (его окно разработки — слева), у значка.
fn место_окна(папка: &std::path::Path) -> Прям {
    let (cx, cy) = курсор();
    let работа = рабочая_область(cx, cy).unwrap_or(Прям { лев: 0, верх: 0, прав: 1600, низ: 1000 });
    let dpi = unsafe { GetDpiForSystem() }.max(96) as f32 / 96.0;
    let px = |d: i32| (d as f32 * dpi).round() as i32;
    let сохранено = std::fs::read_to_string(папка.join(sostoyanie::МЕСТО_ОКНА)).ok()
        .and_then(|т| serde_json::from_str::<serde_json::Value>(&т).ok())
        .and_then(|v| Some(Прям::новый(v.get("x")?.as_i64()? as i32, v.get("y")?.as_i64()? as i32, v.get("w")?.as_i64()? as i32,
            v.get("h")?.as_i64()? as i32)));
    if let Some(с) = сохранено {
        // помнит место — если оно на каком-то мониторе и не меньше наименьшего
        if с.ширина() >= px(600) && с.высота() >= px(400) {
            if let Some(р) = рабочая_область(с.лев + с.ширина() / 2, с.верх + px(20)) { if р.внутри(с.лев + с.ширина() / 2, с.верх + px(20)) { return с; } }
        }
    }
    let (ш, в) = (px(940).min(работа.ширина() - px(24)), px(720).min(работа.высота() - px(24)));
    Прям::новый(работа.прав - ш - px(12), работа.низ - в - px(12), ш, в)
}

/// Показать окно (создать, если нет): только его щелчком; фокус — ему, раз он сам открыл.
pub fn показать_окно(страница: Option<Страница>) {
    let есть = с_видом(|в| {
        if let Some(с) = страница { в.к_странице(с); }
        в.спрятано_с = None;
        в.общее.видно.store(true, Ordering::Relaxed);
        в.общее.обновить_сейчас.store(true, Ordering::Relaxed);
        в.окно
    }).unwrap_or(0);
    if есть != 0 {
        unsafe {
            ShowWindow(есть, if IsIconic(есть) != 0 { SW_RESTORE } else { SW_SHOWNORMAL });
            SetForegroundWindow(есть);
        }
        return;
    }
    let окно = создать_окно();
    if окно == 0 { return; }
    unsafe {
        ShowWindow(окно, SW_SHOWNORMAL);
        SetForegroundWindow(окно);
    }
}

/// Создать главное окно (не показывая): класс, место, тёмный заголовок, значки окна.
fn создать_окно() -> Ручка {
    зарегистрировать(КЛАСС_ОКНА, процедура_окна);
    let (папка, тёмная) = с_видом(|в| (в.папка.clone(), в.тёмная)).unwrap_or_default();
    let м = место_окна(&папка);
    let (к, и) = (широкая(КЛАСС_ОКНА), широкая("Механик"));
    let окно = unsafe { CreateWindowExW(0, к.as_ptr(), и.as_ptr(), WS_OVERLAPPEDWINDOW, м.лев, м.верх, м.ширина(), м.высота(), 0, 0,
        модуль(), std::ptr::null()) };
    if окно == 0 { return 0; }
    тёмный_заголовок(окно, тёмная);
    let dpi = unsafe { GetDpiForWindow(окно) }.max(96);
    let (большой, малый) = (
        znachok::нарисовать(unsafe { GetSystemMetricsForDpi(SM_CXICON, dpi) }, Показ::Спокойно, !тёмная),
        znachok::нарисовать(unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }, Показ::Спокойно, !тёмная));
    unsafe {
        SendMessageW(окно, WM_SETICON, 1, большой);
        SendMessageW(окно, WM_SETICON, 0, малый);
    }
    с_видом(|в| { в.окно = окно; в.значки_окна = (большой, малый); });
    окно
}

fn тёмный_заголовок(окно: Ручка, тёмная: bool) {
    let да: i32 = тёмная as i32;
    let фон = цвет_тона(Тон::Фон, тёмная);
    let текст = цвет_тона(Тон::Текст, тёмная);
    let скругл: i32 = 2;
    unsafe {
        // 20 — тёмный заголовок, 35 — цвет заголовка, 36 — цвет его текста, 33 — скругление (Windows 11)
        DwmSetWindowAttribute(окно, 20, &да as *const i32 as *const c_void, 4);
        DwmSetWindowAttribute(окно, 35, &фон as *const u32 as *const c_void, 4);
        DwmSetWindowAttribute(окно, 36, &текст as *const u32 as *const c_void, 4);
        DwmSetWindowAttribute(окно, 33, &скругл as *const i32 as *const c_void, 4);
    }
}

/// Спрятать (крестик, Esc): значок остаётся; через минуту окно уничтожится.
pub fn спрятать_окно() {
    let окно = с_видом(|в| {
        в.запомнить_место();
        в.спрятано_с = Some(Instant::now());
        в.общее.видно.store(false, Ordering::Relaxed);
        в.окно
    }).unwrap_or(0);
    if окно != 0 { unsafe { ShowWindow(окно, SW_HIDE) }; }
}

/// Закрыть окно совсем (меню значка «Закрыть окно»): освободить GDI сейчас; значок остаётся.
pub fn закрыть_окно() {
    let окно = с_видом(|в| { в.запомнить_место(); в.общее.видно.store(false, Ordering::Relaxed); в.окно }).unwrap_or(0);
    if окно != 0 { unsafe { DestroyWindow(окно) }; }
}

fn исполнить(д: Действие) {
    match д {
        Действие::Страница(с) => { с_видом(|в| в.к_странице(с)); }
        Действие::Копировать => {
            let (окно, текст) = с_видом(|в| (в.окно, в.текст.join("\n"))).unwrap_or_default();
            let ок = в_буфер(окно, &текст);
            с_видом(|в| в.заметить(if ок { "Скопировал страницу в буфер обмена" } else { "Не вышло скопировать" }));
        }
        Действие::ОткрытьПапку => {
            let п = с_видом(|в| в.папка.clone()).unwrap_or_default();
            открыть_папку(&п);
        }
        Действие::ПаузаЧас => super::исполнить_пункт(ПунктМеню::ПаузаЧас),
        Действие::СнятьПаузу => super::исполнить_пункт(ПунктМеню::СнятьПаузу),
        Действие::Остановить => super::исполнить_пункт(ПунктМеню::Остановить),
        Действие::ПриВходе(да) => super::исполнить_пункт(ПунктМеню::ПриВходе(да)),
        Действие::Команда(что, арг) => {
            с_видом(|в| {
                в.номер += 1;
                let ок = sostoyanie::положить_команду(&в.папка, что, &арг, в.номер).is_some();
                в.заметить(if ок { "Передал просьбу — Механик возьмёт её на ближайшем шаге" } else { "Не вышло передать просьбу" });
            });
        }
    }
}

fn координаты(l: isize) -> (i32, i32) { ((l & 0xFFFF) as u16 as i16 as i32, ((l >> 16) & 0xFFFF) as u16 as i16 as i32) }

unsafe extern "system" fn процедура_окна(окно: Ручка, код: u32, w: usize, l: isize) -> isize {
    let итог = std::panic::catch_unwind(|| обработать_окно(окно, код, w, l));
    match итог { Ok(Some(р)) => р, _ => DefWindowProcW(окно, код, w, l) }
}

fn обработать_окно(окно: Ручка, код: u32, w: usize, l: isize) -> Option<isize> {
    match код {
        WM_CLOSE => { спрятать_окно(); Some(0) }
        WM_DESTROY => {
            с_видом(|в| { if в.окно == окно { в.окно = 0; в.буфер = None; в.шрифты = None; в.кнопки.clear(); } });
            Some(0)
        }
        WM_ERASEBKGND => Some(1),
        WM_PAINT => {
            let mut р = Рисование { dc: 0, стереть: 0, прям: Прям::default(), _восст: 0, _доб: 0, _резерв: [0; 32] };
            let dc = unsafe { BeginPaint(окно, &mut р) };
            if dc != 0 { с_видом(|в| { if в.окно == окно { в.нарисовать(dc) } }); }
            unsafe { EndPaint(окно, &р) };
            Some(0)
        }
        WM_PRINTCLIENT => { с_видом(|в| { if в.окно == окно { в.нарисовать(w as Ручка) } }); Some(0) }
        WM_SIZE => {
            // свёрнуто — как спрятано: Мозгу незачем писать сводку (SIZE_MINIMIZED = 1)
            let свёрнуто = w == 1;
            с_видом(|в| { if в.окно == окно { в.общее.видно.store(!свёрнуто && unsafe { IsWindowVisible(окно) } != 0, Ordering::Relaxed); } });
            unsafe { InvalidateRect(окно, std::ptr::null(), 0) };
            Some(0)
        }
        WM_GETMINMAXINFO => {
            let dpi = unsafe { GetDpiForWindow(окно) }.max(96) as f32 / 96.0;
            let п = l as *mut ПределыОкна;
            if !п.is_null() { unsafe { (*п).мин = Точка { x: (760.0 * dpi) as i32, y: (540.0 * dpi) as i32 } }; }
            Some(0)
        }
        WM_DPICHANGED => {
            let п = l as *const Прям;
            с_видом(|в| { в.буфер = None; в.шрифты = None; });
            if !п.is_null() {
                let п = unsafe { *п };
                unsafe { SetWindowPos(окно, 0, п.лев, п.верх, п.ширина(), п.высота(), SWP_NOZORDER | SWP_NOACTIVATE) };
            }
            Some(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = координаты(l);
            let следить = с_видом(|в| {
                let было = в.мышь.and_then(|(x, y)| в.кнопка_под(x, y));
                в.мышь = Some((x, y));
                if было != в.кнопка_под(x, y) { в.перерисовать(); }
                let н = !в.следим_мышь;
                в.следим_мышь = true;
                н
            }).unwrap_or(false);
            if следить {
                // TME_LEAVE = 2
                let mut с = СледитьМышь { размер: std::mem::size_of::<СледитьМышь>() as u32, флаги: 2, окно, задержка: 0 };
                unsafe { TrackMouseEvent(&mut с) };
            }
            Some(0)
        }
        WM_MOUSELEAVE => { с_видом(|в| { в.мышь = None; в.следим_мышь = false; в.перерисовать(); }); Some(0) }
        WM_SETCURSOR => {
            let над = с_видом(|в| в.мышь.and_then(|(x, y)| в.кнопка_под(x, y)).is_some()).unwrap_or(false);
            if над && (l & 0xFFFF) == 1 {
                // HTCLIENT = 1
                unsafe { SetCursor(LoadCursorW(0, IDC_HAND as *const u16)) };
                return Some(1);
            }
            None
        }
        WM_LBUTTONDOWN => { ВЗЯТЬ_МЫШЬ.store(1, Ordering::Relaxed); Some(0) }
        WM_LBUTTONUP => {
            if ВЗЯТЬ_МЫШЬ.swap(0, Ordering::Relaxed) == 0 { return Some(0); }
            let (x, y) = координаты(l);
            let д = с_видом(|в| в.кнопка_под(x, y).map(|н| в.кнопки[н].1.clone())).flatten();
            if let Some(д) = д { исполнить(д); }
            Some(0)
        }
        WM_MOUSEWHEEL => {
            let шаг = ((w >> 16) & 0xFFFF) as u16 as i16 as i32;
            с_видом(|в| { let м = в.масштаб() as f32 / 96.0; в.прокрутить(-(шаг as f32 / 120.0 * 72.0 * м) as i32) });
            Some(0)
        }
        WM_KEYDOWN => {
            let ctrl = unsafe { GetKeyState(VK_CONTROL) } < 0;
            let к = w as u32;
            match к {
                VK_ESCAPE => спрятать_окно(),
                VK_F5 => { с_видом(|в| { в.общее.обновить_сейчас.store(true, Ordering::Relaxed); в.заметить("Обновляю"); }); }
                0x31..=0x37 if ctrl => исполнить(Действие::Страница(Страница::ВСЕ[(к - 0x31) as usize])),
                0x43 if ctrl => исполнить(Действие::Копировать),
                VK_PRIOR | VK_NEXT | VK_HOME | VK_END | VK_UP | VK_DOWN => {
                    с_видом(|в| {
                        let м = в.масштаб() as f32 / 96.0;
                        let страница = (в.видимо as f32 * 0.9) as i32;
                        let на = match к { VK_PRIOR => -страница, VK_NEXT => страница, VK_HOME => -i32::MAX / 2, VK_END => i32::MAX / 2,
                            VK_UP => -(40.0 * м) as i32, _ => (40.0 * м) as i32 };
                        в.прокрутить(на);
                    });
                }
                _ => return None,
            }
            Some(0)
        }
        _ => None,
    }
}

// ───────────────────────────── скрытое окно значка ─────────────────────────────

static TASKBAR_CREATED: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
pub fn сообщение_панели() -> u32 { *TASKBAR_CREATED.get_or_init(|| { let и = широкая("TaskbarCreated"); unsafe { RegisterWindowMessageW(и.as_ptr()) } }) }

unsafe extern "system" fn процедура_значка(окно: Ручка, код: u32, w: usize, l: isize) -> isize {
    let итог = std::panic::catch_unwind(|| обработать_значок(окно, код, w, l));
    match итог { Ok(Some(р)) => р, _ => DefWindowProcW(окно, код, w, l) }
}

fn обработать_значок(окно: Ручка, код: u32, w: usize, l: isize) -> Option<isize> {
    if код == сообщение_панели() && код != 0 {
        // Проводник перезапустился — значок заново
        с_видом(|в| в.поставить_значок());
        return Some(0);
    }
    match код {
        znachok::СООБЩЕНИЕ_ЗНАЧКА => {
            let (событие, x, y) = znachok::событие_значка(w, l);
            match событие {
                NIN_SELECT | NIN_KEYSELECT => {
                    let видно = с_видом(|в| в.окно != 0 && unsafe { IsWindowVisible(в.окно) != 0 && IsIconic(в.окно) == 0 }).unwrap_or(false);
                    if видно { спрятать_окно(); } else { показать_окно(None); }
                }
                NIN_BALLOONUSERCLICK => {
                    let с = с_видом(|в| в.страница_пузыря.take()).flatten();
                    показать_окно(Some(с.unwrap_or(Страница::Сейчас)));
                }
                WM_CONTEXTMENU => {
                    let вид = с_видом(|в| znachok::ВидМеню {
                        пауза_до: в.картина.просьба.пауза_идёт(sostoyanie::сейчас_мс()).then_some(в.картина.просьба.пауза_до_мс),
                        при_входе: в.картина.ответ.as_ref().and_then(|о| о.при_входе).or(в.картина.просьба.при_входе),
                        окно_открыто: в.окно != 0 });
                    if let Some(вид) = вид {
                        if let Some(п) = znachok::показать_меню(окно, x, y, &вид) {
                            match п {
                                ПунктМеню::Открыть => показать_окно(None),
                                ПунктМеню::ЗакрытьОкно => закрыть_окно(),
                                д => super::исполнить_пункт(д),
                            }
                        }
                    }
                }
                _ => {}
            }
            Some(0)
        }
        WM_COPYDATA => {
            let д = l as *const ПереданныеДанные;
            if д.is_null() { return Some(0); }
            let (метка, байт, данные) = unsafe { ((*д).метка, (*д).байт as usize, (*д).данные) };
            if байт > 16 << 10 || (байт > 0 && данные.is_null()) { return Some(0); }
            let текст = if байт == 0 { String::new() } else {
                String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(данные as *const u8, байт) }).into_owned()
            };
            match метка {
                МЕТКА_УВЕДОМЛЕНИЯ => {
                    let Some(у) = serde_json::from_str::<serde_json::Value>(&текст).ok().and_then(|v| Уведомление::из_json(&v)) else { return Some(0) };
                    let итог = с_видом(|в| в.сказать(&у)).unwrap_or((false, "окно занято"));
                    super::записать_себе("уведомление", &serde_json::json!({ "заголовок": у.заголовок, "текст": у.текст,
                        "вид": у.вид.ключ(), "показано": итог.0, "почему": итог.1, "от": "другой процесс" }));
                    Some(итог.0 as isize)
                }
                МЕТКА_ОТКРЫТЬ => { показать_окно(Страница::из(текст.trim())); Some(1) }
                _ => Some(0),
            }
        }
        КАРТИНА_ГОТОВА => {
            let к = с_видом(|в| в.общее.картина.lock().ok().and_then(|mut к| к.take())).flatten();
            if let Some(к) = к { с_видом(|в| в.применить(к)); }
            Some(0)
        }
        ОЧЕРЕДЬ_ГОТОВА => {
            let очередь: Vec<Уведомление> = с_видом(|в| в.общее.очередь.lock().map(|mut о| std::mem::take(&mut *о)).unwrap_or_default())
                .unwrap_or_default();
            for у in очередь {
                let итог = с_видом(|в| в.сказать(&у)).unwrap_or((false, "окно занято"));
                super::записать_себе("уведомление", &serde_json::json!({ "заголовок": у.заголовок, "текст": у.текст,
                    "вид": у.вид.ключ(), "показано": итог.0, "почему": итог.1, "от": "очередь" }));
            }
            Some(0)
        }
        WM_TIMER => {
            match w {
                ТАЙМЕР_ВЫХОД => {
                    // «Остановить до входа»: Мозг всё вернул и вышел — выходим и мы. Не вышел за 20 с (не умеет или занят) —
                    // окно остаётся (он видит, что «Механик» работает) и честно говорит; просьба лежит — Мозг возьмёт её сам
                    if !мьютекс_есть(super::МЬЮТЕКС_МОЗГА) {
                        unsafe { KillTimer(окно, ТАЙМЕР_ВЫХОД); DestroyWindow(окно); }
                        return Some(0);
                    }
                    let долго = с_видом(|в| в.выход_с.is_some_and(|к| к.elapsed() > Duration::from_secs(20))).unwrap_or(false);
                    if долго {
                        unsafe { KillTimer(окно, ТАЙМЕР_ВЫХОД) };
                        let у = Уведомление { мс: sostoyanie::сейчас_мс(), заголовок: "Механик не остановился".into(),
                            текст: "За 20 с он не вышел. Просьба записана — остановится, как только возьмёт её.".into(),
                            вид: ВидУведомления::Внимание, страница: Some(Страница::Настройки.ключ().into()) };
                        let итог = с_видом(|в| { в.выход_с = None; в.заметить("Механик не остановился за 20 с — окно осталось"); в.сказать(&у) });
                        super::записать_себе("остановка_не_вышла", &serde_json::json!({ "показано": итог.map(|и| и.0) }));
                    }
                }
                ТАЙМЕР_УБОРКА => {
                    let уничтожить = с_видом(|в| (в.окно != 0 && в.спрятано_с.is_some_and(|к| к.elapsed() >= СПРЯТАНО_УНИЧТОЖИТЬ))
                        .then_some(в.окно)).flatten();
                    if let Some(о) = уничтожить { unsafe { DestroyWindow(о) }; }
                    if с_видом(|в| в.общее.выход.load(Ordering::Relaxed)).unwrap_or(false) { unsafe { DestroyWindow(окно) }; }
                }
                _ => {}
            }
            Some(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            let имя = if l != 0 { unsafe { строка_из(l as *const u16) } } else { String::new() };
            if код == WM_THEMECHANGED || имя == "ImmersiveColorSet" {
                let (прил, панель) = тема_светлая();
                тёмное_меню(!прил);
                let окно_главное = с_видом(|в| {
                    в.тёмная = !прил;
                    в.светлая_панель = панель;
                    в.иконка_показ = None;
                    в.обновить_значок();
                    в.перерисовать();
                    в.окно
                }).unwrap_or(0);
                if окно_главное != 0 { тёмный_заголовок(окно_главное, !прил); }
            }
            Some(0)
        }
        WM_QUERYENDSESSION => Some(1),
        WM_ENDSESSION => { if w != 0 { с_видом(|в| { в.запомнить_место(); в.лоток.убрать(); }); } Some(0) }
        WM_CLOSE => { unsafe { DestroyWindow(окно) }; Some(0) }
        WM_DESTROY => {
            let главное = с_видом(|в| { в.запомнить_место(); в.лоток.убрать(); в.окно }).unwrap_or(0);
            if главное != 0 { unsafe { DestroyWindow(главное) }; }
            unsafe { PostQuitMessage(0) };
            Some(0)
        }
        _ => None,
    }
}

/// Строка UTF-16 до нуля (не длиннее 256 знаков).
unsafe fn строка_из(п: *const u16) -> String {
    let mut н = 0;
    while н < 256 && *п.add(н) != 0 { н += 1; }
    String::from_utf16_lossy(std::slice::from_raw_parts(п, н))
}

/// Таймеры скрытого окна: уборка спрятанного окна (раз в 10 с).
pub fn завести_таймеры(скрытое: Ручка) { unsafe { SetTimer(скрытое, ТАЙМЕР_УБОРКА, 10_000, std::ptr::null()) }; }
/// «Остановить до входа»: проверять раз в полсекунды, вышел ли Мозг.
pub fn ждать_выхода_мозга(скрытое: Ручка) { unsafe { SetTimer(скрытое, ТАЙМЕР_ВЫХОД, 500, std::ptr::null()) }; }

/// Очередь сообщений потока окна — до `PostQuitMessage`.
pub fn крутить_сообщения() {
    let mut с = Сообщение { окно: 0, код: 0, w: 0, l: 0, время: 0, x: 0, y: 0, _частное: 0 };
    unsafe {
        while GetMessageW(&mut с, 0, 0, 0) > 0 {
            TranslateMessage(&с);
            DispatchMessageW(&с);
        }
    }
}

// ───────────────────────────── живая проба значка ─────────────────────────────

/// Живая проба (только своё): значок у часов на `сколько` — все состояния по кругу, потом убрать. Без уведомлений.
#[cfg(test)]
pub fn живьём_значок(сколько: Duration) {
    // как роль окна: масштаб по монитору (значок — по настоящему масштабу) и тёмное меню (uxtheme №135 не роняет процесс)
    масштаб_по_монитору();
    let меню = тёмное_меню(!тема_светлая().0);
    зарегистрировать(КЛАСС_ЗНАЧКА, процедура_значка);
    let (к, и) = (широкая(КЛАСС_ЗНАЧКА), широкая("Механик — проба значка"));
    let скрытое = unsafe { CreateWindowExW(0, к.as_ptr(), и.as_ptr(), WS_POPUP, 0, 0, 0, 0, 0, 0, модуль(), std::ptr::null()) };
    assert_ne!(скрытое, 0, "скрытое окно");
    let (_, светлая) = тема_светлая();
    let р = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()) }.max(16);
    let mut лоток = Лоток::новый(скрытое);
    let состояния = [Показ::Спокойно, Показ::Работает, Показ::Внимание, Показ::Тревога, Показ::Пауза, Показ::Молчит];
    let первая = znachok::нарисовать(р, состояния[0], светлая);
    assert_ne!(первая, 0, "картинка значка");
    assert!(лоток.добавить(первая, "Механик · проба значка (5 с), сейчас уберу"), "Shell_NotifyIconW(NIM_ADD) не принял");
    let старт = Instant::now();
    let mut было = первая;
    let mut шаг = 0usize;
    let mut с = Сообщение { окно: 0, код: 0, w: 0, l: 0, время: 0, x: 0, y: 0, _частное: 0 };
    while старт.elapsed() < сколько {
        let нужно = ((старт.elapsed().as_millis() as u64 * состояния.len() as u64) / сколько.as_millis().max(1) as u64) as usize;
        if нужно != шаг && нужно < состояния.len() {
            шаг = нужно;
            let н = znachok::нарисовать(р, состояния[шаг], светлая);
            assert!(лоток.обновить(Some(н), &format!("Механик · проба значка: {:?}", состояния[шаг])), "NIM_MODIFY");
            unsafe { DestroyIcon(было) };
            было = н;
        }
        unsafe { while PeekMessageW(&mut с, 0, 0, 0, 1) != 0 { TranslateMessage(&с); DispatchMessageW(&с); } }
        std::thread::sleep(Duration::from_millis(50));
    }
    лоток.убрать();
    unsafe { DestroyIcon(было); DestroyWindow(скрытое); }
    eprintln!("значок {р}×{р} (масштаб {}): показан {} с, {} состояний, убран; тёмное меню: {меню}", unsafe { GetDpiForSystem() },
        сколько.as_secs(), шаг + 1);
}

#[cfg(test)]
mod тесты {
    use super::*;
    use super::super::stranicy::тесты::полная_картина;

    #[test]
    fn png_по_правилам() {
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082, "CRC пустого IEND — известное число");
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        let png = в_png(&[0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0x00FF_FFFF], 2, 2);
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&png[png.len() - 12..], &[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
        // данные: строка — фильтр 0 и RGB
        let idat = png.windows(4).position(|о| о == b"IDAT").unwrap() + 4;
        assert_eq!(&png[idat..idat + 2], &[0x78, 0x01]);
        assert_eq!(&png[idat + 7..idat + 7 + 7], &[0, 255, 0, 0, 0, 255, 0], "первая строка: фильтр, красный, зелёный");
    }

    /// Рисунок вне экрана — тем же кодом, что и окно: фон по палитре, текст есть, кнопки страниц на месте.
    #[test]
    fn окно_рисуется_вне_экрана() {
        let сейчас = 1_794_607_430_000;
        let картина = полная_картина(сейчас);
        for тёмная in [true, false] {
            let кадр = Кадр { картина: &картина, страница: Страница::Сейчас, прокрутка: 0, мышь: None, тёмная, dpi: 144, сейчас, заметка: None };
            let (ш, в) = (1410, 1080);
            let (пиксели, итог) = нарисовать_в_память(ш, в, &кадр).expect("рисунок в памяти");
            let фон = цвет_тона(Тон::Фон, тёмная);
            // COLORREF 0x00BBGGRR → пиксель DIB 0x00RRGGBB
            let фон_dib = ((фон & 0xFF) << 16) | (фон & 0xFF00) | ((фон >> 16) & 0xFF);
            assert_eq!(пиксели[(в as usize - 2) * ш as usize + 2], фон_dib, "нижний левый угол — фон");
            let не_фон = пиксели.iter().filter(|п| **п != фон_dib).count();
            assert!(не_фон > (ш * в / 50) as usize, "нарисовано мало: {не_фон}");
            assert_eq!(итог.кнопки.iter().filter(|(_, д)| matches!(д, Действие::Страница(_))).count(), 7, "все 7 страниц в меню");
            assert!(итог.текст.iter().any(|т| т.contains("Тормозит 25 с")), "строка состояния в шапке");
            for (п, _) in &итог.кнопки { assert!(п.лев >= 0 && п.прав <= ш, "кнопка за краем {п:?}"); }
        }
    }

    /// Живая проба (только своё, на экран ничего не выводит): настоящее главное окно создаётся невидимым, рисует себя по
    /// `WM_PRINTCLIENT` (тот же путь, что `WM_PAINT`: буфер, шрифты по масштабу, раскладка), щелчок по «Кто ест» в меню
    /// страниц меняет страницу, Esc прячет; потом всё убрать.
    #[test]
    #[ignore]
    fn живьём_окно_рисует_себя() {
        масштаб_по_монитору();
        let папка = std::env::temp_dir().join(format!("mekhanik-vid-okno-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&папка);
        let скрытое = создать_скрытое();
        assert_ne!(скрытое, 0, "скрытое окно значка");
        let общее = Arc::new(super::super::Общее::default());
        let mut вид = Вид::новый(папка.clone(), общее.clone(), скрытое);
        вид.картина = полная_картина(sostoyanie::сейчас_мс());
        let тёмная = вид.тёмная;
        поставить_вид(вид);
        let окно = создать_окно();
        assert_ne!(окно, 0, "главное окно");
        assert_eq!(unsafe { IsWindowVisible(окно) }, 0, "окно не показано — проба ничего не выводит на экран");
        let mut п = Прям::default();
        unsafe { GetClientRect(окно, &mut п) };
        let (ш, в) = (п.ширина(), п.высота());
        assert!(ш > 600 && в > 400, "место по умолчанию — 940×720 DIP: {ш}×{в}");
        let (пиксели, dpi) = unsafe {
            let dc = CreateCompatibleDC(0);
            let з = ЗаголовокРисунка { размер: 40, ширина: ш, высота: -в, плоскостей: 1, бит: 32, сжатие: 0, байт: 0, _x: 0, _y: 0,
                _цветов: 0, _важных: 0 };
            let mut биты: *mut c_void = std::ptr::null_mut();
            let рис = CreateDIBSection(dc, &з, 0, &mut биты, 0, 0);
            let было = SelectObject(dc, рис);
            // PRF_CLIENT = 4
            SendMessageW(окно, WM_PRINTCLIENT, dc as usize, 4);
            GdiFlush();
            let п: Vec<u32> = std::slice::from_raw_parts(биты as *const u32, (ш * в) as usize).iter().map(|п| п & 0x00FF_FFFF).collect();
            SelectObject(dc, было);
            DeleteObject(рис);
            DeleteDC(dc);
            (п, GetDpiForWindow(окно))
        };
        let фон = цвет_тона(Тон::Фон, тёмная);
        let фон_dib = ((фон & 0xFF) << 16) | (фон & 0xFF00) | ((фон >> 16) & 0xFF);
        assert_eq!(пиксели[((в - 2) * ш + 2) as usize], фон_dib, "окно нарисовало себя: фон по палитре");
        assert!(пиксели.iter().filter(|п| **п != фон_dib).count() > (ш * в / 50) as usize, "и не только фон");
        let png = std::env::var_os("MEKHANIK_PNG").map(PathBuf::from).unwrap_or_else(|| папка.clone()).join("okno-zhivoe.png");
        let _ = std::fs::write(&png, в_png(&пиксели, ш as u32, в as u32));
        let кнопки = с_видом(|в| в.кнопки.clone()).expect("вид");
        let (к, _) = кнопки.iter().find(|(_, д)| *д == Действие::Страница(Страница::КтоЕст)).expect("кнопка «Кто ест»");
        let l = ((((к.верх + к.низ) / 2) as isize) << 16) | (((к.лев + к.прав) / 2) as isize & 0xFFFF);
        unsafe { SendMessageW(окно, WM_LBUTTONDOWN, 1, l); SendMessageW(окно, WM_LBUTTONUP, 0, l); }
        assert_eq!(с_видом(|в| в.страница), Some(Страница::КтоЕст), "щелчок по меню страниц");
        assert_eq!(общее.страница.load(Ordering::Relaxed), 1, "рабочему потоку — какая страница");
        общее.видно.store(true, Ordering::Relaxed);
        unsafe { SendMessageW(окно, WM_KEYDOWN, VK_ESCAPE as usize, 0) };
        assert!(!общее.видно.load(Ordering::Relaxed), "Esc прячет");
        закрыть_окно();
        assert_eq!(с_видом(|в| в.окно), Some(0), "«Закрыть окно» — уничтожено, значок остаётся");
        unsafe { DestroyWindow(скрытое) };
        снять_вид();
        eprintln!("окно {ш}×{в} при масштабе {dpi}: нарисовало себя, щелчок и Esc — как надо; снимок {}", png.display());
        let _ = std::fs::remove_dir_all(&папка);
    }

    /// Снимки всех страниц вне экрана в PNG (для глаз): тёмная и светлая тема, 150 % и 100 %. Папка — `MEKHANIK_PNG`
    /// или `%CARGO_TARGET_DIR%\vid-png`; в репозиторий не входят.
    #[test]
    #[ignore]
    fn снимки_страниц_в_png() {
        let сейчас = sostoyanie::сейчас_мс();
        let картина = полная_картина(сейчас);
        let пустая = Картина { мс: сейчас, ..Default::default() };
        let папка = std::env::var_os("MEKHANIK_PNG").map(PathBuf::from).unwrap_or_else(|| {
            std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("vid-png")
        });
        std::fs::create_dir_all(&папка).unwrap();
        let mut всего = 0;
        for (dpi, ш, в) in [(144u32, 1410, 1080), (96, 940, 720)] {
            for тёмная in [true, false] {
                for с in Страница::ВСЕ {
                    for (имя, к) in [("полная", &картина), ("пустая", &пустая)] {
                        if имя == "пустая" && (dpi != 144 || !тёмная) { continue; }
                        let кадр = Кадр { картина: к, страница: с, прокрутка: 0, мышь: None, тёмная, dpi, сейчас, заметка: None };
                        let (пиксели, итог) = нарисовать_в_память(ш, в, &кадр).expect("рисунок");
                        let файл = папка.join(format!("{}-{}-{}-{}.png", с.ключ(), if тёмная { "temnaya" } else { "svetlaya" }, dpi,
                            if имя == "полная" { "dannye" } else { "pusto" }));
                        std::fs::write(&файл, в_png(&пиксели, ш as u32, в as u32)).unwrap();
                        // длинная страница — ещё и низ
                        if итог.высота_листа > итог.видимо && имя == "полная" {
                            let кадр = Кадр { прокрутка: i32::MAX / 2, ..кадр };
                            let (пиксели, _) = нарисовать_в_память(ш, в, &кадр).expect("рисунок");
                            std::fs::write(файл.with_file_name(format!("{}-niz.png", файл.file_stem().unwrap().to_string_lossy())),
                                в_png(&пиксели, ш as u32, в as u32)).unwrap();
                            всего += 1;
                        }
                        всего += 1;
                    }
                }
            }
        }
        // значки: все состояния, 16…48 точек, на тёмной и светлой панели
        for р in [16, 24, 32, 48] {
            for светлая in [false, true] {
                let состояния = [Показ::Спокойно, Показ::Работает, Показ::Внимание, Показ::Тревога, Показ::Пауза, Показ::Молчит];
                let (ш, в) = (р * состояния.len() as i32 + 4 * (состояния.len() as i32 + 1), р + 8);
                let фон: u32 = if светлая { 0xF3F3F3 } else { 0x202020 };
                let mut пиксели = vec![фон; (ш * в) as usize];
                for (н, п) in состояния.iter().enumerate() {
                    let покрытие = znachok::покрытие_знака(р).unwrap_or_else(|| vec![0; (р * р) as usize]);
                    let к = znachok::собрать(&покрытие, р as usize, *п, светлая);
                    let x0 = 4 + н as i32 * (р + 4);
                    for y in 0..р { for x in 0..р {
                        let с = к[(y * р + x) as usize];
                        let а = (с >> 24) as f32 / 255.0;
                        let смесь = |сдвиг: u32| -> u32 { (((с >> сдвиг) & 0xFF) as f32 * а + ((фон >> сдвиг) & 0xFF) as f32 * (1.0 - а)).round() as u32 };
                        пиксели[((y + 4) * ш + x0 + x) as usize] = смесь(16) << 16 | смесь(8) << 8 | смесь(0);
                    } }
                }
                std::fs::write(папка.join(format!("znachok-{р}-{}.png", if светлая { "svetlaya" } else { "temnaya" })),
                    в_png(&пиксели, ш as u32, в as u32)).unwrap();
                всего += 1;
            }
        }
        eprintln!("снимков: {всего} → {}", папка.display());
    }
}
