// Copyright 2019 Authors of Red Sift
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use byteorder::{NativeEndian, ReadBytesExt};
use goblin::elf::{Elf, Sym};
use std::ffi::CStr;
use std::fs;
use std::io::{self, BufRead, Cursor, Read};
use std::mem;
use std::os::raw::c_char;
use std::str;

lazy_static! {
    pub(crate) static ref LD_SO_CACHE: Result<LdSoCache, CacheError> =
        LdSoCache::load("/etc/ld.so.cache");
}

const CACHE_HEADER: &'static str = "glibc-ld.so.cache1.1";

pub(crate) struct ElfSymbols<'a> {
    elf: Elf<'a>,
}

impl<'a> ElfSymbols<'a> {
    pub fn parse(data: &[u8]) -> goblin::error::Result<ElfSymbols> {
        let elf = Elf::parse(&data)?;
        Ok(ElfSymbols { elf })
    }

    fn resolve_dyn_syms(&self, sym_name: &str) -> Option<Sym> {
        self.elf.dynsyms.iter().find(|sym| {
            self.elf
                .dynstrtab
                .get(sym.st_name)
                .and_then(|n| n.ok())
                .map(|n| n == sym_name)
                .unwrap_or(false)
        })
    }

    fn resolve_syms(&self, sym_name: &str) -> Option<Sym> {
        self.elf.syms.iter().find(|sym| {
            self.elf
                .strtab
                .get(sym.st_name)
                .and_then(|n| n.ok())
                .map(|n| n == sym_name)
                .unwrap_or(false)
        })
    }

    pub fn resolve(&self, sym_name: &str) -> Option<Sym> {
        self.resolve_dyn_syms(sym_name)
            .or_else(|| self.resolve_syms(sym_name))
    }
}

#[derive(Debug)]
pub(crate) enum CacheError {
    IOError(io::Error),
    InvalidHeader,
}

impl From<io::Error> for CacheError {
    fn from(error: io::Error) -> CacheError {
        CacheError::IOError(error)
    }
}

#[derive(Debug)]
pub(crate) struct CacheEntry {
    key: String,
    value: String,
    flags: i32,
}

#[derive(Debug)]
pub(crate) struct LdSoCache {
    entries: Vec<CacheEntry>,
}

impl LdSoCache {
    pub fn load(path: &str) -> Result<Self, CacheError> {
        let data = fs::read(path).map_err(|e| CacheError::IOError(e))?;
        Self::parse(&data)
    }

    fn parse(data: &[u8]) -> Result<Self, CacheError> {
        let mut cursor = Cursor::new(data);

        let mut buf = [0u8; CACHE_HEADER.len()];
        cursor.read(&mut buf)?;
        let header = str::from_utf8(&buf).or(Err(CacheError::InvalidHeader))?;
        if header != CACHE_HEADER {
            return Err(CacheError::InvalidHeader);
        }

        let num_entries = cursor.read_u32::<NativeEndian>()?;
        let _str_tab_len = cursor.read_u32::<NativeEndian>()?;
        cursor.consume(5 * mem::size_of::<u32>());

        let mut entries = Vec::new();
        for _ in 0..num_entries {
            let flags = cursor.read_i32::<NativeEndian>()?;
            let k_pos = cursor.read_u32::<NativeEndian>()?;
            let v_pos = cursor.read_u32::<NativeEndian>()?;
            cursor.consume(12);
            let key = unsafe {
                CStr::from_ptr(cursor.get_ref()[k_pos as usize..].as_ptr() as *const c_char)
            }
            .to_string_lossy()
            .into_owned();
            let value = unsafe {
                CStr::from_ptr(cursor.get_ref()[v_pos as usize..].as_ptr() as *const c_char)
            }
            .to_string_lossy()
            .into_owned();
            entries.push(CacheEntry { key, value, flags });
        }

        Ok(LdSoCache { entries })
    }

    pub fn resolve(&self, lib: &str) -> Option<&str> {
        let lib = if !lib.contains(".so") {
            lib.to_string() + ".so"
        } else {
            lib.to_string()
        };
        self.entries
            .iter()
            .find(|entry| {
                entry.key.starts_with(&lib)
            })
            .map(|entry| entry.value.as_str())
    }
}