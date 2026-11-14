//! За какими папками следим и что НИКОГДА не трогаем. Папки его разработки (`C:\dev`, `target`, `node_modules`,
//! `.git`…) сторож обходит стороной — там рабочие файлы, не загрузки (решение Артура: ПК не ломать, работе не мешать).

use std::ffi::c_void;
use std::path::{Path, PathBuf};

#[repr(C)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[link(name = "shell32")]
extern "system" {
    fn SHGetKnownFolderPath(ид: *const Guid, флаги: u32, токен: isize, путь: *mut *mut u16) -> i32;
}
#[link(name = "ole32")]
extern "system" {
    fn CoTaskMemFree(п: *mut c_void);
}

/// Известная папка по её GUID.
fn известная_папка(ид: Guid) -> Option<PathBuf> {
    let mut указатель: *mut u16 = std::ptr::null_mut();
    if unsafe { SHGetKnownFolderPath(&ид, 0, 0, &mut указатель) } != 0 || указатель.is_null() {
        return None;
    }
    let mut длина = 0usize;
    while unsafe { *указатель.add(длина) } != 0 {
        длина += 1;
    }
    let срез = unsafe { std::slice::from_raw_parts(указатель, длина) };
    let строка = String::from_utf16_lossy(срез);
    unsafe { CoTaskMemFree(указатель as *mut c_void) };
    Some(PathBuf::from(строка))
}

/// Папка «Загрузки» (FOLDERID_Downloads).
pub fn загрузки() -> Option<PathBuf> {
    известная_папка(Guid {
        d1: 0x374D_E290,
        d2: 0x123F,
        d3: 0x4565,
        d4: [0x91, 0x64, 0x39, 0xC4, 0x92, 0x5E, 0x46, 0x7B],
    })
}

/// Папки, которые сторожим целиком (все файлы). Пока — «Загрузки»; браузерные папки загрузок добавит сведение
/// из их настроек (в отчёте §7.2). Нет «Загрузок» — пусто.
pub fn целиком() -> Vec<PathBuf> {
    загрузки().into_iter().collect()
}

/// Части пути, которые означают папку разработки/сборки — внутрь не лезем вовсе.
/// 🟥 Без голых `bin`/`obj`: они исключали любую папку с таким именем, и вредонос в `Downloads\App\bin\app.exe`
/// не проверялся ни своей проверкой, ни Защитником. Дерево сборки под `C:\dev` и так вне проверки; `target`,
/// `node_modules`, `.git` остаются по именам.
const ЗАПРЕТНЫЕ_ЧАСТИ: &[&str] = &[
    "node_modules", ".git", "target", "__pycache__", ".venv", "venv", ".gradle", ".idea", ".vs", ".rproj.user",
    ".cargo", ".cache",
];

/// НЕ трогать: всё под `C:\dev`, а также любой путь с запретной частью (папки сборок и окружений).
/// Чистая (видно в тесте).
pub fn исключён(путь: &Path) -> bool {
    let низ = путь.to_string_lossy().to_lowercase().replace('/', "\\");
    // его дерево разработки
    if низ == "c:\\dev" || низ.starts_with("c:\\dev\\") {
        return true;
    }
    for часть in низ.split('\\') {
        if ЗАПРЕТНЫЕ_ЧАСТИ.contains(&часть) {
            return true;
        }
    }
    false
}

/// Признаки папки-проекта: подпапка «Загрузок» с таким файлом — «вид В» (его работа, не уведомляем, кроме угроз).
const ПРИЗНАКИ_ПРОЕКТА: &[&str] =
    &[".git", "cargo.toml", "package.json", "pyproject.toml", "renv.lock", "go.mod", "pom.xml"];

/// Есть ли в папке признак проекта (по списку имён в ней).
pub fn похоже_на_проект(имена_в_папке: &[String]) -> bool {
    имена_в_папке.iter().any(|и| {
        let н = и.to_lowercase();
        ПРИЗНАКИ_ПРОЕКТА.contains(&н.as_str()) || н.ends_with(".rproj")
    })
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn исключены_папки_разработки() {
        assert!(исключён(Path::new(r"C:\dev\overseer\app")));
        assert!(исключён(Path::new(r"C:\dev")));
        assert!(исключён(Path::new(r"C:\Users\akoti\Downloads\proj\node_modules\x\pkg.tgz")));
        assert!(исключён(Path::new(r"C:\Users\akoti\Downloads\proj\.git\objects\ab")));
        assert!(исключён(Path::new(r"C:\Users\akoti\Downloads\rust\target\debug\app.exe")));
    }

    #[test]
    fn обычная_загрузка_не_исключена() {
        assert!(!исключён(Path::new(r"C:\Users\akoti\Downloads\лекция.pdf")));
        assert!(!исключён(Path::new(r"C:\Users\akoti\Downloads\Week7_lab.zip")));
        // «development» в имени файла — не папка разработки
        assert!(!исключён(Path::new(r"C:\Users\akoti\Downloads\development-guide.pdf")));
    }

    /// 🟥 Красно на старом: вредонос в `Downloads\...\bin\` (или `obj\`) должен проверяться — старое слепо исключало
    /// любые bin/obj. Настоящие деревья сборки под `C:\dev` и `target`/`node_modules` остаются вне проверки.
    #[test]
    fn bin_obj_в_загрузках_проверяются() {
        assert!(!исключён(Path::new(r"C:\Users\akoti\Downloads\App\bin\app.exe")));
        assert!(!исключён(Path::new(r"C:\Users\akoti\Downloads\App\obj\x.dll")));
        assert!(исключён(Path::new(r"C:\dev\proj\bin\app.exe")), "сборка под C:\\dev — по-прежнему вне проверки");
        assert!(исключён(Path::new(r"C:\Users\akoti\Downloads\rust\target\debug\app.exe")), "target остаётся исключён");
    }

    #[test]
    fn проект_по_признакам() {
        assert!(похоже_на_проект(&["Cargo.toml".into(), "src".into()]));
        assert!(похоже_на_проект(&["task.Rproj".into()]));
        assert!(!похоже_на_проект(&["лекция.pdf".into(), "данные.csv".into()]));
    }
}
