//! 🟥 27.10 СТЕНД «МЕХАНИКА» — своя нагрузка на ПК, чтобы проверить живьём, что «Механик» её узнаёт, называет, лечит и
//! возвращает (его слово 27.10: «если тебе нужно что-то протестировать, прогнать какой-то стенд… тормозим любые другие
//! процессы и всю мощь компьютера тратим именно на тестирование»). В программу «Механика» не входит.
//!
//!   nagruzka gpu   <секунд>             — работа на адаптере экрана (копии больших картинок Direct3D 11)
//!   nagruzka cpu   <секунд> <потоков>   — потоки, что крутятся без сна
//!   nagruzka ozu   <секунд> <ГБ>        — взять и тронуть память (ОЗУ и зачисленная)
//!   nagruzka zach  <секунд> <ГБ>        — только зачислить (предел Windows), не трогая
//!   nagruzka disk  <секунд> <файл>      — читать файл мимо кэша Windows
//! Стенд не ломает ничего: выходит сам через <секунд>, память отдаёт с выходом.
#![allow(confusable_idents, mixed_script_confusables)]

use std::ffi::c_void;
use std::time::{Duration, Instant};

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(имя: *const u16) -> isize;
    fn GetProcAddress(модуль: isize, имя: *const u8) -> *const c_void;
    fn VirtualAlloc(адрес: *mut c_void, размер: usize, тип: u32, защита: u32) -> *mut c_void;
    fn CreateFileW(имя: *const u16, доступ: u32, общий: u32, атр: *const c_void, как: u32, флаги: u32, шаблон: isize) -> isize;
    fn ReadFile(h: isize, буф: *mut c_void, байт: u32, прочтено: *mut u32, перекрытие: *mut c_void) -> i32;
    fn SetFilePointerEx(h: isize, куда: i64, новое: *mut i64, от: u32) -> i32;
    fn CloseHandle(h: isize) -> i32;
}

fn широкая(с: &str) -> Vec<u16> { с.encode_utf16().chain(std::iter::once(0)).collect() }

unsafe fn метод<T: Copy>(объект: *mut c_void, номер: usize) -> T {
    let таблица = *(объект as *const *const *const c_void);
    std::mem::transmute_copy::<*const c_void, T>(&*таблица.add(номер))
}

/// Скомпилировать шейдер HLSL (d3dcompiler_47 есть в Windows): байт-код.
fn шейдер(текст: &str, вход: &str, цель: &str) -> Vec<u8> {
    type Собрать = unsafe extern "system" fn(*const c_void, usize, *const u8, *const c_void, *const c_void, *const u8,
        *const u8, u32, u32, *mut *mut c_void, *mut *mut c_void) -> i32;
    type Указатель = unsafe extern "system" fn(*mut c_void) -> *const u8;
    type Размер = unsafe extern "system" fn(*mut c_void) -> usize;
    let б = широкая("d3dcompiler_47.dll");
    let м = unsafe { LoadLibraryW(б.as_ptr()) };
    let ф = unsafe { GetProcAddress(м, b"D3DCompile\0".as_ptr()) };
    assert!(!ф.is_null(), "нет D3DCompile");
    let собрать: Собрать = unsafe { std::mem::transmute::<*const c_void, Собрать>(ф) };
    let (вх, цл) = (format!("{вход}\0"), format!("{цель}\0"));
    let (mut код, mut ошибки) = (std::ptr::null_mut(), std::ptr::null_mut());
    let hr = unsafe { собрать(текст.as_ptr() as *const c_void, текст.len(), std::ptr::null(), std::ptr::null(),
        std::ptr::null(), вх.as_ptr(), цл.as_ptr(), 0, 0, &mut код, &mut ошибки) };
    assert!(hr >= 0 && !код.is_null(), "D3DCompile {hr:#x}");
    unsafe {
        let п = метод::<Указатель>(код, 3)(код);
        let р = метод::<Размер>(код, 4)(код);
        std::slice::from_raw_parts(п, р).to_vec()
    }
}

/// Тяжёлая отрисовка на адаптере экрана: на картинку 4096×4096 — треугольник во весь кадр с долгим циклом в каждой
/// точке; ждём каждую отрисовку (очередь не копится). Работа экрана ждёт её — как ждала 27.10 «грузил не БАРС».
fn видеокарта(секунд: u64) {
    type Создать = unsafe extern "system" fn(*mut c_void, u32, isize, u32, *const u32, u32, u32, *mut *mut c_void,
        *mut u32, *mut *mut c_void) -> i32;
    type Картинка = unsafe extern "system" fn(*mut c_void, *const [u32; 11], *const c_void, *mut *mut c_void) -> i32;
    type Вид = unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void, *mut *mut c_void) -> i32;
    type СоздатьШейдер = unsafe extern "system" fn(*mut c_void, *const u8, usize, *const c_void, *mut *mut c_void) -> i32;
    type ДатьШейдер = unsafe extern "system" fn(*mut c_void, *mut c_void, *const c_void, u32);
    type Рисовать = unsafe extern "system" fn(*mut c_void, u32, u32);
    type Топология = unsafe extern "system" fn(*mut c_void, u32);
    type Цели = unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void, *mut c_void);
    type Окно = unsafe extern "system" fn(*mut c_void, u32, *const [f32; 6]);
    type СоздатьЗапрос = unsafe extern "system" fn(*mut c_void, *const [u32; 2], *mut *mut c_void) -> i32;
    type Конец = unsafe extern "system" fn(*mut c_void, *mut c_void);
    type Данные = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut c_void, u32, u32) -> i32;
    let б = широкая("d3d11.dll");
    let м = unsafe { LoadLibraryW(б.as_ptr()) };
    let ф = unsafe { GetProcAddress(м, b"D3D11CreateDevice\0".as_ptr()) };
    assert!(!ф.is_null(), "нет D3D11CreateDevice");
    let создать: Создать = unsafe { std::mem::transmute::<*const c_void, Создать>(ф) };
    let (mut у, mut к, mut уровень) = (std::ptr::null_mut(), std::ptr::null_mut(), 0u32);
    let hr = unsafe { создать(std::ptr::null_mut(), 1, 0, 0, std::ptr::null(), 0, 7, &mut у, &mut уровень, &mut к) };
    assert!(hr >= 0, "D3D11CreateDevice {hr:#x}");
    let вш = шейдер("float4 main(uint id : SV_VertexID) : SV_Position { float2 p = float2((id << 1) & 2, id & 2); \
        return float4(p * float2(2, -2) + float2(-1, 1), 0, 1); }", "main", "vs_5_0");
    let пш = шейдер("float4 main(float4 pos : SV_Position) : SV_Target { float x = pos.x * 0.001, y = pos.y * 0.001; \
        [loop] for (int i = 0; i < 600; i++) { x = sin(x * 1.0001 + y); y = cos(y * 0.9999 + x); } return float4(x, y, 0, 1); }",
        "main", "ps_5_0");
    // 4096×4096 RGBA, D3D11_BIND_RENDER_TARGET = 0x20
    let описание: [u32; 11] = [4096, 4096, 1, 1, 28, 1, 0, 0, 0x20, 0, 0];
    let (mut карт, mut цель, mut vs, mut ps, mut запрос) =
        (std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
    unsafe {
        assert!(метод::<Картинка>(у, 5)(у, &описание, std::ptr::null(), &mut карт) >= 0, "CreateTexture2D");
        assert!(метод::<Вид>(у, 9)(у, карт, std::ptr::null(), &mut цель) >= 0, "CreateRenderTargetView");
        assert!(метод::<СоздатьШейдер>(у, 12)(у, вш.as_ptr(), вш.len(), std::ptr::null(), &mut vs) >= 0, "CreateVertexShader");
        assert!(метод::<СоздатьШейдер>(у, 15)(у, пш.as_ptr(), пш.len(), std::ptr::null(), &mut ps) >= 0, "CreatePixelShader");
        assert!(метод::<СоздатьЗапрос>(у, 24)(у, &[0, 0], &mut запрос) >= 0, "CreateQuery");
        метод::<ДатьШейдер>(к, 11)(к, vs, std::ptr::null(), 0);
        метод::<ДатьШейдер>(к, 9)(к, ps, std::ptr::null(), 0);
        метод::<Топология>(к, 24)(к, 4);
        метод::<Цели>(к, 33)(к, 1, &цель, std::ptr::null_mut());
        метод::<Окно>(к, 44)(к, 1, &[0.0, 0.0, 4096.0, 4096.0, 0.0, 1.0]);
    }
    // проба: свой класс очереди видеокарты (0 — простой … 2 — обычный), и замер — сколько ждёт крошечная работа другого
    // устройства (как у «Механика»)
    if let Some(класс) = std::env::args().nth(3).and_then(|с| с.parse::<i32>().ok()) {
        type ДатьКласс = unsafe extern "system" fn(isize, i32) -> i32;
        let г = широкая("gdi32.dll");
        let мг = unsafe { LoadLibraryW(г.as_ptr()) };
        let фг = unsafe { GetProcAddress(мг, b"D3DKMTSetProcessSchedulingPriorityClass\0".as_ptr()) };
        #[link(name = "kernel32")]
        extern "system" { fn GetCurrentProcess() -> isize; }
        let дать: ДатьКласс = unsafe { std::mem::transmute::<*const c_void, ДатьКласс>(фг) };
        println!("свой класс очереди видеокарты {класс}: {}", unsafe { дать(GetCurrentProcess(), класс) });
    }
    let адрес = ф as usize;
    std::thread::spawn(move || {
        let (mut у2, mut к2, mut ур2) = (std::ptr::null_mut(), std::ptr::null_mut(), 0u32);
        let создать: Создать = unsafe { std::mem::transmute::<usize, Создать>(адрес) };
        if unsafe { создать(std::ptr::null_mut(), 1, 0, 0, std::ptr::null(), 0, 7, &mut у2, &mut ур2, &mut к2) } < 0 { return; }
        let mut з2 = std::ptr::null_mut();
        unsafe { метод::<СоздатьЗапрос>(у2, 24)(у2, &[0, 0], &mut з2) };
        let mut ждали: Vec<f64> = Vec::new();
        let конец2 = Instant::now() + Duration::from_secs(секунд.saturating_sub(1));
        while Instant::now() < конец2 {
            let с = Instant::now();
            unsafe {
                метод::<Конец>(к2, 28)(к2, з2);
                let mut готово = 0i32;
                while метод::<Данные>(к2, 29)(к2, з2, &mut готово as *mut i32 as *mut c_void, 4, 0) == 1 {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            ждали.push(с.elapsed().as_secs_f64() * 1000.0);
            std::thread::sleep(Duration::from_millis(250));
        }
        ждали.sort_by(f64::total_cmp);
        if !ждали.is_empty() {
            println!("чужая крошечная работа ждала: средина {:.0} мс, наибольшее {:.0} мс ({} замеров)",
                ждали[ждали.len() / 2], ждали[ждали.len() - 1], ждали.len());
        }
    });
    let конец = Instant::now() + Duration::from_secs(секунд);
    let (mut кадров, mut мс_всего) = (0u64, 0f64);
    while Instant::now() < конец {
        let с = Instant::now();
        unsafe {
            метод::<Рисовать>(к, 13)(к, 3, 0);
            метод::<Конец>(к, 28)(к, запрос);
            let mut готово = 0i32;
            while метод::<Данные>(к, 29)(к, запрос, &mut готово as *mut i32 as *mut c_void, 4, 0) == 1 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        кадров += 1;
        мс_всего += с.elapsed().as_secs_f64() * 1000.0;
    }
    println!("видеокарта: {кадров} отрисовок за {секунд} с, в среднем {:.0} мс", мс_всего / кадров.max(1) as f64);
}

fn процессор(секунд: u64, потоков: usize) {
    let конец = Instant::now() + Duration::from_secs(секунд);
    let нити: Vec<_> = (0..потоков).map(|_| std::thread::spawn(move || {
        let mut х = 1u64;
        while Instant::now() < конец { for _ in 0..100_000 { х = х.wrapping_mul(6364136223846793005).wrapping_add(1); } }
        х
    })).collect();
    let с: u64 = нити.into_iter().map(|н| н.join().unwrap_or(0)).fold(0, u64::wrapping_add);
    println!("процессор: {потоков} потоков {секунд} с ({})", с % 10);
}

fn память(секунд: u64, гб: u64, трогать: bool) {
    // MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE
    let кусок = 256usize << 20;
    let mut взято = 0u64;
    let mut куски = Vec::new();
    for _ in 0..(гб * 4) {
        let п = unsafe { VirtualAlloc(std::ptr::null_mut(), кусок, 0x1000 | 0x2000, 0x04) };
        if п.is_null() { println!("память: Windows отказала после {} МБ", взято >> 20); break; }
        if трогать {
            let с = unsafe { std::slice::from_raw_parts_mut(п as *mut u8, кусок) };
            for i in (0..кусок).step_by(4096) { с[i] = 1; }
        }
        взято += кусок as u64;
        куски.push(п);
    }
    println!("память: {} МБ {}", взято >> 20, if трогать { "взято и тронуто" } else { "зачислено" });
    std::thread::sleep(Duration::from_secs(секунд));
}

fn диск(секунд: u64, путь: &str) {
    // GENERIC_READ, FILE_SHARE_READ|WRITE, OPEN_EXISTING, FILE_FLAG_NO_BUFFERING | FILE_FLAG_SEQUENTIAL_SCAN
    let и = широкая(путь);
    let h = unsafe { CreateFileW(и.as_ptr(), 0x8000_0000, 0x3, std::ptr::null(), 3, 0x2000_0000 | 0x0800_0000, 0) };
    assert!(h != -1 && h != 0, "файл не открылся: {путь}");
    let размер = 4usize << 20;
    let буф = unsafe { VirtualAlloc(std::ptr::null_mut(), размер, 0x1000 | 0x2000, 0x04) };
    let конец = Instant::now() + Duration::from_secs(секунд);
    let mut всего = 0u64;
    while Instant::now() < конец {
        let mut прочтено = 0u32;
        let ок = unsafe { ReadFile(h, буф, размер as u32, &mut прочтено, std::ptr::null_mut()) } != 0;
        if !ок || прочтено == 0 { unsafe { SetFilePointerEx(h, 0, std::ptr::null_mut(), 0) }; continue; }
        всего += прочтено as u64;
    }
    unsafe { CloseHandle(h) };
    println!("диск: {} МБ за {секунд} с", всего >> 20);
}

fn main() {
    let а: Vec<String> = std::env::args().collect();
    let секунд: u64 = а.get(2).and_then(|с| с.parse().ok()).unwrap_or(30);
    match а.get(1).map(String::as_str) {
        Some("gpu") => видеокарта(секунд),
        Some("cpu") => процессор(секунд, а.get(3).and_then(|с| с.parse().ok()).unwrap_or(16)),
        Some("ozu") => память(секунд, а.get(3).and_then(|с| с.parse().ok()).unwrap_or(8), true),
        Some("zach") => память(секунд, а.get(3).and_then(|с| с.parse().ok()).unwrap_or(8), false),
        Some("disk") => диск(секунд, а.get(3).map(String::as_str).unwrap_or("")),
        _ => println!("nagruzka gpu|cpu|ozu|zach|disk <секунд> [потоков|ГБ|файл]"),
    }
}
