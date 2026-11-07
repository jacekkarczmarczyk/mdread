//! Nazwa (opis z zasobów wersji) i ikona programu `.exe` do listy edytorów.

use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::Win32::UI::Shell::SHDefExtractIconW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{HSTRING, w};

/// Opis pliku (`FileDescription`), np. „Notepad++ : a free (GNU) source code editor” albo „Visual Studio Code”.
pub fn description(path: &Path) -> Option<String> {
    let file = HSTRING::from(path.as_os_str());
    unsafe {
        let size = GetFileVersionInfoSizeW(&file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&file, None, size, data.as_mut_ptr() as _).ok()?;

        let mut pointer = std::ptr::null_mut();
        let mut length = 0u32;
        if !VerQueryValueW(data.as_ptr() as _, w!("\\VarFileInfo\\Translation"), &mut pointer, &mut length).as_bool()
            || length < 4
        {
            return None;
        }
        let translation = std::slice::from_raw_parts(pointer as *const u16, 2);
        let key = format!("\\StringFileInfo\\{:04x}{:04x}\\FileDescription", translation[0], translation[1]);
        if !VerQueryValueW(data.as_ptr() as _, &HSTRING::from(key), &mut pointer, &mut length).as_bool() || length == 0 {
            return None;
        }
        let text = std::slice::from_raw_parts(pointer as *const u16, length as usize);
        let text = String::from_utf16_lossy(text).trim_end_matches('\0').trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
}

/// Ikona programu jako PNG w adresie `data:` (bok `size` pikseli).
pub fn icon_data_url(path: &Path, size: u32) -> Option<String> {
    let pixels = icon_pixels(path, size)?;
    Some(format!("data:image/png;base64,{}", base64(&png(size, size, &pixels))))
}

/// Piksele RGBA ikony.
fn icon_pixels(path: &Path, size: u32) -> Option<Vec<u8>> {
    unsafe {
        let mut icon = HICON::default();
        SHDefExtractIconW(&HSTRING::from(path.as_os_str()), 0, 0, Some(&mut icon), None, size).ok().ok()?;
        let mut info = ICONINFO::default();
        let pixels = GetIconInfo(icon, &mut info).ok().and_then(|()| {
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: size as i32,
                    biHeight: -(size as i32), // od góry do dołu
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgra = vec![0u8; (size * size * 4) as usize];
            let dc = GetDC(None::<HWND>);
            let lines = GetDIBits(dc, info.hbmColor, 0, size, Some(bgra.as_mut_ptr() as _), &mut header, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            (lines == size as i32).then_some(bgra)
        });
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        let _ = DestroyIcon(icon);
        let mut pixels = pixels?;
        // Stare ikony bez kanału alfa: całość nieprzezroczysta.
        let has_alpha = pixels.chunks(4).any(|pixel| pixel[3] != 0);
        for pixel in pixels.chunks_mut(4) {
            pixel.swap(0, 2); // BGRA → RGBA
            if !has_alpha {
                pixel[3] = 255;
            }
        }
        Some(pixels)
    }
}

/// Minimalny PNG (RGBA, bez kompresji: bloki „stored” zlib) — ikony mają kilka KB.
fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for row in rgba.chunks(width as usize * 4) {
        raw.push(0); // filtr: brak
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let length = block.len() as u16;
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bitów, RGBA

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", header.as_slice()), (b"IDAT", zlib.as_slice()), (b"IEND", &[][..])] {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }
    png
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bytes = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let value = (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(ALPHABET[(value >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn reads_notepad_icon_and_name() {
        let notepad = Path::new(r"C:\Windows\System32\notepad.exe");
        assert!(icon_data_url(notepad, 32).is_some_and(|url| url.starts_with("data:image/png;base64,iVBOR")));
        assert!(description(notepad).is_some());
    }
}
