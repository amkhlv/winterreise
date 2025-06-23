#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_xml_rs;
extern crate xcb_wm;

use dirs::home_dir;
use gtk::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use xcb::{x::Window, Connection};
use xcb_wm::{ewmh, icccm};

#[derive(Debug)]
pub enum WintError {
    //Errors from external libs:
    SerDe(serde_xml_rs::Error),
    NoConfigFile(std::io::Error),
    XCBConnError(xcb::ConnError),
    XCBError(xcb::Error),
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
impl std::convert::From<xcb::Error> for WintError {
    fn from(err: xcb::Error) -> WintError {
        WintError::XCBError(err)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct BlacklistedItem {
    pub class: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct BlacklistedItems {
    pub item: Vec<BlacklistedItem>,
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
    pub wins: Rc<Vec<(u32, u32, String, String)>>,
    pub desktop: u32,
}

pub fn get_wm_data() -> (
    Rc<Vec<(Window, u32, String, String)>>,
    Rc<String>,
    u32,
    Window,
) {
    let (xcb_conn, screen_id) =
        xcb::Connection::connect(None).expect("XCB connection failed in get_wm_data");
    let ewmh_conn = ewmh::Connection::connect(&xcb_conn);
    let icccm_conn = icccm::Connection::connect(&xcb_conn);
    let (xcb_conn1, _) =
        xcb::Connection::connect(None).expect("XCB connection failed in get_wm_data");
    let geom_req = ewmh::proto::GetDesktopGeometry;
    let geom_cookie = ewmh_conn.send_request(&geom_req);
    let geom_repl = ewmh_conn
        .wait_for_reply(geom_cookie)
        .expect("Failed to get desktop geometry");
    let (desktop_width, desktop_height) = (geom_repl.width, geom_repl.height);
    let geom = Rc::new(String::from(format!(
        "{}x{}",
        desktop_width, desktop_height
    )));
    println!("DESKTOP GEOMETRY:{}x{}", desktop_width, desktop_height);
    let active_win_req = ewmh::proto::GetActiveWindow;
    let active_win_cookie = ewmh_conn.send_request(&active_win_req);
    let active_win_repl = ewmh_conn
        .wait_for_reply(active_win_cookie)
        .expect("Failed to get active window");
    let active = active_win_repl.window;
    let clients_req = ewmh::proto::GetClientList;
    let clients_cookie = ewmh_conn.send_request(&clients_req);
    let clients_repl = ewmh_conn
        .wait_for_reply(clients_cookie)
        .expect("Failed to get client list");
    let clients = clients_repl.clients;

    let wins: Rc<Vec<(Window, u32, String, String)>> = Rc::new(
        clients
            .iter()
            .map(|w| {
                let xconn = xcb::Connection::connect(None).unwrap();
                let net_wm_name_atom = xcb::x::InternAtom {
                    only_if_exists: false,
                    name: "_NET_WM_NAME".as_bytes(),
                };
                let dtop_req = ewmh::proto::GetWmDesktop(*w);
                let dtop_cookie = ewmh_conn.send_request(&dtop_req);
                let dtop_repl = ewmh_conn
                    .wait_for_reply(dtop_cookie)
                    .expect("Failed to get window desktop");
                let dtop = dtop_repl.desktop;
                let wmname_req = ewmh::proto::GetWmName(*w);
                let wmname_cookie = ewmh_conn.send_request(&wmname_req);
                let wmname_repl = ewmh_conn
                    .wait_for_reply(wmname_cookie)
                    .expect("Failed to get window name");
                let nm = wmname_repl.name;
                let wmclass_req = icccm::proto::GetWmClass::new(*w);
                let wmclass_cookie = icccm_conn.send_request(&wmclass_req);
                let wmclass_repl = icccm_conn
                    .wait_for_reply(wmclass_cookie)
                    .expect("Failed to get window class");
                let class = wmclass_repl.class;

                (*w, dtop, nm, class)
            })
            .collect(),
    );
    let desktop_req = ewmh::proto::GetCurrentDesktop;
    let desktop_cookie = ewmh_conn.send_request(&desktop_req);
    let desktop_repl = ewmh_conn
        .wait_for_reply(desktop_cookie)
        .expect("Failed to get current desktop");
    let desktop = desktop_repl.desktop;
    return (wins, geom, desktop, active);
}

pub fn abbreviate(x: String, maxlen: usize) -> String {
    let chars = x.chars().collect::<Vec<_>>();
    let len = chars.len();
    if len < maxlen {
        return x;
    } else {
        return format!(
            "{}...{}",
            &chars[..(maxlen / 8) * 4]
                .iter()
                .cloned()
                .collect::<String>(),
            &chars[(len - (maxlen / 8) * 4)..len]
                .iter()
                .cloned()
                .collect::<String>()
        );
    }
}
pub fn make_vbox(
    wins: &Rc<Vec<(Window, u32, String, String)>>,
    desktop: Option<u32>,
    space_between_buttons: i32,
    maxlen: usize,
    blacklist: &Rc<BlacklistedItems>,
    active: &Window,
) -> (gtk::Box, HashMap<u8, Window>) {
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, space_between_buttons);
    vbox.style_context().add_class("main_vbox");
    let mut charhints: HashMap<u8, Window> = HashMap::new();
    let mut j = 0 as u8;
    match desktop {
        Some(d) => println!("only showing windows on desktop {}", d),
        None => println!("showing windows on all desktops"),
    }
    for (num, win_desktop, name, class) in (*wins)
        .iter()
        .filter(|win| match desktop {
            Some(d) => d == win.1,
            None => true,
        })
        .filter(|win| {
            !(*blacklist)
                .item
                .iter()
                .map(|i| &i.class)
                .collect::<Vec<&String>>()
                .contains(&&win.3)
        })
    {
        let class_sanitized = class.replace(".", "_");
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, space_between_buttons);
        let lbtn = gtk::Button::new();
        let llbl = gtk::Label::new(Some(&format!("{}", (j + 97) as char)));
        if num == active {
            lbtn.style_context().add_class("wmjump_lbtn_current");
        } else {
            lbtn.style_context()
                .add_class(&["wbtn_", &class_sanitized].concat()[..]);
            lbtn.style_context().add_class("wmjump_lbtn");
        }
        lbtn.add(&llbl);
        let rbtn = gtk::Button::new();
        let rlbl = gtk::Label::new(Some(&format!("{}", (j + 97) as char)));
        if num == active {
            rbtn.style_context().add_class("wmjump_rbtn_current");
        } else {
            rbtn.style_context()
                .add_class(&["wbtn_", &class_sanitized].concat()[..]);
            rbtn.style_context().add_class("wmjump_rbtn");
        }
        rbtn.add(&rlbl);
        let btn = gtk::Button::new();
        let truncated = name.clone();
        let lbl = gtk::Label::new(Some(&format!(
            "{}: {}",
            win_desktop + 1,
            abbreviate(truncated, maxlen)
        )));
        btn.style_context()
            .add_class(&["wbtn_", &class_sanitized].concat()[..]);
        btn.style_context().add_class("wmjump_button");
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

pub fn get_config_dir() -> PathBuf {
    let p = Path::join(Path::new(&home_dir().unwrap()), ".config/winterreise/");
    if !p.exists() {
        std::fs::create_dir(&p).expect("Could not create config directory");
    }
    p
}
pub fn get_conf() -> Result<Config, WintError> {
    let config_dir = get_config_dir();
    let config_file_path = Path::join(&config_dir, "config.xml");
    if !config_file_path.exists() {
        let init_config = include_str!("config/config.xml");
        std::fs::write(&config_file_path, init_config)
            .expect("Could not write default config file");
    }
    let config_file = File::open(config_file_path).expect("Could not open config.xml");
    let conf = serde_xml_rs::from_reader(config_file)?;
    return Ok(conf);
}
pub fn check_css(p: &Path) -> () {
    if !p.exists() {
        let init_css = include_str!("config/style.css");
        std::fs::write(p, init_css).expect("Could not write default css file");
    }
}

pub fn go_to_window(win: Window, screen_id: u32, mut delay: u64, ewmh_conn: &ewmh::Connection) {
    let dtop_req = ewmh::proto::GetWmDesktop(win);
    let dtop_cookie = ewmh_conn.send_request(&dtop_req);
    let dtop_repl = ewmh_conn
        .wait_for_reply(dtop_cookie)
        .expect("Failed to get window desktop");

    let dtop = dtop_repl.desktop;
    let active_win_req = ewmh::proto::GetActiveWindow;
    let active_win_cookie = ewmh_conn.send_request(&active_win_req);
    let active_win_repl = ewmh_conn
        .wait_for_reply(active_win_cookie)
        .expect("Failed to get active window");
    let curwin = active_win_repl.window;
    let chdtop_req = ewmh::proto::SendCurrentDesktop::new(ewmh_conn, dtop);
    ewmh_conn
        .send_and_check_request(&chdtop_req)
        .expect("Failed to change desktop");

    let chwin_req = ewmh::proto::SendActiveWindow::new(ewmh_conn, win, 2, 0, Some(curwin));
    ewmh_conn
        .send_and_check_request(&chwin_req)
        .expect("Failed to change active window");

    println!("-- going to window {:?}\n   ...", win);
}
