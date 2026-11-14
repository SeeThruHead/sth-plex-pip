use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use crate::plex::client::Library;
use crate::plex::worker::Wake;

const FETCHERS: usize = 4;
const MOST_KEPT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shape {
    Poster,
    Still,
}

impl Shape {
    fn pixels(self) -> (u32, u32) {
        match self {
            Self::Poster => (240, 360),
            Self::Still => (480, 270),
        }
    }

    pub fn aspect(self) -> f32 {
        let (width, height) = self.pixels();
        width as f32 / height as f32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Wanted {
    path: String,
    shape: Shape,
}

#[derive(Default)]
struct Waiting {
    wanted: Vec<Wanted>,
    closed: bool,
}

type Queue = Arc<(Mutex<Waiting>, Condvar)>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    pub id: egui::TextureId,
    pub size: egui::Vec2,
}

pub struct Art {
    queue: Queue,
    fetched: Receiver<(Wanted, egui::ColorImage)>,
    asked: RefCell<HashSet<Wanted>>,
    textures: HashMap<Wanted, egui::TextureHandle>,
}

fn next(queue: &Queue) -> Option<Wanted> {
    let (waiting, bell) = &**queue;
    let mut waiting =
        bell.wait_while(waiting.lock().ok()?, |waiting| waiting.wanted.is_empty() && !waiting.closed).ok()?;
    if waiting.closed {
        return None;
    }
    waiting.wanted.pop()
}

fn decoded(library: &Library, wanted: &Wanted) -> Option<egui::ColorImage> {
    let (width, height) = wanted.shape.pixels();
    let bytes = library.image(&wanted.path, width, height).ok()?;
    let pixels = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [pixels.width() as usize, pixels.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_raw()))
}

fn fetch(library: Library, queue: Queue, fetched: Sender<(Wanted, egui::ColorImage)>, wake: Wake) {
    std::iter::from_fn(|| next(&queue)).for_each(|wanted| {
        if let Some(picture) = decoded(&library, &wanted) {
            let _ = fetched.send((wanted, picture));
            wake();
        }
    });
}

impl Art {
    pub fn start(library: &Library, wake: Wake) -> Self {
        let queue = Queue::default();
        let (sender, fetched) = channel();
        (0..FETCHERS).for_each(|_| {
            let (library, queue, sender, wake) = (library.clone(), queue.clone(), sender.clone(), wake.clone());
            thread::spawn(move || fetch(library, queue, sender, wake));
        });
        Self { queue, fetched, asked: RefCell::default(), textures: HashMap::new() }
    }

    fn ask(&self, wanted: Wanted) {
        if !self.asked.borrow_mut().insert(wanted.clone()) {
            return;
        }
        let (waiting, bell) = &*self.queue;
        if let Ok(mut waiting) = waiting.lock() {
            waiting.wanted.push(wanted);
            bell.notify_one();
        }
    }

    pub fn picture(&self, path: &str, shape: Shape) -> Option<Picture> {
        let wanted = Wanted { path: path.to_string(), shape };
        let found = self.textures.get(&wanted).map(|texture| Picture { id: texture.id(), size: texture.size_vec2() });
        if found.is_none() {
            self.ask(wanted);
        }
        found
    }

    pub fn take_in(&mut self, context: &egui::Context) {
        let arrived: Vec<(Wanted, egui::ColorImage)> = self.fetched.try_iter().collect();
        if self.textures.len() + arrived.len() > MOST_KEPT {
            self.textures.clear();
            self.asked.borrow_mut().clear();
        }
        self.textures.extend(arrived.into_iter().map(|(wanted, picture)| {
            let texture = context.load_texture(wanted.path.clone(), picture, egui::TextureOptions::LINEAR);
            (wanted, texture)
        }));
    }
}

impl Drop for Art {
    fn drop(&mut self) {
        let (waiting, bell) = &*self.queue;
        if let Ok(mut waiting) = waiting.lock() {
            waiting.closed = true;
        }
        bell.notify_all();
    }
}
