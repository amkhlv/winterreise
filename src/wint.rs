extern crate dirs;
extern crate gdk;
extern crate gdk_sys;
extern crate gio;
extern crate glib;
extern crate gtk;
extern crate xcb_wm;

use crate::gdk::prelude::{ApplicationExt, ApplicationExtManual};
use glib::clone;
use glib::signal::Propagation;
use gtk::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use xcb::x::Window;

use winterreise::{check_css, get_conf, get_config_dir, get_wm_data, make_vbox, Config};

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;

#[derive(Debug, Deserialize)]
struct WindowSimple {
    #[serde(rename = "@nick", default)]
    pub nick: String,
    #[serde(rename = "@geometry", default)]
    pub geometry: String,
}

#[derive(Debug, Deserialize)]
struct Display {
    #[serde(rename = "@resolution", default)]
    pub resolution: String,

    #[serde(rename = "window", default)]
    pub windows: Vec<WindowSimple>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "displays", default)]
struct Displays {
    #[serde(rename = "display", default)]
    pub items: Vec<Display>,
}

fn get_geometry(xml_path: &PathBuf, nick: String, geom: &String) -> Option<Vec<u32>> {
    let tilings: Displays = serde_xml_rs::from_reader(File::open(xml_path).unwrap()).unwrap();
    let x = tilings
        .items
        .iter()
        .filter(|disp| &disp.resolution == geom)
        .next()
        .unwrap();
    x.windows
        .iter()
        .filter(|w| w.nick == nick)
        .next()
        .map(|ni| {
            ni.geometry
                .split(",")
                .map(|s| str::parse::<u32>(s).unwrap())
                .collect()
        })
}

fn do_resize(xconn: &xcb::Connection, wid: Window, g: &Vec<u32>) {
    let req = xcb::x::ConfigureWindow {
        window: wid,
        value_list: &[
            xcb::x::ConfigWindow::X(g[0] as i32),
            xcb::x::ConfigWindow::Y(g[1] as i32),
            xcb::x::ConfigWindow::Width(g[2] as u32),
            xcb::x::ConfigWindow::Height(g[3] as u32),
        ],
    };
    let cookie = xconn.send_request_checked(&req);
    match xconn.check_request(cookie) {
        Ok(_) => println!("Resized window {:?} to {:?}", wid, g),
        Err(e) => println!("Error resizing window {:?}: {:?}", wid, e),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = get_config_dir();
    let conf: Config = get_conf().expect("Could not read the configuration file");
    let maxlen = conf.maxwidth;
    let blacklist = Rc::new(conf.blacklist);
    let space_between_buttons = conf.space_between_buttons;
    let xml_path = Path::join(&config_dir, "tilings.xml");
    let xml_path = Rc::new(xml_path);
    let (wins, geom, desktop, active) = get_wm_data();

    let application = gtk::Application::builder()
        .application_id("com.andreimikhailov.winterreise")
        .build();
    let css = Path::join(&config_dir, "style.css");
    check_css(&css);
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
        let screen = gdk::Screen::default();
        match screen {
            Some(scr) => { gtk::StyleContext::add_provider_for_screen(&scr, &provider, 799); }
            _ => ()
        };
        let window = gtk::ApplicationWindow::new(app);
        window.set_title("Tile");
        window.set_type_hint(gdk::WindowTypeHint::Dialog);
        window.style_context().add_class("main_window_tile");
        window.connect_key_press_event(clone!(@weak app => @default-return Propagation::Proceed, move |_w,e| {
            let keyval = e.keyval();
            let _keystate = e.state();
            if *keyval == gdk_sys::GDK_KEY_Escape as u32 {
                app.quit();
                return Propagation::Stop;
            } else { return Propagation::Proceed; }
        }));

        let (vbox, charhints) = make_vbox(&wins, Some(desktop), space_between_buttons, maxlen, &blacklist, &active);
        window.add(&vbox);
        let entry = gtk::Entry::new();
        entry.style_context().add_class("wmjump_cmd_entry");
        let geom1 = Rc::clone(&geom);
        println!("Geometry={:?}", geom1);
        let xml_path = Rc::clone(&xml_path);
        entry.connect_activate(clone!(@weak entry, @weak app => move |_| {
            let command : String = entry.text().to_string();
            let tilings : Vec<(xcb::x::Window, Option<Vec<u32>>)> = command.split(" ").map(|com| {
                let mut it = com.chars();
                let charhint = it.next().unwrap();
                let wid = *charhints.get(&(charhint as u8 - 97 as u8)).unwrap();
                let tiling = it.collect::<String>();
                let mg = get_geometry(&xml_path, tiling, &geom1);
                return (wid, mg)
            }).collect();
            app.quit();
            let (xcb_conn, _screen_id) = xcb::Connection::connect(None).expect("XCB connection failed");
            for (wid, mg) in tilings.iter() {
                match mg {
                    Some(g) => do_resize(&xcb_conn, *wid, &g),
                    None => ()
                }
            }
        }));
        vbox.add(&entry);
        entry.grab_focus();
        window.show_all();
    });
    application.run();
    Ok(())
}
