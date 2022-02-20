use crate::error::Error;

use byteorder::{BigEndian, ReadBytesExt};

use std::io::{Read, Seek, SeekFrom};

pub trait Io {
    type Reader: Read + Seek;

    fn load<S: AsRef<str>>(&self, name: S) -> Result<Self::Reader, Error>;

    fn entry(&self, entry: &MemEntry) -> Result<Vec<u8>, Error> {
        let mut reader = self.load(entry.bank_id.name())?;
        reader.seek(SeekFrom::Start(entry.bank_offset as u64))?;
        let mut buf = vec![0; entry.packed_size as usize];
        reader.read_exact(&mut buf)?;

        if entry.packed_size == entry.size {
            Ok(buf)
        } else {
            let decoder = Decoder::new(entry, buf);
            decoder.decode()
        }
    }
}

struct Decoder {
    crc: u32,
    check: u32,
    data_size: i32,
    size: u16,
    output: Vec<u8>,
    output_cursor: usize,
    input: Vec<u8>,
    input_cursor: usize,
}

impl Decoder {
    fn new(entry: &MemEntry, input: Vec<u8>) -> Self {
        Self {
            crc: 0,
            check: 0,
            data_size: 0,
            size: 0,
            output: vec![0; entry.size as usize],
            output_cursor: entry.size as usize - 1,
            input,
            input_cursor: entry.packed_size as usize,
        }
    }

    fn decode(mut self) -> Result<Vec<u8>, Error> {
        self.data_size = self.read_rev_u32()? as i32;
        self.crc = self.read_rev_u32()?;
        self.check = self.read_rev_u32()?;

        self.crc ^= self.check;

        loop {
            if !self.next_chunk()? {
                self.size = 1;

                if !self.next_chunk()? {
                    self.dec_unk1(3, 0)?;
                } else {
                    self.dec_unk2(8)?;
                }
            } else {
                let c = self.get_code(2)?;
                if c == 3 {
                    self.dec_unk1(8, 8)?;
                } else {
                    if c < 2 {
                        self.size = c + 2;
                        self.dec_unk2(c as u8 + 9)?;
                    } else {
                        self.size = self.get_code(8)?;
                        self.dec_unk2(12)?;
                    }
                }
            }

            if self.data_size <= 0 {
                break;
            }
        }

        if self.crc != 0 {
            return Err(Error::CrcCheckFailed);
        }

        Ok(self.output)
    }

    fn next_chunk(&mut self) -> Result<bool, Error> {
        let mut cf = self.rcr(false);

        if self.check == 0 {
            self.check = self.read_rev_u32()?;
            self.crc ^= self.check;
            cf = self.rcr(true);
        }

        Ok(cf)
    }

    fn get_code(&mut self, num_chunks: u8) -> Result<u16, Error> {
        let mut c = 0;

        for _ in 0..num_chunks {
            c <<= 1;

            if self.next_chunk()? {
                c |= 1;
            }
        }

        Ok(c)
    }

    fn dec_unk1(&mut self, num_chunks: u8, add_count: u8) -> Result<(), Error> {
        let count = self.get_code(num_chunks)? + add_count as u16 + 1;
        self.data_size -= count as i32;
        for _ in 0..count {
            let value = self.get_code(8)?;
            let out = self
                .output
                .get_mut(self.output_cursor)
                .expect("write within buffer");
            *out = value as u8;
            self.output_cursor = self.output_cursor.wrapping_sub(1);
        }
        Ok(())
    }

    fn dec_unk2(&mut self, num_chunks: u8) -> Result<(), Error> {
        let i = self.get_code(num_chunks)?;
        let count = self.size + 1;
        self.data_size -= count as i32;
        for _ in 0..count {
            let value = *self
                .output
                .get(self.output_cursor + i as usize)
                .expect("read within buffer");
            let out = self
                .output
                .get_mut(self.output_cursor)
                .expect("write within buffer");
            *out = value;
            self.output_cursor = self.output_cursor.wrapping_sub(1);
        }
        Ok(())
    }

    fn rcr(&mut self, cf: bool) -> bool {
        let rcf = (self.check & 1) != 0;
        self.check >>= 1;
        if cf {
            self.check |= 0x80000000;
        }

        rcf
    }

    fn read_rev_u32(&mut self) -> Result<u32, Error> {
        if self.input_cursor < 4 {
            return Err(Error::InputBufferDrained);
        }

        self.input_cursor -= 4;
        let bytes = &self
            .input
            .get(self.input_cursor..self.input_cursor + 4)
            .ok_or(Error::InputBufferDrained)?;

        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }
}

pub struct Resources<T: Io> {
    io: T,
    loaded_part: Option<GamePart>,
    entries: Vec<MemEntry>,
    requested_part: Option<GamePart>,
}

impl<T: Io> Resources<T> {
    pub fn load(io: T) -> Result<Self, Error> {
        let mut mem_list = std::io::BufReader::new(io.load("MEMLIST.BIN")?);
        let mut entries = Vec::new();
        while let Some(entry) = MemEntry::next(&mut mem_list)? {
            entries.push(entry);
        }
        eprintln!("found entries: {}", entries.len());

        Ok(Resources {
            io,
            loaded_part: None,
            entries,
            requested_part: None,
        })
    }

    pub fn prepare_part(&mut self, part: GamePart) {
        if self.loaded_part == Some(part) {
            return;
        }

        self.unload();

        self.request_part(part);

        self.load_requested();
        self.loaded_part = Some(part);
    }

    fn unload(&mut self) {
        for entry in self.entries.iter_mut() {
            entry.state = MemEntryState::NotNeeded;
        }
        self.loaded_part = None;
    }

    pub fn requested_part(&mut self) -> Option<GamePart> {
        self.requested_part.take()
    }

    fn request_part(&mut self, part: GamePart) {
        if let Some(entry) = self.entries.get_mut(part.palette()) {
            entry.state = MemEntryState::Requested;
        }

        if let Some(entry) = self.entries.get_mut(part.bytecode()) {
            entry.state = MemEntryState::Requested;
        }

        if let Some(entry) = self.entries.get_mut(part.cinematic()) {
            entry.state = MemEntryState::Requested;
        }

        if let Some(entry) = part.alt_video().and_then(|idx| self.entries.get_mut(idx)) {
            entry.state = MemEntryState::Requested;
        }
    }

    fn load_requested(&mut self) {
        for entry in self.entries.iter_mut() {
            if let MemEntryState::Requested = entry.state {
                match self.io.entry(entry) {
                    Ok(data) => {
                        entry.state = MemEntryState::Loaded(data);
                    }
                    Err(err) => {
                        eprintln!("unable to load resource: {:?} {:?}", err, entry);
                        entry.state = MemEntryState::NotNeeded;
                    }
                }
            }
        }
    }

    pub fn load_part_or_entry(&mut self, resource_id: u16) {
        if resource_id as usize > self.entries.len() {
            self.requested_part = GamePart::from(resource_id);
        } else {
            if let Some(entry) = self.entries.get_mut(resource_id as usize) {
                if let MemEntryState::NotNeeded = entry.state {
                    entry.state = MemEntryState::Requested;
                    self.load_requested();
                }
            }
        }
    }

    pub fn palette(&self) -> Option<&[u8]> {
        self.segment(|s| Some(s.palette()))
    }

    pub fn bytecode(&self) -> Option<&[u8]> {
        self.segment(|s| Some(s.bytecode()))
    }

    pub fn cinematic(&self) -> Option<&[u8]> {
        self.segment(|s| Some(s.cinematic()))
    }

    pub fn alt_video(&self) -> Option<&[u8]> {
        self.segment(GamePart::alt_video)
    }

    fn segment<F: Fn(&GamePart) -> Option<usize>>(&self, f: F) -> Option<&[u8]> {
        self.loaded_part
            .and_then(|p| f(&p))
            .and_then(|s| self.entries.get(s))
            .and_then(|e| match e.state {
                MemEntryState::Loaded(ref data) => Some(data.as_slice()),
                _ => None,
            })
    }
}

#[derive(Debug, Clone)]
pub struct MemEntry {
    state: MemEntryState,
    kind: ResourceType,
    bank_id: BankId,
    bank_offset: u32,
    packed_size: u16,
    size: u16,
}

impl MemEntry {
    fn next<R: Read>(mut reader: R) -> Result<Option<Self>, Error> {
        let state = reader.read_u8()?;
        if state == 255 {
            return Ok(None);
        }

        let state = state.try_into()?;
        let kind = reader.read_u8()?.into();
        let _buf_ptr = reader.read_u16::<BigEndian>()?;
        let _unknown_a = reader.read_u16::<BigEndian>()?;
        let _rank_number = reader.read_u8()?;
        let bank_id = reader.read_u8()?.try_into()?;
        let bank_offset = reader.read_u32::<BigEndian>()?;
        let _unknown_b = reader.read_u16::<BigEndian>()?;
        let packed_size = reader.read_u16::<BigEndian>()?;
        let _unknown_c = reader.read_u16::<BigEndian>()?;
        let size = reader.read_u16::<BigEndian>()?;

        Ok(Some(MemEntry {
            state,
            kind,
            bank_id,
            bank_offset,
            packed_size,
            size,
        }))
    }
}

#[derive(Debug, Clone)]
enum MemEntryState {
    NotNeeded,
    Loaded(Vec<u8>),
    Requested,
}

impl TryFrom<u8> for MemEntryState {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let value = match value {
            0 => Self::NotNeeded,
            1 => Self::Loaded(vec![]),
            2 => Self::Requested,
            _ => return Err(Error::InvalidMemEntryState(value)),
        };

        Ok(value)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ResourceType {
    Sound,
    Music,
    PolygonAnimation,
    Palette,
    Bytecode,
    PolygonCinematic,
    Unknown,
}

impl From<u8> for ResourceType {
    fn from(value: u8) -> Self {
        match value {
            0 => ResourceType::Sound,
            1 => ResourceType::Music,
            2 => ResourceType::PolygonAnimation,
            3 => ResourceType::Palette,
            4 => ResourceType::Bytecode,
            5 => ResourceType::PolygonCinematic,
            _ => ResourceType::Unknown,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GamePart {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
}

impl GamePart {
    pub fn from(id: u16) -> Option<Self> {
        let part = match id {
            0x3e80 => GamePart::One,
            0x3e81 => GamePart::Two,
            0x3e82 => GamePart::Three,
            0x3e83 => GamePart::Four,
            0x3e84 => GamePart::Five,
            0x3e85 => GamePart::Six,
            0x3e86 => GamePart::Seven,
            0x3e87 => GamePart::Eight,
            0x3e88 => GamePart::Nine,
            0x3e89 => GamePart::Ten,
            _ => return None,
        };

        Some(part)
    }

    pub const fn palette(&self) -> usize {
        match self {
            GamePart::One => 0x14,
            GamePart::Two => 0x17,
            GamePart::Three => 0x1a,
            GamePart::Four => 0x1d,
            GamePart::Five => 0x20,
            GamePart::Six => 0x23,
            GamePart::Seven => 0x26,
            GamePart::Eight => 0x29,
            GamePart::Nine => 0x7d,
            GamePart::Ten => 0x7d,
        }
    }

    pub const fn bytecode(&self) -> usize {
        match self {
            GamePart::One => 0x15,
            GamePart::Two => 0x18,
            GamePart::Three => 0x1b,
            GamePart::Four => 0x1e,
            GamePart::Five => 0x21,
            GamePart::Six => 0x24,
            GamePart::Seven => 0x27,
            GamePart::Eight => 0x2a,
            GamePart::Nine => 0x7e,
            GamePart::Ten => 0x7e,
        }
    }

    pub const fn cinematic(&self) -> usize {
        match self {
            GamePart::One => 0x16,
            GamePart::Two => 0x19,
            GamePart::Three => 0x1c,
            GamePart::Four => 0x1f,
            GamePart::Five => 0x22,
            GamePart::Six => 0x25,
            GamePart::Seven => 0x28,
            GamePart::Eight => 0x2b,
            GamePart::Nine => 0x7f,
            GamePart::Ten => 0x7f,
        }
    }

    pub const fn alt_video(&self) -> Option<usize> {
        match self {
            GamePart::One => None,
            GamePart::Two => None,
            GamePart::Three => Some(0x11),
            GamePart::Four => Some(0x11),
            GamePart::Five => Some(0x11),
            GamePart::Six => None,
            GamePart::Seven => Some(0x11),
            GamePart::Eight => Some(0x11),
            GamePart::Nine => None,
            GamePart::Ten => None,
        }
    }
}

#[derive(Debug, Clone)]
struct BankId(u8);

impl BankId {
    fn name(&self) -> &'static str {
        match self.0 {
            1 => "BANK01",
            2 => "BANK02",
            3 => "BANK03",
            4 => "BANK04",
            5 => "BANK05",
            6 => "BANK06",
            7 => "BANK07",
            8 => "BANK08",
            9 => "BANK09",
            0xa => "BANK0A",
            0xb => "BANK0B",
            0xc => "BANK0C",
            0xd => "BANK0D",
            _ => unreachable!("invalid bank id: {}", self.0),
        }
    }
}

impl TryFrom<u8> for BankId {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 0 && value <= 0x0d {
            Ok(BankId(value))
        } else {
            Err(Error::InvalidBankId(value))
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum PolygonSource {
    Cinematic,
    AltVideo,
}

#[derive(Debug, Copy, Clone)]
pub struct PolygonResource {
    pub buffer_offset: usize,
    pub source: PolygonSource,
}
