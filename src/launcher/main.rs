extern crate orbclient;

use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::process::Command;
use std::thread;

use orbclient::{BmpFile, Color, EventOption, Window};

use package::Package;

pub mod package;

fn draw(window: &mut Window, packages: &Vec<Box<Package>>, shutdown: &BmpFile, mouse_x: i32, mouse_y: i32){
    window.set(Color::rgba(0, 0, 0, 0));
    let w = window.width();
    window.rect(0, 16, w, 32, Color::rgba(0, 0, 0, 128));

    let mut x = 0;
    for package in packages.iter() {
        if package.icon.has_data() {
            let y = window.height() as isize - package.icon.height() as isize;

            if mouse_y >= y as i32 && mouse_x >= x && mouse_x < x + package.icon.width() as i32 {
                window.rect(x as i32, y as i32,
                                  package.icon.width() as u32, package.icon.height() as u32,
                                  Color::rgba(128, 128, 128, 128));

                window.rect(x as i32, y as i32 - 16,
                    package.name.len() as u32 * 8, 16,
                    Color::rgba(0, 0, 0, 128));

                let mut c_x = x;
                for c in package.name.chars() {
                    window.char(c_x as i32, y as i32 - 16, c, Color::rgb(255, 255, 255));
                    c_x += 8;
                }
            }

            window.image(x as i32, y as i32,
                        package.icon.width() as u32,
                        package.icon.height() as u32,
                        &package.icon);
            x = x + package.icon.width() as i32;
        }
    }

    if shutdown.has_data() {
        x = window.width() as i32 - shutdown.width() as i32;
        let y = window.height() as isize - shutdown.height() as isize;

        if mouse_y >= y as i32 && mouse_x >= x &&
           mouse_x < x + shutdown.width() as i32 {
            window.rect(x as i32, y as i32,
                              shutdown.width() as u32, shutdown.height() as u32,
                              Color::rgba(128, 128, 128, 128));
        }

        window.image(x as i32, y as i32,
                        shutdown.width() as u32, shutdown.height() as u32,
                        &shutdown);
        x = x + shutdown.width() as i32;
    }

    window.sync();
}

fn main() {
    let mut packages: Vec<Box<Package>> = Vec::new();

    //TODO: Use a directory walk
    for entry_result in fs::read_dir("apps/").unwrap() {
        let entry = entry_result.unwrap();
        if entry.file_type().unwrap().is_dir() {
            packages.push(Package::from_path(&("apps/".to_string() + entry.file_name().to_str().unwrap())));
        }
    }

    let paths = env::args().skip(1);
    if paths.len() > 0 {
        for ref path in paths {
            for package in packages.iter() {
                let mut accepted = false;
                for accept in package.accepts.iter() {
                    if (accept.starts_with('*') &&
                        path.ends_with(&accept[1 ..])) ||
                       (accept.ends_with('*') &&
                        path.starts_with(&accept[.. accept.len() - 1])) {
                        accepted = true;
                        break;
                    }
                }
                if accepted {
                    if let Err(err) = Command::new(&package.binary).arg(&path).spawn() {
                        println!("launcher: failed to launch '{}': {}", package.binary, err);
                    }
                    break;
                }
            }
        }
    } else {
        let shutdown = BmpFile::from_path("ui/actions/system-shutdown.bmp");
        if ! shutdown.has_data() {
            println!("Failed to read shutdown icon");
        }

        let mut window = Window::new(0, 600 - 48, 800, 48, "").unwrap();

        draw(&mut window, &packages, &shutdown, -1, -1);
        'running: loop {
            for event in window.events() {
                match event.to_option() {
                    EventOption::Mouse(mouse_event) => {
                        draw(&mut window, &packages, &shutdown, mouse_event.x, mouse_event.y);

                        if mouse_event.left_button {
                            let mut x = 0;
                            for package in packages.iter() {
                                if package.icon.has_data() {
                                    let y = window.height() as i32 - package.icon.height() as i32;
                                    if mouse_event.y >= y && mouse_event.x >= x &&
                                       mouse_event.x < x + package.icon.width() as i32 {
                                        if let Err(err) = Command::new(&package.binary).spawn() {
                                            println!("{}: Failed to launch: {}", package.binary, err);
                                        }
                                    }
                                    x = x + package.icon.width() as i32;
                                }
                            }

                            if shutdown.has_data() {
                                x = window.width() as i32 - shutdown.width() as i32;
                                let y = window.height() as i32 - shutdown.height() as i32;
                                if mouse_event.y >= y && mouse_event.x >= x &&
                                   mouse_event.x < x + shutdown.width() as i32 {
                                       File::create("acpi:off");
                                       break 'running;
                                }
                            }
                        }
                    },
                    EventOption::Quit(_) => break 'running,
                    _ => ()
                }
            }

            thread::yield_now();
        }
    }
}
