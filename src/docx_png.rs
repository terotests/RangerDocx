// SPDX-License-Identifier: AGPL-3.0-or-later

use ranger::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::rgr::{ImageBuffer, Inflate};

/// ==============================================================================
/// DocxPng - minimal PNG → pdf_writer ImageBuffer (RGB/RGBA) for SoftCanvas.
/// ==============================================================================
/// Supports color types 2 (RGB) and 6 (RGBA), 8-bit, non-interlaced.
/// Uses gallery/zip Inflate. Kept at SoftCanvas ImageBuffer level (not OOXML).
/// ==============================================================================
#[derive(Clone)]
pub struct DocxPng {
}

impl DocxPng {
    pub fn new() -> Self {
        Self {
        }
    }
    fn decode(data: &[u8]) -> Rc<RefCell<ImageBuffer>> {
        let mut empty: Rc<RefCell<ImageBuffer>> = Rc::new(RefCell::new(ImageBuffer::new()));
        if (data.len() as i64) < 24 {
            return empty;
        }
        // signature
        if (data[0] as i64) != 137 {
            return empty;
        }
        if (data[1] as i64) != 80 {
            return empty;
        }
        let mut pos: i64 = 8;
        let n: i64 = data.len() as i64;
        let mut width: i64 = 0;
        let mut height: i64 = 0;
        let mut bit_depth: i64 = 0;
        let mut color_type: i64 = 0;
        let mut idat: Vec<u8> = vec![0u8; 0];
        let mut idat_len: i64 = 0;
        while pos + 8 <= n {
            let __len: i64 = DocxPng::u32(data, pos);
            pos += 4;
            let t0: i64 = data[pos as usize] as i64;
            let t1: i64 = data[(pos + 1) as usize] as i64;
            let t2: i64 = data[(pos + 2) as usize] as i64;
            let t3: i64 = data[(pos + 3) as usize] as i64;
            pos += 4;
            if ((t0 == 73 && t1 == 72) && t2 == 68) && t3 == 82 {
                // IHDR
                width = DocxPng::u32(data, pos);
                height = DocxPng::u32(data, pos + 4);
                bit_depth = data[(pos + 8) as usize] as i64;
                color_type = data[(pos + 9) as usize] as i64;
            }
            if ((t0 == 73 && t1 == 68) && t2 == 65) && t3 == 84 {
                // IDAT append
                let next_len: i64 = idat_len + __len;
                let mut next: Vec<u8> = vec![0u8; (next_len) as usize];
                let mut ci: i64 = 0;
                while ci < idat_len {
                    next[ci as usize] = (idat[ci as usize] as i64) as u8;
                    ci += 1;
                }
                let mut di: i64 = 0;
                while di < __len {
                    next[(idat_len + di) as usize] = (data[(pos + di) as usize] as i64) as u8;
                    di += 1;
                }
                idat = next.clone();
                idat_len = next_len;
            }
            if ((t0 == 73 && t1 == 69) && t2 == 78) && t3 == 68 {
                pos = n;
            } else {
                pos = (pos + __len) + 4;
            }
        }
        if width < 1 || height < 1 {
            return empty;
        }
        if bit_depth != 8 {
            return empty;
        }
        if color_type != 2 && color_type != 6 {
            return empty;
        }
        let mut bpp: i64 = 3;
        if color_type == 6 {
            bpp = 4;
        }
        let row_bytes: i64 = width * bpp;
        let expected: i64 = (row_bytes + 1) * height;
        // PNG IDAT is zlib-wrapped; strip CMF+FLG then raw deflate.
        if idat_len < 6 {
            return empty;
        }
        let deflate_len: i64 = idat_len - 6;
        let mut deflate_buf: Vec<u8> = vec![0u8; (deflate_len) as usize];
        let mut di2: i64 = 0;
        while di2 < deflate_len {
            deflate_buf[di2 as usize] = (idat[(di2 + 2) as usize] as i64) as u8;
            di2 += 1;
        }
        let mut infl: Rc<RefCell<Inflate>> = Rc::new(RefCell::new(Inflate::new()));
        let mut raw: Vec<u8> = Inflate::decompress(&infl, &deflate_buf);
        if (raw.len() as i64) < expected {
            return empty;
        }
        let mut img: Rc<RefCell<ImageBuffer>> = Rc::new(RefCell::new(ImageBuffer::new()));
        ImageBuffer::init(&img, width, height);
        let mut y: i64 = 0;
        let mut prev: Vec<i64> = Vec::new();
        let mut pi: i64 = 0;
        while pi < row_bytes {
            prev.push(0);
            pi += 1;
        }
        while y < height {
            let row_off: i64 = y * (row_bytes + 1);
            let filter: i64 = raw[row_off as usize] as i64;
            let mut cur: Vec<i64> = Vec::new();
            let mut x: i64 = 0;
            while x < row_bytes {
                let fv: i64 = raw[((row_off + 1) + x) as usize] as i64;
                let mut left: i64 = 0;
                if x >= bpp {
                    left = cur[(x - bpp) as usize];
                }
                let up: i64 = prev[x as usize];
                let mut up_left: i64 = 0;
                if x >= bpp {
                    up_left = prev[(x - bpp) as usize];
                }
                let mut recon: i64 = fv;
                if filter == 1 {
                    recon = fv + left;
                }
                if filter == 2 {
                    recon = fv + up;
                }
                if filter == 3 {
                    recon = fv + ((left + up) / 2);
                }
                if filter == 4 {
                    recon = fv + DocxPng::paeth(left, up, up_left);
                }
                recon = DocxPng::mod256(recon);
                cur.push(recon);
                x += 1;
            }
            let mut px: i64 = 0;
            while px < width {
                let o: i64 = (y * width + px) * 4;
                let base: i64 = px * bpp;
                img.borrow_mut().pixels[o as usize] = (cur[base as usize]) as u8;
                img.borrow_mut().pixels[(o + 1) as usize] = (cur[(base + 1) as usize]) as u8;
                img.borrow_mut().pixels[(o + 2) as usize] = (cur[(base + 2) as usize]) as u8;
                if bpp == 4 {
                    img.borrow_mut().pixels[(o + 3) as usize] = (cur[(base + 3) as usize]) as u8;
                } else {
                    img.borrow_mut().pixels[(o + 3) as usize] = 255;
                }
                px += 1;
            }
            prev = cur.clone();
            y += 1;
        }
        img
    }
    pub fn u32(data: &[u8], at: i64) -> i64 {
        let b0: i64 = data[at as usize] as i64;
        let b1: i64 = data[(at + 1) as usize] as i64;
        let b2: i64 = data[(at + 2) as usize] as i64;
        let b3: i64 = data[(at + 3) as usize] as i64;
        ((b0 * 256 + b1) * 256 + b2) * 256 + b3
    }
    pub fn mod256(mut v: i64) -> i64 {
        while v < 0 {
            v += 256;
        }
        while v >= 256 {
            v -= 256;
        }
        v
    }
    pub fn paeth(a: i64, b: i64, c: i64) -> i64 {
        let p: i64 = (a + b) - c;
        let mut pa: i64 = p - a;
        if pa < 0 {
            pa = 0 - pa;
        }
        let mut pb: i64 = p - b;
        if pb < 0 {
            pb = 0 - pb;
        }
        let mut pc: i64 = p - c;
        if pc < 0 {
            pc = 0 - pc;
        }
        if pa <= pb && pa <= pc {
            return a;
        }
        if pb <= pc {
            return b;
        }
        c
    }
}
