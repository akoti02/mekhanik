//! «МЕХАНИК» — библиотека пакета (роли и голова живут в `main.rs`). Выделена отдельно от двоичного файла, чтобы стенды
//! из `examples` (например, стенд цены `cena`) могли мерить те же куски, что крутит Мозг, не повторяя их заново. На сам
//! установленный «Механик» это не влияет: тот же код, только из общей библиотеки, двоичный файл по-прежнему в `main.rs`.
// имена по-русски, как в БАРСе: «р» и «p», «с» и «c» рядом — не путаница
#![allow(confusable_idents, mixed_script_confusables)]

pub mod arbitr;
pub mod kopiya;
pub mod kto;
pub mod lechenie;
pub mod obnovlenie;
pub mod okna;
pub mod okno_mozg;
pub mod predvidenie;
pub mod yashchik;
pub mod sobytiya;
pub mod sobytiya_win;
pub mod storozh;
pub mod svyaz; // внутри себя подключает zhurnal и kartina_barsa через #[path] — отдельно их не объявлять (один поток записи)
pub mod vid;
pub mod videokarta;
pub mod vnimanie;
pub mod zagruzki;
pub mod zhizn;
