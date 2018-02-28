extern crate ransid;

use std::{cmp, mem, ptr};
use std::collections::BTreeSet;
use std::io::Result;

use config::Config;
use orbclient::{Color, EventOption, Mode, Renderer, Window, WindowFlag};
use orbfont::Font;

static DEFAULT_FONT: &'static [u8] = include_bytes!("../res/FiraMono-Regular.ttf");
static DEFAULT_FONT_BOLD: &'static [u8] = include_bytes!("../res/FiraMono-Bold.ttf");

#[derive(Clone, Copy)]
pub struct Block {
    c: char,
    fg: Color,
    bg: Color,
    bold: bool,
}

pub struct Console {
    pub console: ransid::Console,
    pub window: Window,
    pub alternate: bool,
    pub grid: Box<[Block]>,
    pub alt_grid: Box<[Block]>,
    pub font: Font,
    pub font_bold: Font,
    pub changed: BTreeSet<usize>,
    pub mouse_x: u16,
    pub mouse_y: u16,
    pub mouse_left: bool,
    pub ctrl: bool,
    pub input: Vec<u8>,
    pub requested: usize,
    pub block_width: usize,
    pub block_height: usize,
    pub alpha: u8,
}

impl Console {
    pub fn new(config: &Config, width: u32, height: u32, block_width: usize, block_height: usize) -> Console {
        let mut window = Window::new_flags(-1, -1, width, height, "Terminal", &[WindowFlag::Async, WindowFlag::Resizable])
                            .unwrap();
        window.sync();

        let ransid = ransid::Console::new(width as usize / block_width, height as usize / block_height);
        let grid = vec![Block {
            c: '\0',
            fg: Color { data: 0 },
            bg: Color { data: 0 },
            bold: false
        }; ransid.state.w * ransid.state.h].into_boxed_slice();

        let font = Font::from_path(&config.font).unwrap_or_else(|_| Font::from_data(DEFAULT_FONT).unwrap());
        let font_bold = Font::from_path(&config.font_bold).unwrap_or_else(|_| Font::from_data(DEFAULT_FONT_BOLD).unwrap());

        Console {
            console: ransid,
            alternate: false,
            grid: grid.clone(),
            alt_grid: grid,
            window: window,
            font: font,
            font_bold: font_bold,
            changed: BTreeSet::new(),
            mouse_x: 0,
            mouse_y: 0,
            mouse_left: false,
            ctrl: false,
            input: Vec::new(),
            requested: 0,
            block_width,
            block_height,
            alpha: 224
        }
    }

    pub fn input(&mut self, event_option: EventOption) {
        match event_option {
            EventOption::Key(key_event) => {
                let mut buf = vec![];

                if key_event.scancode == 0x1D {
                    self.ctrl = key_event.pressed;
                } else if key_event.pressed {
                    match key_event.scancode {
                        0x4A if self.ctrl => { // Ctrl-Minus
                            let new_block_width = self.block_width - 1;
                            self.set_block_size(new_block_width);

                            let w = self.window.width() as usize / self.block_width;
                            let h = self.window.height() as usize / self.block_height;

                            self.resize_grid(w, h);
                            self.sync();
                        },
                        0x4E if self.ctrl => { // Ctrl-Plus
                            let new_block_width = self.block_width + 1;
                            self.set_block_size(new_block_width);

                            let w = self.window.width() as usize / self.block_width;
                            let h = self.window.height() as usize / self.block_height;

                            self.resize_grid(w, h);
                            self.sync();
                        },
                        0x0E => { // Backspace
                            buf.extend_from_slice(b"\x7F");
                        },
                        0x47 => { // Home
                            buf.extend_from_slice(b"\x1B[H");
                        },
                        0x48 => { // Up
                            buf.extend_from_slice(b"\x1B[A");
                        },
                        0x49 => { // Page up
                            buf.extend_from_slice(b"\x1B[5~");
                        },
                        0x4B => { // Left
                            buf.extend_from_slice(b"\x1B[D");
                        },
                        0x4D => { // Right
                            buf.extend_from_slice(b"\x1B[C");
                        },
                        0x4F => { // End
                            buf.extend_from_slice(b"\x1B[F");
                        },
                        0x50 => { // Down
                            buf.extend_from_slice(b"\x1B[B");
                        },
                        0x51 => { // Page down
                            buf.extend_from_slice(b"\x1B[6~");
                        },
                        0x52 => { // Insert
                            buf.extend_from_slice(b"\x1B[2~");
                        },
                        0x53 => { // Delete
                            buf.extend_from_slice(b"\x1B[3~");
                        },
                        _ => {
                            let c = match key_event.character {
                                '\n' => '\r',
                                c @ 'A' ... 'Z' if self.ctrl => ((c as u8 - b'A') + b'\x01') as char,
                                c @ 'a' ... 'z' if self.ctrl => ((c as u8 - b'a') + b'\x01') as char,
                                c => c
                            };

                            if c != '\0' {
                                let mut b = [0; 4];
                                buf.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                            }
                        }
                    }
                }

                self.input.extend(buf);
            },
            EventOption::Mouse(mouse_event) => {
                let x = (mouse_event.x / self.block_width as i32) as u16 + 1;
                let y = (mouse_event.y / self.block_height as i32) as u16 + 1;
                if self.console.state.mouse_rxvt && self.console.state.mouse_btn {
                    if self.mouse_left && (x != self.mouse_x || y != self.mouse_y) {
                        let string = format!("\x1B[<{};{};{}M", 32, self.mouse_x, self.mouse_y);
                        self.input.extend(string.as_bytes());
                    }
                }
                self.mouse_x = x;
                self.mouse_y = y;
            },
            EventOption::Button(button_event) => {
                if self.console.state.mouse_rxvt {
                    if button_event.left {
                        if ! self.mouse_left {
                            let string = format!("\x1B[<{};{};{}M", 0, self.mouse_x, self.mouse_y);
                            self.input.extend(string.as_bytes());
                        }
                    } else if self.mouse_left {
                        let string = format!("\x1B[<{};{};{}m", 0, self.mouse_x, self.mouse_y);
                        self.input.extend(string.as_bytes());
                    }
                    self.mouse_left = button_event.left;
                }
            },
            EventOption::Scroll(scroll_event) => {
                if self.ctrl {
                    let new_block_width = (self.block_width as i32 + scroll_event.y.signum()) as usize;
                    self.set_block_size(new_block_width);

                    let w = self.window.width() as usize / self.block_width;
                    let h = self.window.height() as usize / self.block_height;

                    self.resize_grid(w, h);
                    self.sync();
                } else if self.console.state.mouse_rxvt {
                    if scroll_event.y > 0 {
                        let string = format!("\x1B[<{};{};{}M", 64, self.mouse_x, self.mouse_y);
                        self.input.extend(string.as_bytes());
                    } else if scroll_event.y < 0 {
                        let string = format!("\x1B[<{};{};{}M", 65, self.mouse_x, self.mouse_y);
                        self.input.extend(string.as_bytes());
                    }
                }
            },
            EventOption::Resize(resize_event) => {
                let w = resize_event.width as usize / self.block_width;
                let h = resize_event.height as usize / self.block_height;
                self.resize_grid(w, h);
                self.sync();
            },
            _ => ()
        }
    }

    pub fn invert(&mut self, x: usize, y: usize, w: usize, h: usize) {
        let width = self.window.width() as usize;
        let height = self.window.height() as usize;

        let start_y = cmp::min(height - 1, y);
        let end_y = cmp::min(height - 1, y + h);

        let start_x = cmp::min(width - 1, x);
        let len = cmp::min(width - 1, x + w) - start_x;

        let mut offscreen_ptr = self.window.data_mut().as_mut_ptr() as usize;

        let stride = width * 4;

        let offset = y * stride + start_x * 4;
        offscreen_ptr += offset;

        let mut rows = end_y - start_y;
        while rows > 0 {
            let mut row_ptr = offscreen_ptr;
            let mut cols = len;
            while cols > 0 {
                unsafe {
                    let color = *(row_ptr as *mut u32);
                    *(row_ptr as *mut u32) = color ^ 0x00FFFFFF;
                }
                row_ptr += 4;
                cols -= 1;
            }
            offscreen_ptr += stride;
            rows -= 1;
        }
    }

    pub fn write(&mut self, buf: &[u8], sync: bool) -> Result<usize> {
        let alpha = self.alpha;
        let cvt = |color: ransid::Color| -> Color {
            Color {
                data: ((alpha as u32) << 24) | (color.as_rgb() & 0xFFFFFF)
            }
        };

        if self.console.state.cursor && self.console.state.x < self.console.state.w && self.console.state.y < self.console.state.h {
            let x = self.console.state.x;
            let y = self.console.state.y;
            let block_width = self.block_width;
            let block_height = self.block_height;
            self.invert(x * block_width, y * block_height, block_width, block_height);
            self.changed.insert(y);
        }

        {
            let font = &self.font;
            let font_bold = &self.font_bold;
            let console_bg = self.console.state.background;
            let console_w = self.console.state.w;
            let console_h = self.console.state.h;
            let block_width = self.block_width;
            let block_height = self.block_height;
            let alt = &mut self.alternate;
            let grid = &mut self.grid;
            let alt_grid = &mut self.alt_grid;
            let window = &mut self.window;
            let input = &mut self.input;
            let changed = &mut self.changed;
            let mut str_buf = [0; 4];
            self.console.write(buf, |event| {
                match event {
                    ransid::Event::Char { x, y, c, color, bold, .. } => {
                        if bold {
                            font_bold.render(&c.encode_utf8(&mut str_buf), block_height as f32).draw(window, x as i32 * block_width as i32, y as i32 * block_height as i32, cvt(color));
                        } else {
                            font.render(&c.encode_utf8(&mut str_buf), block_height as f32).draw(window, x as i32 * block_width as i32, y as i32 * block_height as i32, cvt(color));
                        }

                        if let Some(ref mut block) = grid.get_mut(y * console_w + x) {
                            block.c = c;
                            block.fg = cvt(color);
                            block.bold = bold;
                        }

                        changed.insert(y);
                    },
                    ransid::Event::Input { data } => {
                        input.extend(data);
                    },
                    ransid::Event::Rect { x, y, w, h, color } => {
                        window.mode().set(Mode::Overwrite);
                        window.rect(x as i32 * block_width as i32, y as i32 * block_height as i32, w as u32 * block_width as u32, h as u32 * block_height as u32, cvt(color));
                        window.mode().set(Mode::Blend);

                        for y2 in y..y + h {
                            for x2 in x..x + w {
                                if let Some(ref mut block) = grid.get_mut(y2 * console_w + x2) {
                                    block.c = '\0';
                                    block.bg = cvt(color);
                                }
                            }
                            changed.insert(y2);
                        }
                    },
                    ransid::Event::ScreenBuffer { alternate, clear } => {
                        if *alt != alternate {
                            mem::swap(grid, alt_grid);

                            window.set(cvt(console_bg));

                            for y in 0..console_h {
                                for x in 0..console_w {
                                    let block = &mut grid[y * console_w + x];

                                    if clear {
                                        block.c = '\0';
                                        block.bg = cvt(console_bg);
                                    }

                                    window.mode().set(Mode::Overwrite);
                                    window.rect(x as i32 * block_width as i32, y as i32 * block_height as i32, block_width as u32, block_height as u32, block.bg);
                                    window.mode().set(Mode::Blend);

                                    if block.c != '\0' {
                                        if block.bold {
                                            font_bold.render(&block.c.encode_utf8(&mut str_buf), block_height as f32).draw(window, x as i32 * block_width as i32, y as i32 * block_height as i32, block.fg);
                                        } else {
                                            font.render(&block.c.encode_utf8(&mut str_buf), block_height as f32).draw(window, x as i32 * block_width as i32, y as i32 * block_height as i32, block.fg);
                                        }
                                    }
                                }
                                changed.insert(y as usize);
                            }
                        }
                        *alt = alternate;
                    },
                    ransid::Event::Move {from_x, from_y, to_x, to_y, w, h } => {
                        let width = window.width() as usize;
                        let pixels = window.data_mut();

                        for raw_y in 0..h {
                            let y = if from_y > to_y {
                                raw_y
                            } else {
                                h - raw_y - 1
                            };

                            for pixel_y in 0..block_height {
                                {
                                    let off_from = ((from_y + y) * block_height + pixel_y) * width + from_x * block_width;
                                    let off_to = ((to_y + y) * block_height + pixel_y) * width + to_x * block_width;
                                    let len = w * block_width;

                                    if off_from + len <= pixels.len() && off_to + len <= pixels.len() {
                                        unsafe {
                                            let data_ptr = pixels.as_mut_ptr() as *mut u32;
                                            ptr::copy(data_ptr.offset(off_from as isize), data_ptr.offset(off_to as isize), len);
                                        }
                                    }
                                }
                            }

                            {
                                let off_from = (from_y + y) * console_w + from_x;
                                let off_to = (to_y + y) * console_w + to_x;
                                let len = w;

                                if off_from + len <= grid.len() && off_to + len <= grid.len() {
                                    unsafe {
                                        let data_ptr = grid.as_mut_ptr();
                                        ptr::copy(data_ptr.offset(off_from as isize), data_ptr.offset(off_to as isize), len);
                                    }
                                }
                            }

                            changed.insert(to_y + y);
                        }
                    },
                    ransid::Event::Resize { w, h } => {
                        //TODO: Make sure grid is resized
                        window.set_size(w as u32 * block_width as u32, h as u32 * block_height as u32);
                    },
                    ransid::Event::Title { title } => {
                        window.set_title(&title);
                    }
                }
            });
        }

        if self.console.state.cursor && self.console.state.x < self.console.state.w && self.console.state.y < self.console.state.h {
            let x = self.console.state.x;
            let y = self.console.state.y;
            let block_width = self.block_width;
            let block_height = self.block_height;
            self.invert(x * block_width, y * block_height, block_width, block_height);
            self.changed.insert(y as usize);
        }

        if sync {
            self.sync();
        }

        Ok(buf.len())
    }

    fn resize_grid(&mut self, w: usize, h: usize) {
        let alpha = self.alpha;
        let cvt = |color: ransid::Color| -> Color {
            Color {
                data: ((alpha as u32) << 24) | (color.as_rgb() & 0xFFFFFF)
            }
        };

        if w != self.console.state.w || h != self.console.state.h {
            let mut grid = vec![Block {
                c: '\0', fg: cvt(self.console.state.foreground), bg: cvt(self.console.state.background), bold: false
            }; w * h].into_boxed_slice();

            let mut alt_grid = vec![Block {
                c: '\0', fg: cvt(self.console.state.foreground), bg: cvt(self.console.state.background), bold: false
            }; w * h].into_boxed_slice();

            self.window.set(cvt(self.console.state.background));

            {
                let font = &self.font;
                let font_bold = &self.font_bold;
                let window = &mut self.window;
                let mut str_buf = [0; 4];
                for y in 0..self.console.state.h {
                    for x in 0..self.console.state.w {
                        let block = self.grid[y * self.console.state.w + x];
                        if y < h && x < w {
                            grid[y * w + x] = block;

                            let alt_block = self.alt_grid[y * self.console.state.w + x];
                            alt_grid[y * w + x] = alt_block;
                        }

                        window.mode().set(Mode::Overwrite);
                        window.rect(x as i32 * self.block_width as i32, y as i32 * self.block_height as i32, self.block_width as u32, self.block_height as u32, block.bg);
                        window.mode().set(Mode::Blend);

                        if block.c != '\0' {
                            if block.bold {
                                font_bold.render(&block.c.encode_utf8(&mut str_buf), self.block_height as f32).draw(window, x as i32 * self.block_width as i32, y as i32 * self.block_height as i32, block.fg);
                            } else {
                                font.render(&block.c.encode_utf8(&mut str_buf), self.block_height as f32).draw(window, x as i32 * self.block_width as i32, y as i32 * self.block_height as i32, block.fg);
                            }
                        }
                    }
                    self.changed.insert(y as usize);
                }
            }

            self.console.resize(w, h);
            self.grid = grid;
            self.alt_grid = alt_grid;

            if self.console.state.cursor && self.console.state.x < self.console.state.w && self.console.state.y < self.console.state.h {
                let x = self.console.state.x;
                let y = self.console.state.y;
                let block_width = self.block_width;
                let block_height = self.block_height;
                self.invert(x * block_width, y * block_height, block_width, block_height);
            }
        }
    }

    fn set_block_size(&mut self, block_width: usize) {
        self.block_width = if block_width < 4 { 4 } else if block_width > 48 { 48 } else { block_width };
        self.block_height = self.block_width * 2;
    }

    fn sync(&mut self) {
        /*
        let width = self.window.width;
        for change in self.changed.iter() {
            self.display.sync(0, change * 16, width, 16);
        }
        */
        if ! self.changed.is_empty() {
            self.window.sync();
        }
        self.changed.clear();
    }

    pub fn redraw(&mut self) {
        /*
        let width = self.window.width;
        let height = self.window.height;
        */
        self.window.sync();
        self.changed.clear();
    }
}
