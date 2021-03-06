/*!

Binding for the iconv library

[![Build Status](https://drone.io/github.com/andelf/rust-iconv/status.png)](https://drone.io/github.com/andelf/rust-iconv/latest)

 */

#![desc = "iconv bindings for Rust."]
#![license = "MIT"]

#![crate_name = "iconv"]
#![crate_type = "lib"]
#![doc(html_logo_url = "http://www.rust-lang.org/logos/rust-logo-128x128-blk-v2.png",
       html_favicon_url = "http://www.rust-lang.org/favicon.ico",
       html_root_url = "http://static.rust-lang.org/doc/master")]

#![feature(globs,phase)]

#[phase(plugin, link)] extern crate log;
extern crate libc;

use std::mem;
use std::vec::Vec;
use std::io;
use std::io::{IoResult, IoError};
use std::os::errno;
use libc::consts::os::posix88::{E2BIG, EILSEQ, EINVAL};
use std::ptr;
use std::str;
// for copy_from
use std::slice::MutableCloneableSlice;

/* automatically generated by rust-bindgen */
/* and then manually modified :P */

use libc::{c_char, size_t, c_int, c_void};
#[allow(non_camel_case_types)]
type iconv_t = *mut c_void;

#[cfg(target_os = "macos")]
#[link(name = "iconv")]
extern {}

// iconv is part of linux glibc
#[cfg(target_os = "linux")]
extern {}

extern "C" {
    fn iconv_open(__tocode: *const c_char, __fromcode: *const c_char) -> iconv_t;
    fn iconv(__cd: iconv_t, __inbuf: *mut *mut c_char,
                 __inbytesleft: *mut size_t, __outbuf: *mut *mut c_char,
                 __outbytesleft: *mut size_t) -> size_t;
    fn iconv_close(__cd: iconv_t) -> c_int;
}
/* automatically generated ends */

/// The representation of a iconv converter
pub struct Converter {
    cd: iconv_t,
}

impl Converter {
    /// Creates a new Converter from ``from`` encoding and ``to`` encoding.
    pub fn new(from: &str, to: &str) -> Converter {
        let handle = from.with_c_str(|from_encoding| {
                to.with_c_str(|to_encoding| unsafe {
                        iconv_open(to_encoding, from_encoding)
                    })
            });
        if handle == -1 as iconv_t {
            panic!("Error creating conversion descriptor from {:} to {:}", from, to);
        }
        Converter { cd: handle }
    }

    /// Convert from input into output.
    /// Returns (bytes_read, bytes_written, errno).
    pub fn convert(&self, input: &[u8], output: &mut [u8]) -> (uint, uint, c_int) {
        let input_left = input.len() as size_t;
        let output_left = output.len() as size_t;

        if input_left > 0 && output_left > 0 {
            let input_ptr = input.as_ptr();
            let output_ptr = output.as_ptr();

            let ret = unsafe { iconv(self.cd,
                                     mem::transmute(&input_ptr), mem::transmute(&input_left),
                                     mem::transmute(&output_ptr), mem::transmute(&output_left))
            };
            let bytes_read = input.len() - input_left as uint;
            let bytes_written = output.len() - output_left as uint;

            return (bytes_read, bytes_written, if ret == -1 as size_t { errno() as c_int } else { 0 })
        } else if input_left == 0 && output_left > 0 {
            let output_ptr = output.as_ptr();

            let ret = unsafe { iconv(self.cd,
                                     ptr::null_mut::<*mut c_char>(), mem::transmute(&input_left),
                                     mem::transmute(&output_ptr), mem::transmute(&output_left))
            };

            let bytes_written = output.len() - output_left as uint;

            return (0, bytes_written, if -1 as size_t == ret { errno() as c_int } else { 0 })
        } else {
            let ret = unsafe { iconv(self.cd,
                                     ptr::null_mut::<*mut c_char>(), mem::transmute(&input_left),
                                     ptr::null_mut::<*mut c_char>(), mem::transmute(&output_left))
            };

            return (0, 0, if -1 as size_t == ret { errno() as c_int } else { 0 })
        }
    }
}


impl Drop for Converter {
    fn drop(&mut self) {
        unsafe { iconv_close(self.cd) };
    }
}

/// A ``Reader`` which does iconv convert from another Reader.
pub struct IconvReader<R> {
    inner: R,
    conv: Converter,
    buf: Vec<u8>,
    read_pos: uint,
    write_pos: uint,
    err: Option<IoError>,
    tempbuf: Vec<u8>,        // used when outbut is too small and can't make a single convertion
}

impl<R:Reader> IconvReader<R> {
    pub fn new(r: R, from: &str, to: &str) -> IconvReader<R> {
        let conv = Converter::new(from, to);
        IconvReader { inner: r, conv: conv,
                      buf: Vec::from_elem(8*1024, 0u8),
                      read_pos: 0, write_pos: 0, err: None,
                      tempbuf: Vec::new(), // small buf allocate dynamicly
        }
    }

    fn fill_buf(&mut self) {
        if self.read_pos > 0 {
            unsafe {
                ptr::copy_memory::<u8>(self.buf.as_mut_ptr(),
                                       mem::transmute(&self.buf[self.read_pos]),
                                       self.write_pos - self.read_pos);
            }

            self.write_pos -= self.read_pos;
            self.read_pos = 0;
        }
        match self.inner.read(self.buf.slice_from_mut(self.write_pos)) {
            Ok(nread) => {
                self.write_pos += nread;
            }
            Err(e) => {
                self.err = Some(e);
            }
        }
    }
}

impl<R:Reader> Reader for IconvReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint> {
        if self.tempbuf.len() != 0 {
            let nwrite = buf.clone_from_slice(self.tempbuf.as_slice());
            if nwrite < self.tempbuf.len() {
                self.tempbuf = self.tempbuf.slice_from(nwrite).to_vec();
            } else {
                self.tempbuf = Vec::new();
            }
            return Ok(nwrite);
        }

        while self.write_pos == 0 || self.read_pos == self.write_pos {
            if self.err.is_some() {
                return Err(self.err.clone().unwrap());
            }

            self.fill_buf();
        }

        let (nread, nwrite, err) = self.conv.convert(self.buf.slice(self.read_pos, self.write_pos), buf);

        self.read_pos += nread;

        match err {
            EILSEQ => {
                debug!("An invalid multibyte sequence has been encountered in the input.");
                return Err(io::standard_error(io::InvalidInput));
            }
            EINVAL => {
                debug!("An incomplete multibyte sequence has been encountered in the input.");
                // FIXME fill_buf() here is ugly
                self.fill_buf();
                return Ok(nwrite);
            }
            E2BIG => {
                debug!("There is not sufficient room at *outbuf.");
                // FIXED: if outbuf buffer has size 1? Can't hold a
                if nread == 0 && nwrite == 0 && buf.len() > 0 {
                    // outbuf too small and can't conv 1 rune
                    let mut tempbuf = Vec::from_elem(8, 0u8);
                    assert!(self.tempbuf.is_empty());
                    let (nread, temp_nwrite, err) = self.conv.convert(self.buf.slice(self.read_pos, self.write_pos), tempbuf.as_mut_slice());
                    self.read_pos += nread;
                    // here we will write 1 or 2 bytes as most.
                    // try avoiding return Ok(0)
                    let nwrite = buf.clone_from_slice(tempbuf.as_slice());
                    self.tempbuf = tempbuf.slice(nwrite, temp_nwrite).to_vec();
                    match err {
                        EILSEQ => return Err(io::standard_error(io::InvalidInput)),
                        _ => return Ok(nwrite),
                    }
                }
                return Ok(nwrite);
            }
            0 => {
                return Ok(nwrite);
            }
            _ => unreachable!()
        }
    }
}

/// A ``Writer`` which does iconv convert into another Writer. not implemented yet.
pub struct IconvWriter<W> {
    inner: W,
    conv: Converter,
    buf: Vec<u8>,
    read_pos: uint,
    write_pos: uint,
    err: Option<IoError>,
}

impl<W:Writer> IconvWriter<W> {
    pub fn new(r: W, from: &str, to: &str) -> IconvWriter<W> {
        let conv = Converter::new(from, to);
        IconvWriter { inner: r, conv: conv,
                      buf: Vec::from_elem(8*1024, 0u8),
                      read_pos: 0, write_pos: 0, err: None,
        }
    }
}

impl<W:Writer> Writer for IconvWriter<W> {
    fn write(&mut self, _buf: &[u8]) -> IoResult<()> {
        unimplemented!()
    }
}

// TODO: use Result<> instead of Option<> to indicate Error
fn convert_bytes(inbuf: &[u8], from: &str, to: &str) -> Option<Vec<u8>> {
    let converter = Converter::new(from, to);
    let mut outbuf_size = inbuf.len() * 2;
    let mut total_nread = 0;
    let mut total_nwrite = 0;

    let mut outbuf = Vec::with_capacity(outbuf_size);
    unsafe { outbuf.set_len(outbuf_size) };

    while total_nread < inbuf.len() {
        let (nread, nwrite, err) = converter.convert(inbuf.slice_from(total_nread),
                                                     outbuf.slice_from_mut(total_nwrite));

        total_nread += nread;
        total_nwrite += nwrite;

        match err {
            EINVAL | EILSEQ => return None,
            E2BIG => {
                outbuf_size += inbuf.len();
                outbuf.reserve(outbuf_size);
                unsafe { outbuf.set_len(outbuf_size) };
            }
            _ => ()
        }
    }

    unsafe { outbuf.set_len(total_nwrite) };
    outbuf.shrink_to_fit();

    return Some(outbuf);
}


/// Can be encoded to bytes via iconv
pub trait IconvEncodable {
    /// Encode to bytes with encoding
    fn encode_with_encoding(&self, encoding: &str) -> Option<Vec<u8>>;
}

impl<'a> IconvEncodable for &'a [u8] {
    fn encode_with_encoding(&self, encoding: &str) -> Option<Vec<u8>> {
        convert_bytes(*self, "UTF-8", encoding)
    }
}

impl<'a> IconvEncodable for Vec<u8> {
    fn encode_with_encoding(&self, encoding: &str) -> Option<Vec<u8>> {
        convert_bytes(self.as_slice(), "UTF-8", encoding)
    }
}

impl<'a> IconvEncodable for &'a str {
    fn encode_with_encoding(&self, encoding: &str) -> Option<Vec<u8>> {
        return self.as_bytes().encode_with_encoding(encoding);
    }
}

impl<'a> IconvEncodable for String {
    fn encode_with_encoding(&self, encoding: &str) -> Option<Vec<u8>> {
        return self.as_bytes().encode_with_encoding(encoding);
    }
}

/// Can be decoded to str via iconv
pub trait IconvDecodable {
    /// Decode to str with encoding
    fn decode_with_encoding(&self, encoding: &str) -> Option<String>;
}

impl<'a> IconvDecodable for &'a [u8] {
    fn decode_with_encoding(&self, encoding: &str) -> Option<String> {
        convert_bytes(*self, encoding, "UTF-8").and_then(|bs| {
                str::from_utf8(bs.as_slice()).map(|s| {
                        s.into_string()
                    })
            })
    }
}

impl<'a> IconvDecodable for Vec<u8> {
    fn decode_with_encoding(&self, encoding: &str) -> Option<String> {
        convert_bytes(self.as_slice(), encoding, "UTF-8").and_then(|bs| {
                str::from_utf8(bs.as_slice()).map(|s| {
                        s.into_string()
                    })
            })
    }
}


#[cfg(test)]
mod test {
    use std::io;
    use std::io::BufReader;

    use super::*;

    #[test]
    fn test_reader() {
        let a = "噗哈";
        let cont = a.repeat(1024);

        let r = BufReader::new(cont.as_bytes());
        let mut cr = IconvReader::new(r, "UTF-8", "GBK");

        let mut nread: int = 0;
        loop {
            match cr.read_exact(4) {
                Ok(ref seg) if seg.len() == 4 => {
                    assert_eq!(seg, &vec!(224, 219, 185, 254));
                    nread += 4;
                }
                Err(ref e) if e.kind == io::EndOfFile => {
                    break;
                }
                _ => {
                    unreachable!();
                }
            }
        }
        assert_eq!(nread, 1024 * 4);
    }

    #[test]
    fn test_encoder_normal() {
        assert!("".encode_with_encoding("LATIN1").unwrap().is_empty());

        let a = "哈哈";
        assert_eq!(a.encode_with_encoding("GBK").unwrap(), vec!(0xb9, 0xfe, 0xb9, 0xfe));

        let b = a.repeat(1024);
        for ch in b.encode_with_encoding("GBK").unwrap().as_slice().chunks(4) {
            assert_eq!(ch, vec![0xb9, 0xfe, 0xb9, 0xfe].as_slice());
        }

        let c = vec!(0xe5, 0x93, 0x88, 0xe5, 0x93, 0x88); // utf8 bytes
        assert_eq!(c.encode_with_encoding("GBK").unwrap(), vec!(0xb9, 0xfe, 0xb9, 0xfe));
    }

    #[test]
    #[should_fail]
    fn test_encoder_fail_creating_converter() {
        assert!("".encode_with_encoding("NOT_EXISTS").unwrap().is_empty());
    }

    #[test]
    #[should_fail]
    fn test_encoder_ilseq() {
        let a = vec!(0xff, 0xff, 0xff);
        a.encode_with_encoding("GBK").unwrap();
    }

    #[test]
    #[should_fail]
    fn test_encoder_invalid() {
        let a = vec!(0xe5, 0x93, 0x88, 0xe5, 0x88); // incomplete utf8 bytes
        a.encode_with_encoding("GBK").unwrap();
    }

    #[test]
    fn test_decoder_normal() {
        assert_eq!(b"".decode_with_encoding("CP936").unwrap(), "".to_string());

        let a = vec!(0xb9, 0xfe, 0xb9, 0xfe);
        assert_eq!(a.decode_with_encoding("GBK").unwrap(), "哈哈".to_string());

        let b = Vec::from_fn(1000, |i| a[i%4]); // grow to 1000 bytes and fill with a

        for c in b.decode_with_encoding("GBK").unwrap().as_slice().chars() {
            assert_eq!(c, '哈');
        }
    }

    #[test]
    #[should_fail]
    fn test_decoder_fail_creating_converter() {
        assert_eq!(b"".decode_with_encoding("NOT_EXSITS").unwrap(), "".to_string());
    }

    #[test]
    #[should_fail]
    fn test_decoder_ilseq() {
        let a = vec!(0xff, 0xff, 0xff);
        a.decode_with_encoding("GBK").unwrap();
    }

    #[test]
    #[should_fail]
    fn test_decoder_invalid() {
        let a = vec!(0xb9, 0xfe, 0xb9); // incomplete gbk bytes
        a.decode_with_encoding("GBK").unwrap();
    }

    #[test]
    fn test_caocao_joke() {
        let a = "曹操";
        let b = "变巨";
        assert_eq!(a.encode_with_encoding("BIG5").unwrap(),
                   b.encode_with_encoding("GBK").unwrap());
    }
}
