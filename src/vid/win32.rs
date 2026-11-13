//! Вызовы Windows для вида «Механика» (значок у часов и окно, 13.11): своими объявлениями, без крейта `windows`, как
//! `okna.rs`. Только то, что нужно окну: окна и сообщения, рисование GDI, значок у часов, тема и масштаб экрана, вход
//! сеанса. Ни одного сетевого вызова: окно ничего не грузит из сети (его слово 13.11 — «Механик» полностью офлайн).

use std::ffi::c_void;

/// HWND, HDC, HGDIOBJ, HICON, HMENU — всё числом, как в `okna.rs`.
pub type Ручка = isize;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Прям { pub лев: i32, pub верх: i32, pub прав: i32, pub низ: i32 }

impl Прям {
    pub fn новый(x: i32, y: i32, ш: i32, в: i32) -> Прям { Прям { лев: x, верх: y, прав: x + ш, низ: y + в } }
    pub fn ширина(&self) -> i32 { self.прав - self.лев }
    pub fn высота(&self) -> i32 { self.низ - self.верх }
    pub fn внутри(&self, x: i32, y: i32) -> bool { x >= self.лев && x < self.прав && y >= self.верх && y < self.низ }
    #[cfg(test)]
    pub fn пересекает(&self, д: &Прям) -> bool {
        self.лев < д.прав && д.лев < self.прав && self.верх < д.низ && д.верх < self.низ
    }
    pub fn сдвиг(&self, dx: i32, dy: i32) -> Прям {
        Прям { лев: self.лев + dx, верх: self.верх + dy, прав: self.прав + dx, низ: self.низ + dy }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Точка { pub x: i32, pub y: i32 }

#[repr(C)]
pub struct Сообщение { pub окно: Ручка, pub код: u32, pub w: usize, pub l: isize, pub время: u32, pub x: i32, pub y: i32,
    pub _частное: u32 }

#[repr(C)]
pub struct КлассОкна {
    pub размер: u32, pub стиль: u32, pub процедура: Option<Процедура>, pub _доп_класса: i32, pub _доп_окна: i32,
    pub модуль: Ручка, pub значок: Ручка, pub курсор: Ручка, pub фон: Ручка, pub _меню: *const u16, pub имя: *const u16,
    pub значок_малый: Ручка,
}

pub type Процедура = unsafe extern "system" fn(Ручка, u32, usize, isize) -> isize;

#[repr(C)]
pub struct Рисование { pub dc: Ручка, pub стереть: i32, pub прям: Прям, pub _восст: i32, pub _доб: i32, pub _резерв: [u8; 32] }

/// NOTIFYICONDATAW целиком (Vista и новее): 976 байт на x64 — проверено тестом.
#[repr(C)]
pub struct ДанныеЗначка {
    pub размер: u32, pub окно: Ручка, pub номер: u32, pub флаги: u32, pub сообщение: u32, pub значок: Ручка,
    pub подсказка: [u16; 128], pub состояние: u32, pub маска_состояния: u32, pub текст: [u16; 256], pub версия: u32,
    pub заголовок: [u16; 64], pub флаги_текста: u32, pub guid: [u8; 16], pub значок_текста: Ручка,
}

impl ДанныеЗначка {
    pub fn новые(окно: Ручка, номер: u32) -> ДанныеЗначка {
        ДанныеЗначка { размер: std::mem::size_of::<ДанныеЗначка>() as u32, окно, номер, флаги: 0, сообщение: 0, значок: 0,
            подсказка: [0; 128], состояние: 0, маска_состояния: 0, текст: [0; 256], версия: 0, заголовок: [0; 64],
            флаги_текста: 0, guid: [0; 16], значок_текста: 0 }
    }
}

#[repr(C)]
pub struct СведенияЗначка { pub значок: i32, pub x: u32, pub y: u32, pub маска: Ручка, pub цвет: Ручка }

#[repr(C)]
pub struct ЗаголовокРисунка { pub размер: u32, pub ширина: i32, pub высота: i32, pub плоскостей: u16, pub бит: u16,
    pub сжатие: u32, pub байт: u32, pub _x: i32, pub _y: i32, pub _цветов: u32, pub _важных: u32 }

#[repr(C)]
pub struct ПереданныеДанные { pub метка: usize, pub байт: u32, pub данные: *const c_void }

#[repr(C)]
pub struct СведенияМонитора { pub размер: u32, pub монитор: Прям, pub работа: Прям, pub флаги: u32 }

#[repr(C)]
pub struct СледитьМышь { pub размер: u32, pub флаги: u32, pub окно: Ручка, pub задержка: u32 }

#[repr(C)]
pub struct ПределыОкна { pub _р: Точка, pub _макс: Точка, pub _место: Точка, pub мин: Точка, pub _макс_тяни: Точка }

#[repr(C)]
#[derive(Default)]
pub struct Размер { pub ш: i32, pub в: i32 }

#[repr(C)]
pub struct ПамятьПК { pub длина: u32, pub загрузка: u32, pub всего_физ: u64, pub доступно_физ: u64, pub всего_зач: u64,
    pub доступно_зач: u64, pub всего_вирт: u64, pub доступно_вирт: u64, pub доступно_расш: u64 }

#[link(name = "user32")]
extern "system" {
    pub fn RegisterClassExW(к: *const КлассОкна) -> u16;
    pub fn CreateWindowExW(доп: u32, класс: *const u16, имя: *const u16, стиль: u32, x: i32, y: i32, ш: i32, в: i32,
                           родитель: Ручка, меню: Ручка, модуль: Ручка, параметр: *const c_void) -> Ручка;
    pub fn DefWindowProcW(окно: Ручка, код: u32, w: usize, l: isize) -> isize;
    pub fn DestroyWindow(окно: Ручка) -> i32;
    pub fn ShowWindow(окно: Ручка, как: i32) -> i32;
    pub fn IsWindowVisible(окно: Ручка) -> i32;
    pub fn IsIconic(окно: Ручка) -> i32;
    pub fn GetMessageW(с: *mut Сообщение, окно: Ручка, от: u32, до: u32) -> i32;
    #[cfg(test)]
    pub fn PeekMessageW(с: *mut Сообщение, окно: Ручка, от: u32, до: u32, убрать: u32) -> i32;
    pub fn TranslateMessage(с: *const Сообщение) -> i32;
    pub fn DispatchMessageW(с: *const Сообщение) -> isize;
    pub fn PostMessageW(окно: Ручка, код: u32, w: usize, l: isize) -> i32;
    pub fn PostQuitMessage(код: i32);
    pub fn SendMessageW(окно: Ручка, код: u32, w: usize, l: isize) -> isize;
    pub fn SendMessageTimeoutW(окно: Ручка, код: u32, w: usize, l: isize, флаги: u32, мс: u32, итог: *mut usize) -> isize;
    pub fn FindWindowW(класс: *const u16, имя: *const u16) -> Ручка;
    pub fn SetTimer(окно: Ручка, номер: usize, мс: u32, ф: *const c_void) -> usize;
    pub fn KillTimer(окно: Ручка, номер: usize) -> i32;
    pub fn InvalidateRect(окно: Ручка, п: *const Прям, стереть: i32) -> i32;
    pub fn BeginPaint(окно: Ручка, р: *mut Рисование) -> Ручка;
    pub fn EndPaint(окно: Ручка, р: *const Рисование) -> i32;
    pub fn GetClientRect(окно: Ручка, п: *mut Прям) -> i32;
    pub fn GetWindowRect(окно: Ручка, п: *mut Прям) -> i32;
    pub fn SetWindowPos(окно: Ручка, после: Ручка, x: i32, y: i32, ш: i32, в: i32, флаги: u32) -> i32;
    pub fn GetDpiForWindow(окно: Ручка) -> u32;
    pub fn GetDpiForSystem() -> u32;
    pub fn GetSystemMetricsForDpi(что: i32, dpi: u32) -> i32;
    pub fn SetProcessDpiAwarenessContext(контекст: isize) -> i32;
    pub fn CreatePopupMenu() -> Ручка;
    pub fn AppendMenuW(меню: Ручка, флаги: u32, номер: usize, текст: *const u16) -> i32;
    pub fn SetMenuDefaultItem(меню: Ручка, номер: u32, по_месту: u32) -> i32;
    pub fn TrackPopupMenuEx(меню: Ручка, флаги: u32, x: i32, y: i32, окно: Ручка, _п: *const c_void) -> i32;
    pub fn DestroyMenu(меню: Ручка) -> i32;
    pub fn SetForegroundWindow(окно: Ручка) -> i32;
    pub fn GetCursorPos(т: *mut Точка) -> i32;
    pub fn CreateIconIndirect(с: *const СведенияЗначка) -> Ручка;
    pub fn DestroyIcon(з: Ручка) -> i32;
    pub fn LoadCursorW(модуль: Ручка, имя: *const u16) -> Ручка;
    pub fn SetCursor(к: Ручка) -> Ручка;
    pub fn RegisterWindowMessageW(имя: *const u16) -> u32;
    pub fn MessageBoxW(окно: Ручка, текст: *const u16, заголовок: *const u16, вид: u32) -> i32;
    pub fn OpenClipboard(окно: Ручка) -> i32;
    pub fn EmptyClipboard() -> i32;
    pub fn SetClipboardData(формат: u32, данные: Ручка) -> Ручка;
    pub fn CloseClipboard() -> i32;
    pub fn GetKeyState(клавиша: i32) -> i16;
    pub fn TrackMouseEvent(с: *mut СледитьМышь) -> i32;
    pub fn MonitorFromPoint(т: Точка, флаги: u32) -> Ручка;
    pub fn GetMonitorInfoW(монитор: Ручка, с: *mut СведенияМонитора) -> i32;
    pub fn FillRect(dc: Ручка, п: *const Прям, кисть: Ручка) -> i32;
    pub fn DrawTextW(dc: Ручка, текст: *const u16, длина: i32, п: *mut Прям, формат: u32) -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateCompatibleDC(dc: Ручка) -> Ручка;
    pub fn DeleteDC(dc: Ручка) -> i32;
    pub fn CreateCompatibleBitmap(dc: Ручка, ш: i32, в: i32) -> Ручка;
    pub fn CreateBitmap(ш: i32, в: i32, плоскостей: u32, бит: u32, данные: *const c_void) -> Ручка;
    pub fn CreateDIBSection(dc: Ручка, з: *const ЗаголовокРисунка, вид: u32, биты: *mut *mut c_void, _р: Ручка, _с: u32) -> Ручка;
    pub fn SelectObject(dc: Ручка, о: Ручка) -> Ручка;
    pub fn DeleteObject(о: Ручка) -> i32;
    pub fn BitBlt(dc: Ручка, x: i32, y: i32, ш: i32, в: i32, из: Ручка, x1: i32, y1: i32, оп: u32) -> i32;
    pub fn CreateFontW(высота: i32, ширина: i32, наклон: i32, поворот: i32, вес: i32, курсив: u32, подчерк: u32,
                       зачерк: u32, набор: u32, точность: u32, обрезка: u32, качество: u32, шаг: u32, имя: *const u16) -> Ручка;
    pub fn SetTextColor(dc: Ручка, цвет: u32) -> u32;
    pub fn SetBkMode(dc: Ручка, вид: i32) -> i32;
    pub fn CreateSolidBrush(цвет: u32) -> Ручка;
    pub fn CreatePen(вид: i32, ширина: i32, цвет: u32) -> Ручка;
    pub fn GetStockObject(что: i32) -> Ручка;
    pub fn RoundRect(dc: Ручка, л: i32, в: i32, п: i32, н: i32, ш: i32, вы: i32) -> i32;
    pub fn Ellipse(dc: Ручка, л: i32, в: i32, п: i32, н: i32) -> i32;
    pub fn GetTextExtentPoint32W(dc: Ручка, текст: *const u16, длина: i32, р: *mut Размер) -> i32;
    pub fn GetGlyphIndicesW(dc: Ручка, текст: *const u16, длина: i32, номера: *mut u16, флаги: u32) -> u32;
    pub fn GetTextFaceW(dc: Ручка, длина: i32, имя: *mut u16) -> i32;
    pub fn SaveDC(dc: Ручка) -> i32;
    pub fn RestoreDC(dc: Ручка, какой: i32) -> i32;
    pub fn IntersectClipRect(dc: Ручка, л: i32, в: i32, п: i32, н: i32) -> i32;
    pub fn GdiFlush() -> i32;
}

#[link(name = "shell32")]
extern "system" {
    pub fn Shell_NotifyIconW(что: u32, д: *const ДанныеЗначка) -> i32;
    pub fn ShellExecuteW(окно: Ручка, действие: *const u16, файл: *const u16, параметры: *const u16, папка: *const u16,
                         как: i32) -> Ручка;
    pub fn SetCurrentProcessExplicitAppUserModelID(имя: *const u16) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    pub fn DwmSetWindowAttribute(окно: Ручка, что: u32, значение: *const c_void, размер: u32) -> i32;
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(ключ: isize, путь: *const u16, имя: *const u16, флаги: u32, тип: *mut u32, данные: *mut c_void,
                    байт: *mut u32) -> i32;
}

#[link(name = "wtsapi32")]
extern "system" {
    fn WTSQuerySessionInformationW(сервер: Ручка, сеанс: u32, что: u32, буфер: *mut *mut u16, байт: *mut u32) -> i32;
    fn WTSFreeMemory(п: *mut c_void);
}

#[link(name = "kernel32")]
extern "system" {
    pub fn GetModuleHandleW(имя: *const u16) -> Ручка;
    fn LoadLibraryW(имя: *const u16) -> Ручка;
    fn GetProcAddress(модуль: Ручка, имя: *const u8) -> *const c_void;
    fn CreateMutexW(атрибуты: *const c_void, взять: i32, имя: *const u16) -> Ручка;
    fn OpenMutexW(доступ: u32, наследовать: i32, имя: *const u16) -> Ручка;
    pub fn CloseHandle(h: Ручка) -> i32;
    fn GetLastError() -> u32;
    fn GetCurrentProcess() -> Ручка;
    fn SetPriorityClass(h: Ручка, класс: u32) -> i32;
    fn GlobalAlloc(флаги: u32, байт: usize) -> Ручка;
    fn GlobalLock(h: Ручка) -> *mut c_void;
    fn GlobalUnlock(h: Ручка) -> i32;
    fn GlobalFree(h: Ручка) -> Ручка;
    fn GlobalMemoryStatusEx(м: *mut ПамятьПК) -> i32;
    fn GetSystemTimes(простой: *mut u64, ядро: *mut u64, польз: *mut u64) -> i32;
    fn GetTickCount64() -> u64;
}

// ── числа Windows, что нужны окну ──
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_SIZE: u32 = 0x0005;
pub const WM_PAINT: u32 = 0x000F;
pub const WM_CLOSE: u32 = 0x0010;
pub const WM_QUERYENDSESSION: u32 = 0x0011;
pub const WM_ERASEBKGND: u32 = 0x0014;
pub const WM_ENDSESSION: u32 = 0x0016;
pub const WM_SETTINGCHANGE: u32 = 0x001A;
pub const WM_SETCURSOR: u32 = 0x0020;
pub const WM_GETMINMAXINFO: u32 = 0x0024;
pub const WM_COPYDATA: u32 = 0x004A;
pub const WM_CONTEXTMENU: u32 = 0x007B;
pub const WM_SETICON: u32 = 0x0080;
pub const WM_KEYDOWN: u32 = 0x0100;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_MOUSEMOVE: u32 = 0x0200;
pub const WM_LBUTTONDOWN: u32 = 0x0201;
pub const WM_LBUTTONUP: u32 = 0x0202;
pub const WM_MOUSEWHEEL: u32 = 0x020A;
pub const WM_MOUSELEAVE: u32 = 0x02A3;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const WM_PRINTCLIENT: u32 = 0x0318;
pub const WM_THEMECHANGED: u32 = 0x031A;
pub const WM_USER: u32 = 0x0400;
pub const WM_APP: u32 = 0x8000;

pub const NIM_ADD: u32 = 0;
pub const NIM_MODIFY: u32 = 1;
pub const NIM_DELETE: u32 = 2;
pub const NIM_SETVERSION: u32 = 4;
pub const NIF_MESSAGE: u32 = 0x01;
pub const NIF_ICON: u32 = 0x02;
pub const NIF_TIP: u32 = 0x04;
pub const NIF_INFO: u32 = 0x10;
pub const NIF_SHOWTIP: u32 = 0x80;
pub const NIIF_INFO: u32 = 0x01;
pub const NIIF_WARNING: u32 = 0x02;
pub const NIIF_ERROR: u32 = 0x03;
pub const NIIF_NOSOUND: u32 = 0x10;
pub const NIIF_RESPECT_QUIET_TIME: u32 = 0x80;
pub const NOTIFYICON_VERSION_4: u32 = 4;
pub const NIN_SELECT: u32 = WM_USER;
pub const NIN_KEYSELECT: u32 = WM_USER + 1;
pub const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;

pub const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
pub const WS_POPUP: u32 = 0x8000_0000;
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNORMAL: i32 = 1;
pub const SW_RESTORE: i32 = 9;
pub const SWP_NOZORDER: u32 = 0x0004;
pub const SWP_NOACTIVATE: u32 = 0x0010;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const TPM_NONOTIFY: u32 = 0x0080;
pub const TPM_RETURNCMD: u32 = 0x0100;
pub const MF_STRING: u32 = 0x0000;
pub const MF_GRAYED: u32 = 0x0001;
pub const MF_CHECKED: u32 = 0x0008;
pub const MF_SEPARATOR: u32 = 0x0800;
pub const DT_LEFT: u32 = 0x0000;
pub const DT_CENTER: u32 = 0x0001;
pub const DT_RIGHT: u32 = 0x0002;
pub const DT_VCENTER: u32 = 0x0004;
pub const DT_SINGLELINE: u32 = 0x0020;
pub const DT_NOCLIP: u32 = 0x0100;
pub const DT_NOPREFIX: u32 = 0x0800;
pub const DT_END_ELLIPSIS: u32 = 0x8000;
pub const SRCCOPY: u32 = 0x00CC_0020;
pub const VK_ESCAPE: u32 = 0x1B;
pub const VK_PRIOR: u32 = 0x21;
pub const VK_NEXT: u32 = 0x22;
pub const VK_END: u32 = 0x23;
pub const VK_HOME: u32 = 0x24;
pub const VK_UP: u32 = 0x26;
pub const VK_DOWN: u32 = 0x28;
pub const VK_F5: u32 = 0x74;
pub const VK_CONTROL: i32 = 0x11;
pub const MB_OKCANCEL: u32 = 0x0001;
pub const MB_ICONQUESTION: u32 = 0x0020;
pub const MB_DEFBUTTON2: u32 = 0x0100;
pub const IDOK: i32 = 1;
/// Курсоры: стрелка и рука.
pub const IDC_ARROW: usize = 32512;
pub const IDC_HAND: usize = 32649;
/// SM_CXSMICON, SM_CXICON.
pub const SM_CXSMICON: i32 = 49;
pub const SM_CXICON: i32 = 11;

pub fn широкая(с: &str) -> Vec<u16> { с.encode_utf16().chain(std::iter::once(0)).collect() }

/// Чистая (видно в тесте): строка в поле UTF-16 фиксированной длины (с нулём в конце). Не влезает — режем по целому
/// знаку (суррогатную пару не рвём) и ставим «…».
pub fn в_поле<const N: usize>(с: &str) -> [u16; N] {
    let mut поле = [0u16; N];
    // места нет даже для «…» с нулём
    if N < 2 { return поле; }
    let единицы: Vec<u16> = с.encode_utf16().collect();
    let влезает = N - 1;
    if единицы.len() <= влезает {
        поле[..единицы.len()].copy_from_slice(&единицы);
        return поле;
    }
    let mut к = влезает - 1;
    // первая отброшенная единица — младшая половина суррогатной пары: старшую тоже отбросить
    if к > 0 && (0xDC00..=0xDFFF).contains(&единицы[к]) { к -= 1; }
    поле[..к].copy_from_slice(&единицы[..к]);
    поле[к] = '…' as u16;
    поле
}

/// Чистая (видно в тесте): FILETIME (100 нс с 1601) — миллисекунды с 1970.
pub fn filetime_мс(ft: u64) -> u64 { (ft / 10_000).saturating_sub(11_644_473_600_000) }

/// Функция системной библиотеки по имени или по номеру (`номер` — порядковый номер вывоза, тогда `имя` пусто).
fn функция(библиотека: &str, имя: &[u8], номер: Option<u16>) -> Option<*const c_void> {
    let б = широкая(библиотека);
    let м = unsafe { LoadLibraryW(б.as_ptr()) };
    if м == 0 { return None; }
    let ф = match номер {
        Some(н) => unsafe { GetProcAddress(м, н as usize as *const u8) },
        None => unsafe { GetProcAddress(м, имя.as_ptr()) },
    };
    (!ф.is_null()).then_some(ф)
}

/// Значение DWORD из реестра текущего пользователя. `None` — нет ключа или значения.
pub fn реестр_dword(путь: &str, имя: &str) -> Option<u32> {
    // HKEY_CURRENT_USER
    let hkcu: isize = 0x8000_0001u32 as i32 as isize;
    let (п, и) = (широкая(путь), широкая(имя));
    let mut значение: u32 = 0;
    let mut байт: u32 = 4;
    // RRF_RT_REG_DWORD = 0x10
    let ок = unsafe { RegGetValueW(hkcu, п.as_ptr(), и.as_ptr(), 0x10, std::ptr::null_mut(),
        &mut значение as *mut u32 as *mut c_void, &mut байт) } == 0;
    ок.then_some(значение)
}

/// Тема Windows: (приложения светлые, панель задач светлая). Нет записи — тёмная у него (13.11: обе 0).
pub fn тема_светлая() -> (bool, bool) {
    let путь = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    (реестр_dword(путь, "AppsUseLightTheme").unwrap_or(1) != 0, реестр_dword(путь, "SystemUsesLightTheme").unwrap_or(0) != 0)
}

/// Номер сборки Windows (RtlGetVersion — без обмана совместимости).
pub fn сборка_windows() -> u32 {
    type Версия = unsafe extern "system" fn(*mut [u32; 70]) -> i32;
    let Some(ф) = функция("ntdll.dll", b"RtlGetVersion\0", None) else { return 0 };
    let ф: Версия = unsafe { std::mem::transmute::<*const c_void, Версия>(ф) };
    // RTL_OSVERSIONINFOW: размер, major, minor, build, platform, CSDVersion[128]
    let mut в = [0u32; 70];
    в[0] = (5 * 4 + 128 * 2) as u32;
    if unsafe { ф(&mut в) } < 0 { return 0; }
    в[3]
}

/// Тёмное меню значка — по теме приложений Windows. uxtheme №135 (SetPreferredAppMode) и №136 (FlushMenuThemes) не
/// описаны в документации, но стоят с Windows 10 1903 (18362) — им пользуются и большие программы; раньше 18362 не зовём.
/// Ошибка здесь роняла бы только окно, не Мозг: это и есть довод держать окно отдельным процессом.
pub fn тёмное_меню(тёмное: bool) -> bool {
    if сборка_windows() < 18362 { return false; }
    type Режим = unsafe extern "system" fn(i32) -> i32;
    type Сбросить = unsafe extern "system" fn();
    let (Some(р), Some(с)) = (функция("uxtheme.dll", b"", Some(135)), функция("uxtheme.dll", b"", Some(136))) else { return false };
    let р: Режим = unsafe { std::mem::transmute::<*const c_void, Режим>(р) };
    let с: Сбросить = unsafe { std::mem::transmute::<*const c_void, Сбросить>(с) };
    // 2 — всегда тёмное, 3 — всегда светлое
    unsafe { р(if тёмное { 2 } else { 3 }); с(); }
    true
}

/// Масштаб: каждый монитор свой (версия 2). У exe нет манифеста — ставим вызовом до первого окна.
pub fn масштаб_по_монитору() -> bool { unsafe { SetProcessDpiAwarenessContext(-4) != 0 } }

/// Своему процессу — обычный приоритет: Мозг живёт «ниже обычного» (задача Планировщика), и окно, запущенное им,
/// унаследовало бы его — тормозило бы как раз тогда, когда он открыл посмотреть, «что тормозит» (vid.md §1.3).
pub fn свой_приоритет_обычный() -> bool {
    // NORMAL_PRIORITY_CLASS
    unsafe { SetPriorityClass(GetCurrentProcess(), 0x20) != 0 }
}

/// Имя программы для уведомлений Windows — «Механик» (`HKCU\Software\Classes\AppUserModelId\Mekhanik`).
pub fn имя_программы(имя: &str) -> bool {
    let и = широкая(имя);
    unsafe { SetCurrentProcessExplicitAppUserModelID(и.as_ptr()) >= 0 }
}

/// Занять мьютекс сеанса: `None` — уже занят другим (второй экземпляр). Ручку держим до конца процесса.
pub fn занять_мьютекс(имя: &str) -> Option<Ручка> {
    let и = широкая(имя);
    let h = unsafe { CreateMutexW(std::ptr::null(), 0, и.as_ptr()) };
    // ERROR_ALREADY_EXISTS = 183
    if h == 0 { return None; }
    if unsafe { GetLastError() } == 183 { unsafe { CloseHandle(h) }; return None; }
    Some(h)
}

/// Мьютекс есть — его процесс жив (Мозг держит `Local\mekhanik-odin` всю жизнь).
pub fn мьютекс_есть(имя: &str) -> bool {
    let и = широкая(имя);
    // SYNCHRONIZE
    let h = unsafe { OpenMutexW(0x0010_0000, 0, и.as_ptr()) };
    if h != 0 { unsafe { CloseHandle(h) }; }
    h != 0
}

/// 🟥 Вход в Windows этого сеанса, мс с 1970 (WTSSessionInfo.LogonTime): «остановить до входа» сравнивает с ним. Нет —
/// время загрузки ПК (тогда «до входа» — до перезагрузки).
pub fn вход_сеанса_мс() -> Option<u64> {
    let mut буфер: *mut u16 = std::ptr::null_mut();
    let mut байт: u32 = 0;
    // WTS_CURRENT_SERVER_HANDLE = 0, WTS_CURRENT_SESSION = -1, WTSSessionInfo = 24
    let ок = unsafe { WTSQuerySessionInformationW(0, 0xFFFF_FFFF, 24, &mut буфер, &mut байт) } != 0;
    let mut итог = None;
    if ок && !буфер.is_null() {
        // WTSINFOW: 8 DWORD (32 байта), имена 32 + 17 + 21 WCHAR (до 172), выравнивание — LogonTime с 200-го байта
        if байт as usize >= 216 {
            let б = unsafe { std::slice::from_raw_parts(буфер as *const u8, 216) };
            let ft = u64::from_le_bytes(б[200..208].try_into().unwrap_or([0; 8]));
            if ft > 0 { итог = Some(filetime_мс(ft)); }
        }
        unsafe { WTSFreeMemory(буфер as *mut c_void) };
    }
    итог.or_else(|| {
        let сейчас = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
        // загрузка ПК, округлённая до 10 с: два вызова подряд дают одно число
        Some((сейчас.saturating_sub(unsafe { GetTickCount64() })) / 10_000 * 10_000)
    })
}

/// Память ПК: (ОЗУ доступно, ОЗУ всего, выделено свободно, предел выделения), МБ.
pub fn память_пк() -> Option<(u64, u64, u64, u64)> {
    let mut м = ПамятьПК { длина: std::mem::size_of::<ПамятьПК>() as u32, загрузка: 0, всего_физ: 0, доступно_физ: 0,
        всего_зач: 0, доступно_зач: 0, всего_вирт: 0, доступно_вирт: 0, доступно_расш: 0 };
    if unsafe { GlobalMemoryStatusEx(&mut м) } == 0 { return None; }
    Some((м.доступно_физ >> 20, м.всего_физ >> 20, м.доступно_зач >> 20, м.всего_зач >> 20))
}

/// Время процессора всей машины: (простой, ядро, пользователь), 100 нс — разница двух замеров даёт занятость.
pub fn время_машины() -> Option<(u64, u64, u64)> {
    let (mut п, mut я, mut у) = (0u64, 0u64, 0u64);
    (unsafe { GetSystemTimes(&mut п, &mut я, &mut у) } != 0).then_some((п, я, у))
}

/// Чистая (видно в тесте): занятость процессора между двумя замерами `время_машины`, доля 0…1 (ядро включает простой).
pub fn занятость(было: (u64, u64, u64), стало: (u64, u64, u64)) -> Option<f64> {
    let простой = стало.0.checked_sub(было.0)?;
    let всего = стало.1.checked_sub(было.1)? + стало.2.checked_sub(было.2)?;
    (всего > 0).then(|| (1.0 - простой as f64 / всего as f64).clamp(0.0, 1.0))
}

/// Рабочая область монитора, где точка (обычно — где значок у часов).
pub fn рабочая_область(x: i32, y: i32) -> Option<Прям> {
    // MONITOR_DEFAULTTONEAREST = 2
    let м = unsafe { MonitorFromPoint(Точка { x, y }, 2) };
    if м == 0 { return None; }
    let mut с = СведенияМонитора { размер: std::mem::size_of::<СведенияМонитора>() as u32, монитор: Прям::default(),
        работа: Прям::default(), флаги: 0 };
    (unsafe { GetMonitorInfoW(м, &mut с) } != 0).then_some(с.работа)
}

pub fn курсор() -> (i32, i32) {
    let mut т = Точка::default();
    unsafe { GetCursorPos(&mut т) };
    (т.x, т.y)
}

/// Текст — в буфер обмена (Ctrl+V в сессию разработки).
pub fn в_буфер(окно: Ручка, текст: &str) -> bool {
    let ш = широкая(текст);
    unsafe {
        if OpenClipboard(окно) == 0 { return false; }
        EmptyClipboard();
        // GMEM_MOVEABLE
        let h = GlobalAlloc(0x0002, ш.len() * 2);
        let mut ок = false;
        if h != 0 {
            let п = GlobalLock(h) as *mut u16;
            if !п.is_null() {
                std::ptr::copy_nonoverlapping(ш.as_ptr(), п, ш.len());
                GlobalUnlock(h);
                // CF_UNICODETEXT = 13; принял — память теперь его
                ок = SetClipboardData(13, h) != 0;
            }
            if !ок { GlobalFree(h); }
        }
        CloseClipboard();
        ок
    }
}

/// Открыть папку в Проводнике (только показать; ничего не запускает).
pub fn открыть_папку(путь: &std::path::Path) -> bool {
    let (д, п) = (широкая("open"), широкая(&путь.to_string_lossy()));
    // SW_SHOWNORMAL; больше 32 — получилось
    unsafe { ShellExecuteW(0, д.as_ptr(), п.as_ptr(), std::ptr::null(), std::ptr::null(), 1) > 32 }
}

/// Цвет GDI (COLORREF: 0x00BBGGRR) из #RRGGBB.
pub const fn цвет(rgb: u32) -> u32 { ((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF) }

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn размеры_как_в_windows() {
        // ошибка в размере — Shell_NotifyIconW молча отказал бы, и значка не было бы вовсе
        assert_eq!(std::mem::size_of::<ДанныеЗначка>(), 976, "NOTIFYICONDATAW");
        assert_eq!(std::mem::size_of::<Сообщение>(), 48, "MSG");
        assert_eq!(std::mem::size_of::<КлассОкна>(), 80, "WNDCLASSEXW");
        assert_eq!(std::mem::size_of::<Рисование>(), 72, "PAINTSTRUCT");
        assert_eq!(std::mem::size_of::<СведенияЗначка>(), 32, "ICONINFO");
        assert_eq!(std::mem::size_of::<ЗаголовокРисунка>(), 40, "BITMAPINFOHEADER");
        assert_eq!(std::mem::size_of::<ПереданныеДанные>(), 24, "COPYDATASTRUCT");
        assert_eq!(std::mem::size_of::<СведенияМонитора>(), 40, "MONITORINFO");
        assert_eq!(std::mem::size_of::<ПамятьПК>(), 64, "MEMORYSTATUSEX");
    }

    #[test]
    fn строка_в_поле() {
        let п: [u16; 8] = в_поле("Механик");
        assert_eq!(String::from_utf16_lossy(&п[..7]), "Механик");
        assert_eq!(п[7], 0);
        let п: [u16; 6] = в_поле("Механик");
        assert_eq!(String::from_utf16_lossy(&п[..5]), "Меха…", "не влезло — «…» и ноль");
        assert_eq!(п[5], 0);
        // суррогатная пара (знак вне BMP) не рвётся пополам: «а😀вг» в 4 единицы — «а…», а не «а\u{D83D}…»
        let п: [u16; 4] = в_поле("а😀вг");
        let т = String::from_utf16(&п[..п.iter().position(|&с| с == 0).unwrap()]).expect("целая строка UTF-16");
        assert_eq!(т, "а…");
        let п: [u16; 1] = в_поле("что угодно");
        assert_eq!(п, [0], "места нет — пусто, но с нулём");
        let п: [u16; 2] = в_поле("что угодно");
        assert_eq!(п, ['…' as u16, 0]);
    }

    #[test]
    fn время_и_занятость() {
        // 2026-11-13 00:00:00 UTC = 1794528000000 мс; FILETIME = (мс + 11644473600000) * 10000
        assert_eq!(filetime_мс((1_794_528_000_000 + 11_644_473_600_000) * 10_000), 1_794_528_000_000);
        assert_eq!(filetime_мс(0), 0);
        assert_eq!(занятость((100, 300, 100), (150, 500, 100)), Some(0.75), "простоя 50 из 200 — занято 75 %");
        assert_eq!(занятость((100, 300, 100), (100, 300, 100)), None, "время не шло");
        assert_eq!(цвет(0x16181D), 0x1D1816);
    }

    /// Живая проба (только чтение): тема, сборка, вход сеанса, память — на этом ПК.
    #[test]
    #[ignore]
    fn живьём_сведения() {
        let (прил, панель) = тема_светлая();
        let вход = вход_сеанса_мс().expect("вход сеанса");
        let сейчас = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        eprintln!("тема: приложения светлые {прил}, панель светлая {панель}; сборка {}; вход {} мин назад; память {:?}",
            сборка_windows(), (сейчас - вход) / 60_000, память_пк());
        assert!(вход < сейчас && сейчас - вход < 40 * 24 * 3600 * 1000, "вход — в прошлом, не дальше 40 суток");
        assert_eq!(вход_сеанса_мс(), Some(вход), "одно число на двух вызовах");
        assert!(сборка_windows() >= 22000);
    }
}
