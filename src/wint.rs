extern crate gtk;
extern crate glib;
extern crate gdk;
extern crate gio;
extern crate dirs;
extern crate gdk_sys;
extern crate xcb_util;

use std::fs::File;
use std::rc::Rc;
use gtk::prelude::*;
use gio::prelude::*;
use glib::clone;
use glib::signal::Inhibit;
use dirs::home_dir;
use std::path::{Path,PathBuf};
use xcb_util::ewmh;

use winterreise::{Config, get_conf, get_config_dir, get_wm_data, make_vbox};

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;

#[derive(Debug, Deserialize)]
struct Window {
    pub nick: String,
    pub geometry: String
}

#[derive(Debug, Deserialize)]
struct Display {
    pub resolution: String,

    #[serde(rename = "window", default)]
        pub windows: Vec<Window>
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "displays", default)]
struct Displays {
    #[serde(rename = "display", default)]
    pub items: Vec<Display>
}

fn get_geometry(xml_path: &PathBuf, nick:String, geom: &String) -> Option<Vec<u32>> {
    let tilings : Displays = serde_xml_rs::from_reader(File::open(xml_path).unwrap()).unwrap();
    let x = tilings.items.iter().filter(|disp| &disp.resolution == geom).next().unwrap();
    x.windows.iter().filter(|w| w.nick == nick).next()
        .map(|ni| ni.geometry.split(",").map(|s| str::parse::<u32>(s).unwrap()).collect())
}

fn do_resize(conn: &ewmh::Connection, scid: i32, wid: u32, g: &Vec<u32>) {
            ewmh::request_move_resize_window(
                &conn,
                scid,
                wid,
                xcb::GRAVITY_STATIC,
                ewmh::CLIENT_SOURCE_TYPE_NORMAL,
                ewmh::MOVE_RESIZE_WINDOW_X | ewmh::MOVE_RESIZE_WINDOW_Y | ewmh::MOVE_RESIZE_WINDOW_HEIGHT | ewmh::MOVE_RESIZE_WINDOW_WIDTH,
                g[0],
                g[1],
                g[2],
                g[3]
                );
            conn.flush();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir();
    let conf: Config = get_conf()?;
    let maxlen = conf.maxwidth;
    let blacklist = Rc::new(conf.blacklist);
    let delay = conf.delay;
    let space_between_buttons = conf.space_between_buttons;
    let attempts = conf.attempts;
    let xml_path = Path::join(&config_dir, "tilings.xml");
    let xml_path = Rc::new(xml_path);
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
        window.set_title("Tile");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.get_style_context().add_class("main_window_tile");
        window.connect_key_press_event(clone!(@weak app => @default-return Inhibit(false), move |_w,e| {
            let keyval = e.get_keyval();
            let _keystate = e.get_state();
            if *keyval == gdk_sys::GDK_KEY_Escape as u32 {
                app.quit();
                return Inhibit(true);
            } else { return Inhibit(false); }
        }));

        let (vbox, charhints) = make_vbox(&wins, Some(desktop), space_between_buttons, maxlen, &blacklist, &active);
        window.add(&vbox);
        let entry = gtk::Entry::new();
        entry.get_style_context().add_class("wmjump_cmd_entry");
        let geom1 = Rc::clone(&geom);
        let xml_path = Rc::clone(&xml_path);
        entry.connect_activate(clone!(@weak entry, @weak app => move |_| {
            let command : String = entry.get_text().to_string();
            let tilings : Vec<(u32, Option<Vec<u32>>)> = command.split(" ").map(|com| {
                let mut it = com.chars();
                let charhint = it.next().unwrap();
                let wid = *charhints.get(&(charhint as u8 - 97 as u8)).unwrap();
                let tiling = it.collect::<String>();
                let mg = get_geometry(&*xml_path, tiling, &*geom1);
                return (wid, mg)
            }).collect();
            app.quit();
            let (xcb_conn_2, screen_id) = xcb::Connection::connect(None).unwrap();
            let ewmh_conn_2 = ewmh::Connection::connect(xcb_conn_2).map_err(|(e, _)| e).unwrap();
            for _t in 0..attempts {
                std::thread::sleep(std::time::Duration::from_millis(delay));
                for (wid, mg) in tilings.iter() {
                    match mg {
                        Some(g) => do_resize(&ewmh_conn_2, screen_id, *wid, &g),
                        None => ()
                    }
                }
            }
        }));
        vbox.add(&entry);
        entry.grab_focus();
        window.show_all();
    });
    application.run(&[]);
    Ok(())
}
