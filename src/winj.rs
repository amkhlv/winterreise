extern crate gdk;
extern crate gio;
extern crate dirs;
extern crate gdk_sys;
extern crate xcb;
extern crate xcb_util;
extern crate clap;

use clap::{Arg, App};
use std::rc::Rc;
use std::cell::RefCell;
use gtk::prelude::*;
use gio::prelude::*;
use glib::clone;
use glib::signal::Inhibit;
use dirs::home_dir;
use std::path::Path;
use std::io::{Write,BufRead};
use xcb_util::ewmh;

use winterreise::{Config, TMPFile, get_conf, get_wm_data, make_vbox, go_to_window};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clops = App::new("wmjump")
        .author("Andrei Mikhailov")
        .about("Window navigation")
        .arg(Arg::with_name("current")
             .help("only show windows on the current desktop")
             .short("c"))
        .get_matches();
    let config_dir = Path::join(Path::new(&home_dir().unwrap()), ".config/winterreise/");
    let conf: Config = get_conf()?;
    let maxlen = conf.maxwidth;
    let blacklist = Rc::new(conf.blacklist);
    let tmpfilename = match conf.tmpfile {
            TMPFile::Custom(x) => format!("{}",x),
            TMPFile::InXdgRuntime => 
                match std::env::vars().into_iter().filter(|(k,_v)| k == "XDG_RUNTIME_DIR").next() {
                    Some(x) => format!("{}/winterreise", x.1),
                    None => panic!("system does not have XDG_RUNTIME_DIR")
                },
            TMPFile::InTmp => String::from("/tmp/winterreise")
        };
    let tmpfile = std::fs::OpenOptions::new().read(true).open(&tmpfilename)
        .unwrap_or_else(|_e| std::fs::OpenOptions::new().write(true).create(true).open(&tmpfilename).unwrap());
    let prev_win = match std::io::BufReader::new(&tmpfile).lines().into_iter().next() {
        Some(Ok(x)) => {
            match x.parse::<u32>() { Ok(w) => Some(w) , _ => None }
        },
        _ => None
    };
    let tmpfile = std::fs::OpenOptions::new().read(true).write(true).truncate(true).create(true).open(&tmpfilename).unwrap();
    let tmpfile = Rc::new(RefCell::new(tmpfile)); 
    let delay = conf.delay;
    let space_between_buttons = conf.space_between_buttons;
    let attempts = conf.attempts;
    let (wins, geom, desktop, active) = get_wm_data()?;

    let application = gtk::Application::new(
        Some("com.andreimikhailov.winterreise"),
        Default::default(),
        ).expect("failed to initialize GTK application");
    let css = Path::join(&config_dir, "style.css");
    application.connect_activate(move |app| {
        let provider = gtk::CssProvider::new();
        match css.to_str() {
            Some(x) => {
                match provider.load_from_path(x) {
                    Ok(_) => (),
                    Err(x) => { println!("ERROR: {:?}", x); }
                }
            }
            None => { println!("ERROR: CSS file not found") ; }
        };
        let screen = gdk::Screen::get_default();
        match screen {
            Some(scr) => { gtk::StyleContext::add_provider_for_screen(&scr, &provider, 799); }
            _ => ()
        };
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Jump to...");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.get_style_context().add_class(if clops.is_present("current") { "main_window_currentonly" } else { "main_window" });
        window.connect_focus_out_event(clone!(@weak app => @default-return Inhibit(false), move |_w,_e| { app.quit(); return Inhibit(true); }));
        let (vbox, charhints) = make_vbox(
            &wins, 
            if clops.is_present("current") { Some(desktop) } else { None }, 
            space_between_buttons,
            maxlen,
            &blacklist,
            &active
            );
        window.add(&vbox);
        let hints = Rc::new(charhints);
        let tmpfile = tmpfile.clone();
        window.connect_key_press_event(clone!(@weak app => @default-return Inhibit(false), move |_w,e| {
            let keyval = e.get_keyval(); 
            let _keystate = e.get_state();
            if *keyval == gdk_sys::GDK_KEY_Escape as u32 {
                match prev_win {
                    Some(x) => { tmpfile.borrow_mut().write(&format!("{}",x).into_bytes()[..])  ; () }
                    None => ()
                }
                app.quit();
                return Inhibit(true);
            }
            if *keyval == gdk_sys::GDK_KEY_space as u32 {
                app.quit();
                match prev_win {
                    Some(w) =>  {
                        println!("-- previous window was {:#x}",w);
                        let (xcb_conn, screen_id) = xcb::Connection::connect(None).unwrap();
                        let ewmh_conn = ewmh::Connection::connect(xcb_conn).map_err(|(e, _)| e).unwrap();
                        go_to_window(w, screen_id, attempts, delay, &ewmh_conn);
                        tmpfile.borrow_mut().write(&format!("{}",active).into_bytes()[..]);
                    }
                    None => ()
                }
                return Inhibit(true);
            }
            let a = (format!("{}",*keyval)).parse::<u8>();
            match a {
                Ok(aa) => {
                    app.quit();
                    let (xcb_conn, screen_id) = xcb::Connection::connect(None).unwrap();
                    let ewmh_conn = ewmh::Connection::connect(xcb_conn).map_err(|(e, _)| e).unwrap();
                    let mut dt = delay;
                    if aa < 97 && aa > 48 {
                        tmpfile.borrow_mut().write(&format!("{}",active).into_bytes()[..]);
                        for _t in 0..attempts {
                            let new_desktop = (aa - 49) as u32;
                            std::thread::sleep(std::time::Duration::from_millis(dt));
                            let cd = ewmh::get_current_desktop(&ewmh_conn, screen_id).get_reply().unwrap();
                            if cd == new_desktop { 
                                println!("-- Welcome to desktop {} !", new_desktop) ; 
                                break 
                            } else { 
                                println!("-- going to desktop {}\n   ...", new_desktop); 
                                dt = dt * 2; 
                            }
                            ewmh::request_change_current_desktop(&ewmh_conn, screen_id, ( aa - 49 ) as u32, 0);
                            ewmh_conn.flush();
                        }
                        return Inhibit(true);
                    } else  if let Some(s) = &hints.get(&(aa - 97)) {
                        tmpfile.borrow_mut().write(&format!("{}",active).into_bytes()[..]);
                        go_to_window(**s, screen_id, attempts, delay, &ewmh_conn);
                        return Inhibit(true);
                    } else {
                        return Inhibit(false);
                    }
                },
                _ => { return Inhibit(false); }
            }
        }));
        window.show_all();
    });
    application.run(&[]);
    Ok(())
}
