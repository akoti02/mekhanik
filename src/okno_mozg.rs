//! 🟥 13.11 (свод волны 2а) — СТОРОНА МОЗГА ДЛЯ ОКНА. Пакет `vid` показывает значок и окно, а Мозг даёт им пищу и
//! исполняет их просьбы (vid §interfaces «ЧТО ДОЛЖЕН МОЗГ»):
//! 1) `znachok.json` — при смене состояния (не чаще, чем зовём: раз в такт окна 2 с);
//! 2) `sejchas.json` — раз в 2 с, пока окно живо (`okno_zhivo.json` свежее 10 с);
//! 3) просьба окна `prosba_okna.json` — пауза на час (замер идёт, лечение/уступки/заморозки/копия стоят), остановить до
//!    входа (всё вернуть и выйти), запуск при входе (задача «Mekhanik», права не нужны); ответ — `otvet_okna.json`;
//! 4) команды `komandy\*.json` — вернуть из карантина загрузку и т. п.; ответ — `otvety_okna\<id>.json`.
//!
//! Схемы — `vid::sostoyanie`. Этот модуль только пишет файлы состояния и читает просьбы: решений о лечении не принимает
//! (их принимает `storozh`), БАРС не трогает.

#![allow(dead_code)]

use serde_json::{json, Value};
use std::path::Path;

use crate::vid::sostoyanie::{self as сост, Значок, Ответ, Состояние, ЗНАЧОК, КОМАНДЫ, ОТВЕТЫ_КОМАНД};

/// Держит последнее, что писали, чтобы `znachok.json` шёл только при смене, а ответ окну — один раз на просьбу.
pub struct Публикатор {
    посл_значок: Option<Value>,
    посл_просьба: String,
    посл_входе: Option<bool>,
    номер: u64,
}

impl Публикатор {
    pub fn новый() -> Публикатор {
        Публикатор { посл_значок: None, посл_просьба: String::new(), посл_входе: None, номер: 0 }
    }

    /// `znachok.json` — записать, только если он изменился (vid: не чаще раза в 5 с; такт окна и так 2 с).
    pub fn значок(&mut self, папка: &Path, з: &Значок) {
        let мут = з.в_json();
        // сравниваем без поля времени: иначе писали бы каждый такт
        let без_времени = |v: &Value| { let mut c = v.clone(); if let Some(о) = c.as_object_mut() { о.remove("ms"); } c };
        if self.посл_значок.as_ref().map(без_времени) == Some(без_времени(&мут)) { return; }
        if сост::записать(&папка.join(ЗНАЧОК), &мут.to_string()).is_ok() { self.посл_значок = Some(мут); }
    }

    /// `sejchas.json` — записать, пока окно живо (его `okno_zhivo.json` свежее 10 с). Тело — по схеме `Сводка` v1.
    pub fn сейчас(&self, папка: &Path, тело: &Value) {
        if сост::окно_открыто(папка, сост::сейчас_мс()) {
            let _ = сост::записать(&папка.join(сост::СВОДКА), &тело.to_string());
        }
    }

    /// Исполнить просьбу окна (`prosba_okna.json`): запуск при входе (задача «Mekhanik») и ответ `otvet_okna.json` на
    /// новую просьбу. Пауза и остановку исполняет `storozh` (он держит лечение и уступки); здесь — только ответ и
    /// переключение задачи входа. Возвращает `Остановка`, если окно просит остановить до входа и вход тот же.
    pub fn просьба(&mut self, папка: &Path, вход: Option<u64>) -> Остановка {
        let п = сост::прочитать_просьбу(папка);
        let остановлен = п.остановлен(вход);
        // новая просьба (другой id) — исполнить разовое (задача входа) и ответить
        if !п.id.is_empty() && п.id != self.посл_просьба {
            self.посл_просьба = п.id.clone();
            let (при_входе, почему_входа) = self.задача_входа(п.при_входе);
            let почему = if остановлен { "останавливаю до следующего входа: всё вернул и выхожу".to_string() }
                else if п.пауза_идёт(сост::сейчас_мс()) { format!("пауза до {} — всё вернул, никого не трогаю", п.пауза_до_мс) }
                else if let Some(т) = почему_входа { т }
                else { "принял".to_string() };
            let ответ = Ответ { мс: сост::сейчас_мс(), id: п.id.clone(), ок: true, почему, пауза_до_мс: п.пауза_до_мс,
                при_входе, выходит: остановлен };
            let _ = сост::записать_ответ(папка, &ответ);
        }
        if остановлен { Остановка::ДоВхода } else { Остановка::Нет }
    }

    /// Переключить задачу входа «Mekhanik» (её ставит одна команда администратора; `schtasks /change` прав не требует).
    /// Возвращает, как задача стоит на деле (по `/query`), и строку для ответа, если меняли.
    fn задача_входа(&mut self, желаемое: Option<bool>) -> (Option<bool>, Option<String>) {
        let Some(вкл) = желаемое else { return (есть_задача_входа(), None) };
        if self.посл_входе == Some(вкл) { return (Some(вкл), None); }
        let глагол = if вкл { "/enable" } else { "/disable" };
        let итог = std::process::Command::new("schtasks").args(["/change", "/tn", "Mekhanik", глагол]).output();
        let вышло = итог.map(|в| в.status.success()).unwrap_or(false);
        if вышло { self.посл_входе = Some(вкл); }
        let как = есть_задача_входа();
        crate::svyaz::журнал("механик_задача_входа", &json!({ "просили": вкл, "вышло": вышло, "стоит": как }));
        (как, Some(if вкл { "запускать при входе" } else { "не запускать при входе" }.into()))
    }

    /// Исполнить разовые команды окна из `komandy\` (кнопки «Вернуть» и т. п.); ответить в `otvety_okna\<id>.json`.
    pub fn команды(&mut self, папка: &Path) {
        let дир = папка.join(КОМАНДЫ);
        let Ok(файлы) = std::fs::read_dir(&дир) else { return };
        for ф in файлы.filter_map(Result::ok) {
            let путь = ф.path();
            if путь.extension().is_none_or(|р| р != "json") { continue; }
            let Ok(v) = std::fs::read_to_string(&путь).and_then(|т| serde_json::from_str::<Value>(&т)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))) else { let _ = std::fs::remove_file(&путь); continue };
            let id = v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
            let что = v.get("what").and_then(Value::as_str).unwrap_or("");
            let арг = v.get("arg").and_then(Value::as_str).unwrap_or("");
            let (ок, почему) = исполнить_команду(что, арг);
            let _ = сост::записать(&папка.join(ОТВЕТЫ_КОМАНД).join(format!("{id}.json")),
                &json!({ "ok": ок, "why": почему }).to_string());
            let _ = std::fs::remove_file(&путь);
            crate::svyaz::журнал("механик_команда_окна", &json!({ "что": что, "арг": арг, "ок": ок, "почему": почему }));
        }
    }
}

/// Остановить ли Мозг по просьбе окна.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Остановка { Нет, ДоВхода }

/// Чистая (видно в тесте): пауза на час идёт — Мозг держит лечение, уступки, заморозки, приоритет разуму и копию, но
/// замер, журнал и `barsu.json` не трогает.
pub fn пауза_идёт(папка: &Path, сейчас: u64) -> bool { сост::прочитать_просьбу(папка).пауза_идёт(сейчас) }

/// Исполнить команду окна. Пути к карантину и история — у сторожа загрузок; окно шлёт только `id` записи.
fn исполнить_команду(что: &str, арг: &str) -> (bool, String) {
    match что {
        "zagruzki_vernut" => match crate::zagruzki::вернуть(арг) {
            Ok(путь) => (true, format!("вернул: {}", путь.display())),
            Err(e) => (false, format!("не вышло вернуть: {e}")),
        },
        // «в корзину» и «просмотрено» сторож загрузок пока не умеет (есть только вернуть(id)) — честный ответ
        "zagruzki_v_korzinu" => (false, "в корзину «Механик» пока не переносит — верни и удали сам".into()),
        "zagruzki_prosmotreno" => (true, "отметка «просмотрено» в этой версии не хранится".into()),
        "videokarta_snova" => (false, "повторный зажим видеокарты просит сам «Механик», когда снова заметит — отдельной кнопкой пока нельзя".into()),
        _ => (false, format!("неизвестная команда: {что}")),
    }
}

/// Задача входа «Mekhanik» включена? (`schtasks /query`; нет задачи или не узнать — `None`).
fn есть_задача_входа() -> Option<bool> {
    let в = std::process::Command::new("schtasks").args(["/query", "/tn", "Mekhanik", "/fo", "LIST"]).output().ok()?;
    if !в.status.success() { return Some(false); }
    let текст = String::from_utf8_lossy(&в.stdout).to_lowercase();
    // «готово»/«ready»/«выполняется» — включена; «отключено»/«disabled» — нет
    if текст.contains("disabled") || текст.contains("отключен") { Some(false) } else { Some(true) }
}

/// Собрать `Значок` из того, что знает Мозг. Чистая: время и счётчики — аргументами.
#[allow(clippy::too_many_arguments)]
pub fn собрать_значок(сейчас: u64, тормоз_идёт: bool, строка: String, сост_: Состояние,
                      причины: Vec<сост::Причина>, сегодня: Option<сост::Сегодня>) -> Значок {
    let _ = тормоз_идёт;
    Значок { мс: сейчас, состояние: сост_, строка, причины, сегодня, pid: std::process::id(),
        версия: env!("CARGO_PKG_VERSION").to_string() }
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn значок_только_при_смене() {
        let п = std::env::temp_dir().join(format!("mekh-okno-mozg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&п);
        std::fs::create_dir_all(&п).unwrap();
        let mut пуб = Публикатор::новый();
        let з = |сейчас: u64, с: Состояние| Значок { мс: сейчас, состояние: с, строка: "диск C: 17 ГБ".into(),
            причины: vec![], сегодня: None, pid: 1, версия: "0.6.0".into() };
        пуб.значок(&п, &з(1000, Состояние::Внимание));
        let первый = std::fs::metadata(п.join(ЗНАЧОК)).unwrap().modified().unwrap();
        // та же суть, другое время — не пишем
        пуб.значок(&п, &з(3000, Состояние::Внимание));
        assert_eq!(std::fs::metadata(п.join(ЗНАЧОК)).unwrap().modified().unwrap(), первый, "та же суть — файл не трогаем");
        // другое состояние — пишем
        пуб.значок(&п, &з(4000, Состояние::Тревога));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(п.join(ЗНАЧОК)).unwrap()).unwrap();
        assert_eq!(v.get("state").and_then(Value::as_str), Some("alarm"));
        let _ = std::fs::remove_dir_all(&п);
    }

    #[test]
    fn ответ_на_новую_просьбу_и_остановку() {
        let п = std::env::temp_dir().join(format!("mekh-okno-mozg2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&п);
        std::fs::create_dir_all(&п).unwrap();
        let mut пуб = Публикатор::новый();
        // паузы и остановки нет — ответа нет (пустой id)
        assert_eq!(пуб.просьба(&п, Some(5)), Остановка::Нет);
        assert!(!п.join(сост::ОТВЕТ).exists());
        // остановить до входа, вход тот же
        сост::записать(&п.join(сост::ПРОСЬБА), &сост::Просьба { мс: 1, id: "1-0".into(), стоп_вход_мс: 5, ..Default::default() }
            .в_json().to_string()).unwrap();
        assert_eq!(пуб.просьба(&п, Some(5)), Остановка::ДоВхода);
        let о = Ответ::из_json(&serde_json::from_str(&std::fs::read_to_string(п.join(сост::ОТВЕТ)).unwrap()).unwrap());
        assert!(о.выходит && о.ок && о.id == "1-0");
        // та же просьба второй раз — ответ не переписываем (id тот же), но остановку всё равно видим
        let было = std::fs::metadata(п.join(сост::ОТВЕТ)).unwrap().modified().unwrap();
        assert_eq!(пуб.просьба(&п, Some(5)), Остановка::ДоВхода);
        assert_eq!(std::fs::metadata(п.join(сост::ОТВЕТ)).unwrap().modified().unwrap(), было);
        // другой вход — просьба устарела
        assert_eq!(пуб.просьба(&п, Some(9)), Остановка::Нет);
        let _ = std::fs::remove_dir_all(&п);
    }

    #[test]
    fn команда_неизвестная_отвечает() {
        let (ок, _) = исполнить_команду("чепуха", "");
        assert!(!ок);
    }
}
