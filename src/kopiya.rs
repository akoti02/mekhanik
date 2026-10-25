//! 🟥 25.10 (Р-193): КОПИЯ ПК НА BOX НЕ ОТНИМАЕТ ЕГО ВИДЕО. Замер 25.10 ≈17:00: YouTube буферизовал, загрузка
//! 5,9–17 Мбит/с, а копия ПК (`scripts/kopiya-pk.ps1`, днём `--bwlimit 1M`) отдавала ≈1,07 МБ/с; копию придержали — 16,8.
//! Пока играет видео (датчик декодера БАРСа), скорость копии — `ПРИ_ВИДЕО` (rclone слушает пульт на `ПУЛЬТ`, флаг
//! `--rc` в скрипте копии); видео кончилось — как в расписании скрипта: 08:00–23:30 — 1M, ночью без предела. Копия
//! не останавливается и ничего не теряет — только медленнее, пока он смотрит.

use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::svyaz::журнал_барсу;

const ПУЛЬТ: &str = "127.0.0.1:5573";
const ПРИ_ВИДЕО: &str = "128k";
/// Пока придержана — подтверждать раз в столько (rclone мог начать новую папку или новый прогон).
const ПОДТВЕРЖДАТЬ: Duration = Duration::from_secs(60);

/// Чистая (видно в тесте): скорость копии по расписанию скрипта (`--bwlimit '08:00,1M 23:30,off'`).
pub fn по_расписанию(час: u32, минута: u32) -> &'static str {
    let м = час * 60 + минута;
    if (8 * 60..23 * 60 + 30).contains(&м) { "1M" } else { "off" }
}

fn пульт(скорость: &str) -> Result<(), String> {
    let адрес = ПУЛЬТ.parse().map_err(|e| format!("{e}"))?;
    let mut с = TcpStream::connect_timeout(&адрес, Duration::from_millis(300)).map_err(|e| format!("{e}"))?;
    let _ = с.set_read_timeout(Some(Duration::from_secs(3)));
    let тело = json!({ "rate": скорость }).to_string();
    write!(с, "POST /core/bwlimit HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
        Connection: close\r\n\r\n{}", тело.len(), тело).map_err(|e| format!("{e}"))?;
    let mut ответ = String::new();
    let _ = с.read_to_string(&mut ответ);
    if ответ.starts_with("HTTP/1.1 200") || ответ.starts_with("HTTP/1.0 200") { Ok(()) }
    else { Err(ответ.lines().next().unwrap_or("нет ответа").to_string()) }
}

#[derive(Default)]
pub struct Копия { придержана: bool, когда: Option<Instant> }

impl Копия {
    /// Шаг раз в 10 с: `видео` — играет ли видео по датчику БАРСа.
    pub fn шаг(&mut self, видео: bool) {
        if видео {
            if self.придержана && self.когда.is_some_and(|к| к.elapsed() < ПОДТВЕРЖДАТЬ) { return; }
            // копия не идёт — пульта нет: молчим, спросим снова через минуту
            let было = self.придержана;
            self.когда = Some(Instant::now());
            if пульт(ПРИ_ВИДЕО).is_ok() {
                self.придержана = true;
                if !было {
                    журнал_барсу("механик_копия_придержана", &json!({ "скорость": ПРИ_ВИДЕО,
                        "почему": "играет видео — копия ПК на Box не отнимает у него интернет" }));
                }
            }
        } else if self.придержана {
            let т = chrono::Local::now();
            use chrono::Timelike;
            let скорость = по_расписанию(т.hour(), т.minute());
            // копия кончилась, пока он смотрел, — отпускать некого
            let _ = пульт(скорость);
            self.придержана = false;
            self.когда = None;
            журнал_барсу("механик_копия_отпущена", &json!({ "скорость": скорость, "почему": "видео кончилось" }));
        }
    }
}

#[cfg(test)]
mod тесты {
    use super::*;

    #[test]
    fn расписание_копии() {
        assert_eq!(по_расписанию(7, 59), "off");
        assert_eq!(по_расписанию(8, 0), "1M");
        assert_eq!(по_расписанию(23, 29), "1M");
        assert_eq!(по_расписанию(23, 30), "off");
        assert_eq!(по_расписанию(2, 0), "off");
    }
}
