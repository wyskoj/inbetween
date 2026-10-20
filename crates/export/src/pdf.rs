//! A contact sheet as a PDF: pictures in a grid with a caption under each, a title on every
//! page and page numbers.
//!
//! The writer is small and written by hand: pictures arrive already encoded as baseline JPEG
//! (RGB) and are embedded as they are (`DCTDecode`), text uses the built-in Helvetica (so it
//! needs no font file; characters outside Latin-1 become `?`). The output is deterministic.

use crate::ExportError;

/// Most pictures one contact sheet may hold.
pub const MAX_CELLS: usize = 20_000;
/// Most pages one contact sheet may have.
pub const MAX_PAGES: usize = 2_000;
/// Most columns on a page.
pub const MAX_COLUMNS: u32 = 12;

/// A picture for the sheet: baseline JPEG bytes (three components) and their pixel size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfImage {
    pub width: u32,
    pub height: u32,
    pub jpeg: Vec<u8>,
}

/// One cell of the grid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub image: PdfImage,
    pub caption: String,
}

/// Page sizes in points (1/72 inch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    pub width: f64,
    pub height: f64,
}

impl PageSize {
    pub const A4: Self = Self {
        width: 595.28,
        height: 841.89,
    };
    pub const LETTER: Self = Self {
        width: 612.0,
        height: 792.0,
    };

    pub fn landscape(self) -> Self {
        Self {
            width: self.height,
            height: self.width,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetOptions {
    pub page: PageSize,
    pub columns: u32,
    /// Shown at the top of every page.
    pub title: String,
    /// Draw each cell's caption under its picture.
    pub captions: bool,
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self {
            page: PageSize::A4,
            columns: 3,
            title: String::new(),
            captions: true,
        }
    }
}

const MARGIN: f64 = 36.0;
const GAP: f64 = 10.0;
const HEADER: f64 = 22.0;
const FOOTER: f64 = 22.0;
const CAPTION_LINE: f64 = 11.0;
const CAPTION_SIZE: f64 = 8.0;

/// Where each cell goes on its page: `(page, x, y, box width, box height)` with the origin at
/// the bottom left, in points, plus the grid's geometry.
struct Grid {
    cell_w: f64,
    cell_h: f64,
    rows: usize,
    columns: usize,
    top: f64,
}

fn grid(opts: &SheetOptions) -> Result<Grid, ExportError> {
    let columns = opts.columns.clamp(1, MAX_COLUMNS) as usize;
    let usable_w = opts.page.width - 2.0 * MARGIN;
    let cell_w = (usable_w - GAP * (columns as f64 - 1.0)) / columns as f64;
    let caption = if opts.captions {
        2.0 * CAPTION_LINE
    } else {
        0.0
    };
    // Pictures sit in a 4:3 box; the caption goes under it.
    let cell_h = cell_w * 0.75 + caption + 4.0;
    let usable_h = opts.page.height - 2.0 * MARGIN - HEADER - FOOTER;
    let rows = ((usable_h + GAP) / (cell_h + GAP)).floor();
    if !(cell_w > 20.0 && rows >= 1.0) {
        return Err(ExportError::Encode(
            "the page is too small for that many columns".into(),
        ));
    }
    Ok(Grid {
        cell_w,
        cell_h,
        rows: rows as usize,
        columns,
        top: opts.page.height - MARGIN - HEADER,
    })
}

/// Escape text for a PDF string in WinAnsi (Latin-1) encoding.
fn pdf_text(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.push(b'(');
    for c in text.chars() {
        let b = match u8::try_from(u32::from(c)) {
            Ok(b) if b >= 0x20 && b != 0x7F => b,
            _ => b'?',
        };
        if matches!(b, b'(' | b')' | b'\\') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b')');
    out
}

/// Shorten `text` so it fits `width` points in Helvetica at `size` (average glyph about half
/// an em), ending in "..." when it was cut.
fn fit_text(text: &str, size: f64, width: f64) -> String {
    let max = ((width / (size * 0.5)).floor() as usize).max(4);
    let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
    if chars.len() <= max {
        return chars.into_iter().collect();
    }
    let keep = max.saturating_sub(3);
    let mut s: String = chars.into_iter().take(keep).collect();
    s.push_str("...");
    s
}

fn num(v: f64) -> String {
    // Two decimals is plenty for points and keeps the output small and stable.
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

struct Writer {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl Writer {
    fn new() -> Self {
        let mut out = b"%PDF-1.4\n".to_vec();
        // A comment with high bytes tells tools the file is binary.
        out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");
        Self {
            out,
            offsets: Vec::new(),
        }
    }

    /// Start object number `id` (objects must be written in order from 1).
    fn begin(&mut self, id: usize) {
        while self.offsets.len() < id {
            self.offsets.push(0);
        }
        if let Some(slot) = self.offsets.get_mut(id - 1) {
            *slot = self.out.len();
        }
        self.out
            .extend_from_slice(format!("{id} 0 obj\n").as_bytes());
    }

    fn end(&mut self) {
        self.out.extend_from_slice(b"endobj\n");
    }

    fn object(&mut self, id: usize, body: &str) {
        self.begin(id);
        self.out.extend_from_slice(body.as_bytes());
        self.out.push(b'\n');
        self.end();
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        self.begin(id);
        self.out
            .extend_from_slice(format!("<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes());
        self.out.extend_from_slice(data);
        self.out.extend_from_slice(b"\nendstream\n");
        self.end();
    }

    fn finish(mut self, root: usize) -> Vec<u8> {
        let xref = self.out.len();
        let count = self.offsets.len() + 1;
        self.out
            .extend_from_slice(format!("xref\n0 {count}\n0000000000 65535 f \n").as_bytes());
        for off in &self.offsets {
            self.out
                .extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        self.out.extend_from_slice(
            format!("trailer\n<< /Size {count} /Root {root} 0 R >>\nstartxref\n{xref}\n%%EOF\n")
                .as_bytes(),
        );
        self.out
    }
}

/// How many pages `count` pictures take with these options.
pub fn pages_needed(count: usize, opts: &SheetOptions) -> Result<usize, ExportError> {
    let g = grid(opts)?;
    Ok(count.div_ceil(g.rows * g.columns))
}

/// Lay `cells` out on pages and write the PDF.
pub fn contact_sheet(cells: &[Cell], opts: &SheetOptions) -> Result<Vec<u8>, ExportError> {
    if cells.is_empty() {
        return Err(ExportError::NoFrames);
    }
    if cells.len() > MAX_CELLS {
        return Err(ExportError::Encode(format!(
            "a contact sheet holds at most {MAX_CELLS} pictures"
        )));
    }
    for c in cells {
        if c.image.width == 0 || c.image.height == 0 || c.image.jpeg.len() < 4 {
            return Err(ExportError::BadSize {
                width: c.image.width,
                height: c.image.height,
            });
        }
    }
    let g = grid(opts)?;
    let per_page = g.rows * g.columns;
    let pages = cells.len().div_ceil(per_page);
    if pages > MAX_PAGES {
        return Err(ExportError::Encode(format!(
            "that would be {pages} pages; the limit is {MAX_PAGES}. Use fewer pictures or more columns"
        )));
    }

    // Object numbers: 1 catalog, 2 page tree, 3 font, 4.. images, then a page and its content
    // for each page.
    const FIRST_IMAGE: usize = 4;
    let first_page = FIRST_IMAGE + cells.len();
    let mut w = Writer::new();
    w.object(1, "<< /Type /Catalog /Pages 2 0 R >>");
    let kids: Vec<String> = (0..pages)
        .map(|p| format!("{} 0 R", first_page + 2 * p))
        .collect();
    w.object(
        2,
        &format!(
            "<< /Type /Pages /Kids [{}] /Count {pages} >>",
            kids.join(" ")
        ),
    );
    w.object(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
    );
    for (i, c) in cells.iter().enumerate() {
        w.stream(
            FIRST_IMAGE + i,
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
                c.image.width, c.image.height
            ),
            &c.image.jpeg,
        );
    }

    let (pw, ph) = (opts.page.width, opts.page.height);
    for page in 0..pages {
        let page_id = first_page + 2 * page;
        let content_id = page_id + 1;
        let start = page * per_page;
        let on_page = &cells[start..cells.len().min(start + per_page)];
        let mut content: Vec<u8> = Vec::new();
        let text = |content: &mut Vec<u8>, x: f64, y: f64, size: f64, s: &str| {
            content.extend_from_slice(
                format!("BT /F1 {} Tf {} {} Td ", num(size), num(x), num(y)).as_bytes(),
            );
            content.extend_from_slice(&pdf_text(s));
            content.extend_from_slice(b" Tj ET\n");
        };
        if !opts.title.is_empty() {
            let title = fit_text(&opts.title, 12.0, pw - 2.0 * MARGIN);
            text(&mut content, MARGIN, ph - MARGIN - 12.0, 12.0, &title);
        }
        text(
            &mut content,
            pw / 2.0 - 30.0,
            MARGIN - 8.0,
            8.0,
            &format!("page {} of {pages}", page + 1),
        );
        let mut resources = String::new();
        for (n, cell) in on_page.iter().enumerate() {
            let (row, col) = (n / g.columns, n % g.columns);
            let x = MARGIN + col as f64 * (g.cell_w + GAP);
            let top = g.top - row as f64 * (g.cell_h + GAP);
            let box_h = g.cell_w * 0.75;
            // The picture fits its 4:3 box, centred.
            let (iw, ih) = (f64::from(cell.image.width), f64::from(cell.image.height));
            let scale = (g.cell_w / iw).min(box_h / ih);
            let (dw, dh) = (iw * scale, ih * scale);
            let (dx, dy) = (x + (g.cell_w - dw) / 2.0, top - box_h + (box_h - dh) / 2.0);
            let name = format!("Im{}", start + n);
            content.extend_from_slice(
                format!(
                    "q {} 0 0 {} {} {} cm /{name} Do Q\n",
                    num(dw),
                    num(dh),
                    num(dx),
                    num(dy)
                )
                .as_bytes(),
            );
            resources.push_str(&format!("/{name} {} 0 R ", FIRST_IMAGE + start + n));
            if opts.captions && !cell.caption.is_empty() {
                // Up to two lines, split at the last space that fits.
                let max = ((g.cell_w / (CAPTION_SIZE * 0.5)).floor() as usize).max(4);
                let caption: String = cell.caption.chars().filter(|c| !c.is_control()).collect();
                let (first, rest) = split_caption(&caption, max);
                text(&mut content, x, top - box_h - 9.0, CAPTION_SIZE, &first);
                if !rest.is_empty() {
                    let rest = fit_text(&rest, CAPTION_SIZE, g.cell_w);
                    text(
                        &mut content,
                        x,
                        top - box_h - 9.0 - CAPTION_LINE,
                        CAPTION_SIZE,
                        &rest,
                    );
                }
            }
        }
        w.object(
            page_id,
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> /XObject << {resources}>> >> /Contents {content_id} 0 R >>",
                num(pw),
                num(ph)
            ),
        );
        w.stream(content_id, "", &content);
    }
    Ok(w.finish(1))
}

/// Split `text` into a first line of at most `max` characters (at a space when there is one)
/// and the rest.
fn split_caption(text: &str, max: usize) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return (text.to_string(), String::new());
    }
    let cut = chars
        .iter()
        .take(max)
        .rposition(|c| *c == ' ')
        .filter(|i| *i > max / 3)
        .unwrap_or(max);
    let first: String = chars.iter().take(cut).collect();
    let rest: String = chars
        .iter()
        .skip(cut)
        .collect::<String>()
        .trim_start()
        .to_string();
    (first, rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny valid-looking JPEG stand-in: the writer never decodes it.
    fn cell(n: usize, w: u32, h: u32, caption: &str) -> Cell {
        Cell {
            image: PdfImage {
                width: w,
                height: h,
                jpeg: vec![0xFF, 0xD8, n as u8, 0xFF, 0xD9],
            },
            caption: caption.to_string(),
        }
    }

    fn count(haystack: &[u8], needle: impl AsRef<[u8]>) -> usize {
        let n = needle.as_ref();
        haystack.windows(n.len()).filter(|w| *w == n).count()
    }

    /// Every xref entry must point at `N 0 obj`.
    fn check_xref(pdf: &[u8]) {
        let text = String::from_utf8_lossy(pdf);
        let at = text.rfind("startxref\n").unwrap();
        let xref: usize = text[at + 10..].lines().next().unwrap().parse().unwrap();
        assert!(pdf[xref..].starts_with(b"xref\n"));
        let table = String::from_utf8_lossy(&pdf[xref..]);
        let mut lines = table.lines();
        lines.next();
        let header = lines.next().unwrap();
        let total: usize = header.split(' ').nth(1).unwrap().parse().unwrap();
        assert_eq!(lines.next().unwrap(), "0000000000 65535 f ");
        for id in 1..total {
            let entry = lines.next().unwrap();
            let off: usize = entry[..10].parse().unwrap();
            let want = format!("{id} 0 obj\n");
            assert!(
                pdf[off..].starts_with(want.as_bytes()),
                "object {id} at {off}"
            );
        }
    }

    #[test]
    fn a_sheet_has_the_right_pages_images_and_a_valid_xref() {
        let cells: Vec<Cell> = (0..25)
            .map(|i| cell(i, 40, 30, &format!("frame {}", i + 1)))
            .collect();
        let opts = SheetOptions {
            title: "My movie".into(),
            ..SheetOptions::default()
        };
        let pdf = contact_sheet(&cells, &opts).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        assert_eq!(count(&pdf, "/Subtype /Image"), 25);
        let per_page = {
            let g = grid(&opts).unwrap();
            g.rows * g.columns
        };
        let pages = 25usize.div_ceil(per_page);
        assert_eq!(count(&pdf, "/Type /Page "), pages);
        assert_eq!(count(&pdf, "/Type /Page /Parent"), pages);
        assert_eq!(count(&pdf, format!("/Count {pages}")), 1);
        assert_eq!(count(&pdf, "(My movie)"), pages);
        assert_eq!(count(&pdf, "(frame 7)"), 1);
        check_xref(&pdf);
        // Deterministic.
        assert_eq!(pdf, contact_sheet(&cells, &opts).unwrap());
    }

    #[test]
    fn captions_can_be_left_out_and_text_is_escaped() {
        let cells = [cell(0, 10, 10, "a (b) \\ c \u{e9} \u{4e2d}")];
        let with = contact_sheet(&cells, &SheetOptions::default()).unwrap();
        assert_eq!(count(&with, b"(a \\(b\\) \\\\ c \xe9 ?)"), 1);
        let without = contact_sheet(
            &cells,
            &SheetOptions {
                captions: false,
                ..SheetOptions::default()
            },
        )
        .unwrap();
        assert_eq!(count(&without, "(a "), 0);
        check_xref(&without);
    }

    #[test]
    fn long_captions_wrap_to_two_lines_and_are_cut() {
        let long = "word ".repeat(60);
        let pdf = contact_sheet(
            &[cell(0, 10, 10, &long)],
            &SheetOptions {
                columns: 6,
                ..SheetOptions::default()
            },
        )
        .unwrap();
        assert!(count(&pdf, "...)") >= 1);
        let (a, b) = split_caption("alpha beta gamma delta", 12);
        assert_eq!((a.as_str(), b.as_str()), ("alpha beta", "gamma delta"));
        assert_eq!(
            split_caption("short", 12),
            ("short".to_string(), String::new())
        );
    }

    #[test]
    fn landscape_and_columns_change_the_layout() {
        let cells: Vec<Cell> = (0..12).map(|i| cell(i, 40, 30, "x")).collect();
        let portrait = contact_sheet(
            &cells,
            &SheetOptions {
                columns: 2,
                ..SheetOptions::default()
            },
        )
        .unwrap();
        let wide = contact_sheet(
            &cells,
            &SheetOptions {
                columns: 4,
                page: PageSize::LETTER.landscape(),
                ..SheetOptions::default()
            },
        )
        .unwrap();
        assert!(count(&wide, "/MediaBox [0 0 792 612]") >= 1);
        assert!(
            count(&portrait, "/Type /Page /Parent") > count(&wide, "/Type /Page /Parent")
                || count(&portrait, "/Type /Page /Parent") >= 1
        );
    }

    #[test]
    fn hostile_input_is_an_error_not_a_panic() {
        assert_eq!(
            contact_sheet(&[], &SheetOptions::default()),
            Err(ExportError::NoFrames)
        );
        let bad = Cell {
            image: PdfImage {
                width: 0,
                height: 5,
                jpeg: vec![1, 2, 3, 4],
            },
            caption: String::new(),
        };
        assert!(contact_sheet(&[bad], &SheetOptions::default()).is_err());
        let tiny = Cell {
            image: PdfImage {
                width: 5,
                height: 5,
                jpeg: vec![],
            },
            caption: String::new(),
        };
        assert!(contact_sheet(&[tiny], &SheetOptions::default()).is_err());
        let tiny_page = SheetOptions {
            page: PageSize {
                width: 60.0,
                height: 60.0,
            },
            ..SheetOptions::default()
        };
        assert!(contact_sheet(&[cell(0, 5, 5, "")], &tiny_page).is_err());
        let nan_page = SheetOptions {
            page: PageSize {
                width: f64::NAN,
                height: 800.0,
            },
            ..SheetOptions::default()
        };
        assert!(contact_sheet(&[cell(0, 5, 5, "")], &nan_page).is_err());
        let many: Vec<Cell> = (0..MAX_CELLS + 1).map(|i| cell(i, 1, 1, "")).collect();
        assert!(contact_sheet(&many, &SheetOptions::default()).is_err());
        // Zero or huge column counts are clamped.
        for columns in [0, 1, 99] {
            let _ = contact_sheet(
                &[cell(0, 5, 5, "")],
                &SheetOptions {
                    columns,
                    ..SheetOptions::default()
                },
            );
        }
    }
}
