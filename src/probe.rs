use std::{
    ffi::{CStr, c_int, c_uint},
    mem, ptr, slice,
};

use anyhow::{Error, anyhow};
use nanoserde::{DeJson, SerJson};
use x11::{
    xinerama,
    xlib::{self},
};

#[derive(Debug, Clone, DeJson, SerJson)]
pub struct Probe {
    pub windows: Vec<Window>,
    pub monitors: Vec<Monitor>,
}

#[derive(Debug, Clone, DeJson, SerJson)]
pub struct Window {
    pub id: u64,
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub name: Option<String>,
    pub wm_name: Option<String>,
}

impl ToString for Window {
    fn to_string(&self) -> String {
        match (&self.name, &self.wm_name) {
            (None, None) => format!(
                "{}: Unknown window {}x{} {:+}{:+}",
                self.id, self.width, self.height, self.x, self.y,
            ),
            (None, Some(wm_name)) => format!("{}: {}", self.id, wm_name),
            (Some(name), None) => format!("{}: {}", self.id, name),
            (Some(name), Some(wm_name)) => format!("{}: {} ({})", self.id, name, wm_name),
        }
    }
}

#[derive(Debug, Clone, DeJson, SerJson)]
pub struct Monitor {
    pub id: i32,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
}

impl ToString for Monitor {
    fn to_string(&self) -> String {
        format!(
            "{}: {}x{} {:+}{:+}",
            self.id, self.width, self.height, self.x, self.y
        )
    }
}

/// Print screen and window data to stdout.
///
/// This function leaks memory and resources. It should only be run as part of a oneshot process.
pub fn probe() -> Result<(), Error> {
    let display = unsafe { xlib::XOpenDisplay(ptr::null()) };
    if display.is_null() {
        return Err(anyhow!("XOpenDisplay() returned NULL"));
    }

    let screen = unsafe { xlib::XDefaultScreen(display) };
    let root_window = unsafe { xlib::XRootWindow(display, screen) };

    let windows = get_windows(display, root_window)?;
    let monitors = get_monitors(display)?;

    println!("{}", Probe { windows, monitors }.serialize_json());
    Ok(())
}

fn get_monitors(display: *mut xlib::Display) -> Result<Vec<Monitor>, Error> {
    let mut nscreens: c_int = 0;
    let screens = unsafe { xinerama::XineramaQueryScreens(display, &mut nscreens) };
    if screens.is_null() {
        return Err(anyhow!("XineramaQueryScreens() returned NULL"));
    }
    let screens = unsafe { slice::from_raw_parts(screens, nscreens.try_into().unwrap()) };

    Ok(screens
        .into_iter()
        .map(|screen| Monitor {
            id: screen.screen_number,
            x: screen.x_org,
            y: screen.y_org,
            width: screen.width,
            height: screen.height,
        })
        .collect())
}

fn get_windows(display: *mut xlib::Display, window: u64) -> Result<Vec<Window>, Error> {
    get_window_children(display, window)?
        .into_iter()
        .filter_map(|&child| get_window(display, child).transpose())
        .collect()
}

fn get_window(display: *mut xlib::Display, window: u64) -> Result<Option<Window>, Error> {
    // SAFETY: zero is a valid value for each T in XWindowAttributes.
    let mut attributes: xlib::XWindowAttributes = unsafe { mem::zeroed() };
    let status = unsafe { xlib::XGetWindowAttributes(display, window, &mut attributes) };
    if status == 0 {
        return Err(anyhow!("{window}: XGetWindowAttributes returned {status}"));
    }

    if attributes.class == xlib::InputOnly || attributes.map_state != xlib::IsViewable {
        return Ok(None);
    }
    // HACK: Firefox has this weird 1px window and this is the only way I can figure out how to
    // filter it out.
    if attributes.width <= 1 || attributes.height <= 1 {
        return Ok(None);
    }

    let name = {
        let mut name: *mut i8 = ptr::null_mut();
        let status = unsafe { xlib::XFetchName(display, window, &mut name) };
        if status == 0 || name.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(name) }
                    .to_string_lossy()
                    .to_string(),
            )
        }
    };

    let wm_name = get_wm_name(display, window)?;

    Ok(Some(Window {
        id: window,
        x: attributes.x,
        y: attributes.y,
        width: attributes.width,
        height: attributes.height,
        name,
        wm_name,
    }))
}

fn get_wm_name(display: *mut xlib::Display, window: u64) -> Result<Option<String>, Error> {
    let mut wm_name = xlib::XTextProperty {
        value: ptr::null_mut(),
        encoding: 0,
        format: 0,
        nitems: 0,
    };
    let status = unsafe { xlib::XGetWMName(display, window, &mut wm_name) };
    if status == 0 || wm_name.value.is_null() {
        return Ok(None);
    }

    let mut list: *mut *mut i8 = ptr::null_mut();
    let mut len: c_int = 0;
    let status =
        unsafe { xlib::Xutf8TextPropertyToTextList(display, &wm_name, &mut list, &mut len) };
    if status < 0 {
        return Err(anyhow!(
            "{window}: Xutf8TextPropertyToTextList() returned {status}"
        ));
    }
    Ok(if len > 0 {
        Some(
            unsafe { CStr::from_ptr(wm_name.value.cast()) }
                .to_string_lossy()
                .to_string(),
        )
    } else {
        None
    })
}

fn get_window_children<'a>(display: *mut xlib::Display, window: u64) -> Result<&'a [u64], Error> {
    // Ideally we would use the property _NET_CLIENT_LIST, but for some reason that's only set in
    // dom0.
    let (mut root, mut parent) = (0u64, 0u64);
    let mut children: *mut u64 = ptr::null_mut();
    let mut num_children: c_uint = 0;

    let status = unsafe {
        xlib::XQueryTree(
            display,
            window,
            &mut root,
            &mut parent,
            &mut children,
            &mut num_children,
        )
    };
    if status == 0 {
        return Err(anyhow!("{window}: XQueryTree returned {status}"));
    }

    if children.is_null() {
        Ok(&[])
    } else if !children.is_aligned() {
        // Is this even possible?
        Err(anyhow!("{window}: XQueryTree returned unaligned pointer"))
    } else {
        Ok(unsafe { slice::from_raw_parts(children, num_children.try_into().unwrap()) })
    }
}
