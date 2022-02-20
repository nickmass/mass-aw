use engine::error::Error;
use engine::Io;

const MEMLIST: &'static [u8] = include_bytes!("../../games/ootw_2/MEMLIST.BIN");
const BANK01: &'static [u8] = include_bytes!("../../games/ootw_2/BANK01");
const BANK02: &'static [u8] = include_bytes!("../../games/ootw_2/BANK02");
const BANK03: &'static [u8] = include_bytes!("../../games/ootw_2/BANK03");
const BANK04: &'static [u8] = include_bytes!("../../games/ootw_2/BANK04");
const BANK05: &'static [u8] = include_bytes!("../../games/ootw_2/BANK05");
const BANK06: &'static [u8] = include_bytes!("../../games/ootw_2/BANK06");
const BANK07: &'static [u8] = include_bytes!("../../games/ootw_2/BANK07");
const BANK08: &'static [u8] = include_bytes!("../../games/ootw_2/BANK08");
const BANK09: &'static [u8] = include_bytes!("../../games/ootw_2/BANK09");
const BANK0A: &'static [u8] = include_bytes!("../../games/ootw_2/BANK0A");
const BANK0B: &'static [u8] = include_bytes!("../../games/ootw_2/BANK0B");
const BANK0C: &'static [u8] = include_bytes!("../../games/ootw_2/BANK0C");
const BANK0D: &'static [u8] = include_bytes!("../../games/ootw_2/BANK0D");

pub struct EmbeddedResources;

impl Io for EmbeddedResources {
    type Reader = std::io::Cursor<&'static [u8]>;
    fn load<S: AsRef<str>>(&self, file: S) -> Result<Self::Reader, Error> {
        let bytes = match file.as_ref() {
            "MEMLIST.BIN" => MEMLIST,
            "BANK01" => BANK01,
            "BANK02" => BANK02,
            "BANK03" => BANK03,
            "BANK04" => BANK04,
            "BANK05" => BANK05,
            "BANK06" => BANK06,
            "BANK07" => BANK07,
            "BANK08" => BANK08,
            "BANK09" => BANK09,
            "BANK0A" => BANK0A,
            "BANK0B" => BANK0B,
            "BANK0C" => BANK0C,
            "BANK0D" => BANK0D,
            _ => panic!(),
        };

        Ok(std::io::Cursor::new(bytes))
    }
}
