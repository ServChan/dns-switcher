//! Только для отладочной сборки: кадры egui в PNG без окна. egui считает кадры сам
//! (`Context::run_ui`), получившиеся треугольники рисует простой растеризатор, тем же
//! смешиванием, что и шейдер egui_glow. Время идёт по шагам 1/60 с. Окно не создаётся.

use eframe::egui::{self, Color32, ColorImage, Context, Event, Pos2, Rect, TextureId, Theme, Ui, ViewportId, pos2, vec2};
use std::collections::HashMap;
use std::path::Path;

pub const STEP: f32 = 1.0 / 60.0;

pub struct Frames {
    pub ctx: Context,
    textures: HashMap<TextureId, ColorImage>,
    primitives: Vec<egui::ClippedPrimitive>,
    time: f64,
    size: (f32, f32),
    ppp: f32,
    theme: Theme,
    pub events: Vec<Event>,
    /// Окно впереди и не свёрнуто.
    pub focused: bool,
    pub minimized: bool,
}

impl Frames {
    pub fn new(ctx: Context, size: (f32, f32), ppp: f32, theme: Theme) -> Self {
        Self { ctx, textures: HashMap::new(), primitives: Vec::new(), time: 100.0, size, ppp, theme, events: Vec::new(), focused: true, minimized: false }
    }

    pub fn now(&self) -> f64 {
        self.time
    }

    pub fn frame(&mut self, mut paint: impl FnMut(&mut Ui)) {
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.size.0, self.size.1))),
            time: Some(self.time),
            predicted_dt: STEP,
            system_theme: Some(self.theme),
            focused: self.focused,
            events: std::mem::take(&mut self.events),
            ..Default::default()
        };
        if let Some(viewport) = raw.viewports.get_mut(&ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(self.ppp);
            viewport.minimized = Some(self.minimized);
        }
        self.time += STEP as f64;
        let mut output = self.ctx.run_ui(raw, |ui| paint(ui));
        for (id, deltas) in std::mem::take(&mut output.textures_delta.set) {
            for delta in deltas {
                let egui::ImageData::Color(image) = &delta.image;
                match delta.pos {
                    None => {
                        self.textures.insert(id, (**image).clone());
                    }
                    Some([x0, y0]) => {
                        if let Some(target) = self.textures.get_mut(&id) {
                            let [w, h] = image.size;
                            for y in 0..h {
                                for x in 0..w {
                                    target.pixels[(y0 + y) * target.size[0] + x0 + x] = image.pixels[y * w + x];
                                }
                            }
                        }
                    }
                }
            }
        }
        self.primitives = self.ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
    }

    pub fn shot(&self, path: &Path) {
        let (w, h) = ((self.size.0 * self.ppp).round() as usize, (self.size.1 * self.ppp).round() as usize);
        let mut canvas = vec![[0.0f32, 0.0, 0.0]; w * h];
        for primitive in &self.primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive
                && let Some(texture) = self.textures.get(&mesh.texture_id)
            {
                paint_mesh(&mut canvas, (w, h), mesh, texture, primitive.clip_rect, self.ppp);
            }
        }
        write_png(path, &canvas, w, h);
    }
}

// ---------- Растеризатор ----------

fn premultiplied(color: Color32) -> [f32; 4] {
    color.to_array().map(|c| c as f32 / 255.0)
}

/// Билинейная выборка: как GL_LINEAR у текстуры egui. Цвета уже домножены на альфу.
fn sample(texture: &ColorImage, u: f32, v: f32) -> [f32; 4] {
    let [w, h] = texture.size;
    let (x, y) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |x: i32, y: i32| {
        let (x, y) = (x.clamp(0, w as i32 - 1) as usize, y.clamp(0, h as i32 - 1) as usize);
        premultiplied(texture.pixels[y * w + x])
    };
    let (a, b, c, d) = (at(x0 as i32, y0 as i32), at(x0 as i32 + 1, y0 as i32), at(x0 as i32, y0 as i32 + 1), at(x0 as i32 + 1, y0 as i32 + 1));
    std::array::from_fn(|i| (a[i] * (1.0 - fx) + b[i] * fx) * (1.0 - fy) + (c[i] * (1.0 - fx) + d[i] * fx) * fy)
}

fn paint_mesh(canvas: &mut [[f32; 3]], size: (usize, usize), mesh: &egui::Mesh, texture: &ColorImage, clip: Rect, ppp: f32) {
    let (w, h) = size;
    let (cx0, cy0) = ((clip.left() * ppp).round().max(0.0) as usize, (clip.top() * ppp).round().max(0.0) as usize);
    let (cx1, cy1) = (((clip.right() * ppp).round().max(0.0) as usize).min(w), ((clip.bottom() * ppp).round().max(0.0) as usize).min(h));
    // Правило верхнего левого ребра: у соседних треугольников общие пиксели не красятся дважды.
    let edge = |a: Pos2, b: Pos2, x: f32, y: f32| (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x);
    let owns = |a: Pos2, b: Pos2, value: f32, sign: f32| {
        let (dx, dy) = ((b.x - a.x) * sign, (b.y - a.y) * sign);
        value * sign > 0.0 || (value == 0.0 && (dy < 0.0 || (dy == 0.0 && dx > 0.0)))
    };
    for triangle in mesh.indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| &mesh.vertices[i as usize]);
        let (pa, pb, pc) = (pos2(a.pos.x * ppp, a.pos.y * ppp), pos2(b.pos.x * ppp, b.pos.y * ppp), pos2(c.pos.x * ppp, c.pos.y * ppp));
        let area = edge(pa, pb, pc.x, pc.y);
        if area == 0.0 {
            continue;
        }
        let sign = area.signum();
        let (x0, x1) = (pa.x.min(pb.x).min(pc.x).floor().max(cx0 as f32) as usize, (pa.x.max(pb.x).max(pc.x).ceil() as usize).min(cx1));
        let (y0, y1) = (pa.y.min(pb.y).min(pc.y).floor().max(cy0 as f32) as usize, (pa.y.max(pb.y).max(pc.y).ceil() as usize).min(cy1));
        let (ca, cb, cc) = (premultiplied(a.color), premultiplied(b.color), premultiplied(c.color));
        for y in y0..y1 {
            for x in x0..x1 {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let (ea, eb, ec) = (edge(pb, pc, fx, fy), edge(pc, pa, fx, fy), edge(pa, pb, fx, fy));
                if !(owns(pb, pc, ea, sign) && owns(pc, pa, eb, sign) && owns(pa, pb, ec, sign)) {
                    continue;
                }
                let (la, lb, lc) = (ea / area, eb / area, ec / area);
                let u = a.uv.x * la + b.uv.x * lb + c.uv.x * lc;
                let v = a.uv.y * la + b.uv.y * lb + c.uv.y * lc;
                let tex = sample(texture, u, v);
                let color: [f32; 4] = std::array::from_fn(|i| (ca[i] * la + cb[i] * lb + cc[i] * lc) * tex[i]);
                let pixel = &mut canvas[y * w + x];
                for i in 0..3 {
                    pixel[i] = color[i] + pixel[i] * (1.0 - color[3]);
                }
            }
        }
    }
}

// ---------- PNG без сжатия: новые зависимости ради отладки не нужны ----------

fn crc32(data: &[u8], start: u32) -> u32 {
    let mut crc = start;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    crc
}

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(name);
    out.extend(data);
    let crc = crc32(data, crc32(name, 0xFFFF_FFFF));
    out.extend((!crc).to_be_bytes());
}

fn write_png(path: &Path, canvas: &[[f32; 3]], w: usize, h: usize) {
    let mut raw = Vec::with_capacity((w * 3 + 1) * h);
    for row in canvas.chunks_exact(w) {
        raw.push(0);
        for pixel in row {
            raw.extend(pixel.map(|c| (c * 255.0 + 0.5).clamp(0.0, 255.0) as u8));
        }
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(blocks.peek().is_none() as u8);
        zlib.extend((block.len() as u16).to_le_bytes());
        zlib.extend((!(block.len() as u16)).to_le_bytes());
        zlib.extend(block);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    for byte in &raw {
        s1 = (s1 + *byte as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    zlib.extend(((s2 << 16) | s1).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend((w as u32).to_be_bytes());
    header.extend((h as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    std::fs::write(path, png).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
