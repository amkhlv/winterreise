extern crate clap;
extern crate dirs;
extern crate gdk;
extern crate gdk_sys;
extern crate gio;
extern crate xcb;
extern crate xcb_wm;

use clap::{App, Arg};
use dirs::home_dir;
use gio::prelude::*;
use glib::clone;
use glib::signal::Propagation;
use gtk::prelude::*;
use gtk::{glib, Application};

use crate::gdk::prelude::{ApplicationExt, ApplicationExtManual};
use crate::xcb::Xid;
use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::path::Path;
use std::rc::Rc;
use winterreise::{
    check_css, get_conf, get_config_dir, get_wm_data, go_to_window, make_vbox, Config, TMPFile,
};
use xcb_wm::ewmh;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let clops = App::new("wmjump")
        .author("Andrei Mikhailov")
        .about("Window navigation")
        .arg(
            Arg::with_name("current")
                .help("only show windows on the current desktop")
                .short("c"),
        )
        .get_matches();
    let config_dir = get_config_dir();
    let conf: Config = get_conf()?;
    let maxlen = conf.maxwidth;
    let tmpfilename = match conf.tmpfile {
        TMPFile::Custom(x) => format!("{}", x),
        TMPFile::InXdgRuntime => match std::env::vars()
            .into_iter()
            .filter(|(k, _v)| k == "XDG_RUNTIME_DIR")
            .next()
        {
            Some(x) => format!("{}/winterreise", x.1),
            None => panic!("system does not have XDG_RUNTIME_DIR"),
        },
        TMPFile::InTmp => String::from("/tmp/winterreise"),
    };
    let tmpfile = std::fs::OpenOptions::new()
        .read(true)
        .open(&tmpfilename)
        .unwrap_or_else(|_e| {
            std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .open(&tmpfilename)
                .unwrap()
        });
    let prev_win = match std::io::BufReader::new(&tmpfile).lines().into_iter().next() {
        Some(Ok(x)) => match x.parse::<u32>() {
            Ok(w) => Some(w),
            _ => None,
        },
        _ => None,
    };
    let tmpfile = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(true)
        .create(true)
        .open(&tmpfilename)
        .unwrap();
    let tmpfile = Rc::new(RefCell::new(tmpfile));
    let delay = conf.delay;
    let space_between_buttons = conf.space_between_buttons;
    let attempts = conf.attempts;

    let application = gtk::Application::builder()
        .application_id("com.andreimikhailov.winterreise")
        .build();
    let css = Path::join(&config_dir, "style.css");
    check_css(&css);
    let blacklist = Rc::new(conf.blacklist);
    application.connect_activate(move |app| {
        let (wins, geom, desktop, active) = get_wm_data();
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
        let screen = gdk::Screen::default();
        match screen {
            Some(scr) => { gtk::StyleContext::add_provider_for_screen(&scr, &provider, 799); }
            _ => ()
        };
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Jump to...");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.style_context().add_class(if clops.is_present("current") { "main_window_currentonly" } else { "main_window" });
        window.connect_focus_out_event(clone!(@weak app => @default-return Propagation::Proceed, move |_w,_e| { app.quit(); return Propagation::Stop; }));
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
        window.connect_key_press_event(clone!(@weak app => @default-return Propagation::Proceed, move |_w,e| {
            let keyval = e.keyval();
            let _keystate = e.state();
            if *keyval == gdk_sys::GDK_KEY_Escape as u32 {
                match prev_win {
                    Some(x) => { tmpfile.borrow_mut().write(&format!("{}",x).into_bytes()[..]).expect("failed writing to tmpfile")  ; () }
                    None => ()
                }
                app.quit();
                return Propagation::Stop;
            }
            if *keyval == gdk_sys::GDK_KEY_space as u32 {
                app.quit();
                match prev_win {
                    Some(w) =>  {
                        println!("-- previous window was {:#x}",w);
                        let (xcb_conn, screen_id) = xcb::Connection::connect(None).expect("XCB connection failed");
                        let ewmh_conn = ewmh::Connection::connect(&xcb_conn);
                        wins.iter().find(|x| x.0.resource_id() == w).map(|active| {
                            println!("-- going to window {:#x} on screen {}", w, screen_id);
                            go_to_window(active.0, screen_id as u32, delay, &ewmh_conn);
                        });
                        tmpfile.borrow_mut().write(&format!("{}",active.resource_id()).into_bytes()[..]).expect("failed writing to tmpfile");
                    }
                    None => ()
                }
                return Propagation::Stop;
            }
            let a = (format!("{}",*keyval)).parse::<u8>();
            match a {
                Ok(aa) => {
                    app.quit();
                    let (xcb_conn, screen_id) = xcb::Connection::connect(None).expect("XCB connection failed");
                    let ewmh_conn = ewmh::Connection::connect(&xcb_conn);
                    let mut dt = delay;
                    if aa < 97 && aa > 48 {
                        tmpfile.borrow_mut().write(&format!("{}",active.resource_id()).into_bytes()[..]).expect("failed writing to tmpfile");
                        let new_desktop = (aa - 49) as u32;
                        std::thread::sleep(std::time::Duration::from_millis(dt));
                        let dtop_req = ewmh::proto::GetCurrentDesktop;
                        let dtop_cookie = ewmh_conn.send_request(&dtop_req);
                        let dtop_repl = ewmh_conn
                            .wait_for_reply(dtop_cookie)
                            .expect("Failed to get current desktop");
                        let cd = dtop_repl.desktop;
                        if cd == new_desktop {
                            println!("-- Welcome to desktop {} !", new_desktop) ;
                        } else {
                            println!("-- going to desktop {}\n   ...", new_desktop);
                            dt = dt * 2;
                        }
                        let chdt_req = ewmh::proto::SendCurrentDesktop::new(&ewmh_conn, new_desktop);
                        ewmh_conn.send_and_check_request(&chdt_req).expect("Failed to change desktop");
                        return Propagation::Stop;
                    } else  if let Some(s) = &hints.get(&(aa - 97)) {
                        tmpfile.borrow_mut().write(&format!("{}",active.resource_id()).into_bytes()[..]).expect("failed to write to tmpfile");
                        go_to_window(**s, screen_id as u32, delay, &ewmh_conn);
                        return Propagation::Stop;
                    } else {
                        return Propagation::Proceed;
                    }
                },
                _ => { return Propagation::Proceed; }
            }
        }));
        window.show_all();
    });
    let empty: Vec<String> = vec![];

    application.run_with_args(&empty);
    Ok(())
}
