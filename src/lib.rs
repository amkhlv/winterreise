#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;
extern crate xcb_util;

use std::fs::File;
use std::rc::Rc;
use gtk::prelude::*;
use dirs::home_dir;
use std::collections::HashMap;
use std::path::Path;
use xcb_util::{ewmh,icccm};

#[derive(Debug)]
pub enum WintError {
    //Errors from external libs:
    SerDe(serde_xml_rs::Error),
    NoConfigFile(std::io::Error),
    XCBConnError(xcb::ConnError),
    XCBError(xcb::Error<xcb::ffi::xcb_generic_error_t>),
}
impl std::fmt::Display for WintError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            WintError::SerDe(ref err) => err.fmt(f),
            WintError::NoConfigFile(ref err) => err.fmt(f),
            WintError::XCBConnError(ref err) => err.fmt(f),
            WintError::XCBError(ref err) => err.fmt(f),
        }
    }
}
impl std::error::Error for WintError {}
impl std::convert::From<serde_xml_rs::Error> for WintError {
    fn from(err: serde_xml_rs::Error) -> WintError {
        WintError::SerDe(err)
    }
}
impl std::convert::From<std::io::Error> for WintError {
    fn from(err: std::io::Error) -> WintError {
        WintError::NoConfigFile(err)
    }
}
impl std::convert::From<xcb::ConnError> for WintError {
    fn from(err: xcb::ConnError) -> WintError {
        WintError::XCBConnError(err)
    }
}
impl std::convert::From<xcb::Error<xcb::ffi::xcb_generic_error_t>> for WintError {
    fn from(err: xcb::Error<xcb::ffi::xcb_generic_error_t>) -> WintError {
        WintError::XCBError(err)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct BlacklistedItem {
    pub class: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct BlacklistedItems {
    pub item:Vec<BlacklistedItem>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum TMPFile {
#[serde(rename = "in_xdg_runtime")]
    InXdgRuntime,
#[serde(rename = "in_tmp")]
    InTmp,
#[serde(rename = "custom")]
    Custom(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename = "configuration")]
pub struct Config {
    pub tmpfile: TMPFile,
    pub delay: u64,
    #[serde(rename = "spaceBetweenButtons", default)]
    pub space_between_buttons: i32,
    pub maxwidth: usize,
    pub attempts: u8,
    pub blacklist: BlacklistedItems,
}

pub struct WM {
    pub wins: Rc<Vec<(u32,u32,String,String)>>,
    pub desktop: u32,
}

pub fn get_wm_data() -> Result<(Rc<Vec<(u32,u32,String,String)>>, Rc<String>, u32, u32), WintError> {
    let (xcb_conn, screen_id) = xcb::Connection::connect(None)?;
    let ewmh_conn = ewmh::Connection::connect(xcb_conn).map_err(|(e, _)| e)?;
    let (xcb_conn1, _) = xcb::Connection::connect(None)?;
    let (desktop_width,desktop_height) = ewmh::get_desktop_geometry(&ewmh_conn, screen_id).get_reply()?;
    let geom = Rc::new(String::from(format!("{}x{}",desktop_width,desktop_height)));
    println!("DESKTOP GEOMETRY:{}x{}", desktop_width, desktop_height);
    let active = ewmh::get_active_window(&ewmh_conn, screen_id).get_reply().unwrap();
    let clients = ewmh::get_client_list(&ewmh_conn, screen_id);
    let clients = clients.get_reply()?;
    let wins: Rc<Vec<(u32,u32,String,String)>> = 
        Rc::new(clients
                .windows()
                .iter()
                .map(|w| {
                    let xconn = xcb::base::Connection::connect(None).unwrap();
                    let a = xcb::intern_atom(&xconn.0, false, "_NET_WM_NAME").get_reply().unwrap();
                    let u = xcb::intern_atom(&xconn.0, false, "UTF8_STRING").get_reply().unwrap();
                    let dtop = ewmh::get_wm_desktop(&ewmh_conn, *w).get_reply().unwrap();
                    let enc = icccm::get_wm_name(&ewmh_conn, *w).get_reply().unwrap().encoding();
                    let nm = format!("{}",String::from_utf8(xcb::get_property(&xconn.0, false, *w, a.atom(), u.atom(), 0, std::u32::MAX).get_reply().unwrap().value::<u8>().to_vec()).unwrap_or(String::from("???")));
                    //let nm = String::from(icccm::get_wm_name(&xcb_conn1, *w).get_reply().unwrap().name());
                    let class = String::from(icccm::get_wm_class(&xcb_conn1, *w).get_reply().unwrap().instance());
                    if enc != xcb::ATOM_STRING { println!("{:#x} COMPOUND_TEXT ENC:{},NAME:{},CLASS:{}", w, enc, &nm, &class); }
                    (*w, dtop, nm, class)
                }).collect());
    let desktop = ewmh::get_current_desktop(&ewmh_conn, screen_id).get_reply()?;
    return Ok((wins, geom, desktop, active));
}

pub fn abbreviate(x: String, maxlen: usize) -> String {
    let chars = x.chars().collect::<Vec<_>>();
    let len = chars.len();
    if len < maxlen { 
        return x; 
    } else { 
        return format!(
            "{}...{}", 
            &chars[..(maxlen/8)*4].iter().cloned().collect::<String>(),
            &chars[(len - (maxlen/8)*4)..len].iter().cloned().collect::<String>()
            );
    }
}
pub fn make_vbox(
    wins: &Rc<Vec<(u32,u32,String,String)>>, 
    desktop: Option<u32>, 
    space_between_buttons: i32, 
    maxlen: usize, 
    blacklist: &Rc<BlacklistedItems>,
    active: &u32
    ) -> (gtk::Box, HashMap<u8, u32>) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical,space_between_buttons);
    vbox.get_style_context().add_class("main_vbox");
    let mut charhints : HashMap<u8, u32> = HashMap::new();
    let mut j = 0 as u8;
    match desktop {
        Some(d) => println!("only showing windows on desktop {}",d),
        None => println!("showing windows on all desktops")
    }
    for (num, win_desktop, name, class) in 
        (*wins)
            .iter()
            .filter(|win| match desktop { Some(d) => d == win.1, None => true }) 
            .filter(|win| !(*blacklist).item.iter().map(|i| &i.class).collect::<Vec<&String>>().contains(&&win.3))
            {
                let hbox = gtk::Box::new(gtk::Orientation::Horizontal,space_between_buttons);
                let lbtn = gtk::Button::new();
                let llbl = gtk::Label::new(Some(&format!("{}", (j+97) as char)));
                if num == active {
                    lbtn.get_style_context().add_class("wmjump_lbtn_current");
                } else {
                    lbtn.get_style_context().add_class(&["wbtn_", class].concat()[..]);
                    lbtn.get_style_context().add_class("wmjump_lbtn");
                }
                lbtn.add(&llbl);
                let rbtn = gtk::Button::new();
                let rlbl = gtk::Label::new(Some(&format!("{}", (j+97) as char)));
                if num == active {
                    rbtn.get_style_context().add_class("wmjump_rbtn_current");
                } else {
                    rbtn.get_style_context().add_class(&["wbtn_", class].concat()[..]);
                    rbtn.get_style_context().add_class("wmjump_rbtn");
                }
                rbtn.add(&rlbl);
                let btn = gtk::Button::new();
                let truncated = name.clone();
                let lbl = gtk::Label::new(Some(&format!("{}: {}", win_desktop + 1, abbreviate(truncated,maxlen))));
                btn.get_style_context().add_class(&["wbtn_", class].concat()[..]);
                btn.get_style_context().add_class("wmjump_button");
                btn.add(&lbl);
                hbox.add(&lbtn);
                hbox.add(&btn);
                hbox.add(&rbtn);
                vbox.add(&hbox);
                charhints.insert(j, *num);
                j += 1;
            }
    return (vbox, charhints);
}

pub fn get_conf() -> Result<Config, WintError> {
    let config_dir = Path::join(Path::new(&home_dir().unwrap()), ".config/winterreise/");
    let config_file = File::open(Path::join(&config_dir, "config.xml"))?;
    let conf = serde_xml_rs::from_reader(config_file)?;
    return Ok(conf);
}

pub fn go_to_window(win: u32, screen_id: i32, attempts: u8, mut delay: u64, ewmh_conn: &ewmh::Connection) {
    let dtop = ewmh::get_wm_desktop(&ewmh_conn, win).get_reply().unwrap();
    let curwin = ewmh::get_active_window(&ewmh_conn, screen_id).get_reply().unwrap();
    //println!("Running: {}",s);
    //f.write(format!("{:#x}", s));
    //ewmh::set_wm_desktop(&ewmh_conn, **s, dtop);
    ewmh::request_change_current_desktop(&ewmh_conn, screen_id, dtop, 0);
    for _t in 0..attempts {
        std::thread::sleep(std::time::Duration::from_millis(delay));
        let cw = ewmh::get_active_window(&ewmh_conn, screen_id).get_reply().unwrap();
        if cw == win { 
            println!("-- Welcome to window {:#x} !", win); 
            break; 
        } else { 
            println!("-- going to window {:#x}\n   ...", win); 
            delay = delay * 2; 
        }
        ewmh::request_change_active_window(
            &ewmh_conn,
            screen_id,
            win,
            ewmh::CLIENT_SOURCE_TYPE_NORMAL,
            0,
            curwin
            );
        ewmh_conn.flush();
    }
    ewmh_conn.flush();
}
