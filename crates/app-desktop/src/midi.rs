//! MIDI on the desktop with `midir`: every input is connected, messages arrive on a channel and
//! wake the UI; feedback goes to the output with the same name. Ports that come and go are
//! picked up by a rescan, at most every two seconds.

use daw388_ui::MidiMessage;
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

const CLIENT: &str = "DAW 388";
const RESCAN: Duration = Duration::from_secs(2);

pub struct MidiHub {
    inputs: HashMap<String, MidiInputConnection<()>>,
    outputs: HashMap<String, MidiOutputConnection>,
    tx: Sender<MidiMessage>,
    rx: Receiver<MidiMessage>,
    ctx: eframe::egui::Context,
    last_scan: Option<Instant>,
    /// Ports that failed, so the log says so once.
    failed: HashSet<String>,
}

impl MidiHub {
    pub fn new(ctx: eframe::egui::Context) -> Self {
        let (tx, rx) = channel();
        Self { inputs: HashMap::new(), outputs: HashMap::new(), tx, rx, ctx, last_scan: None, failed: HashSet::new() }
    }

    /// Connect new ports and let go of ports that went away.
    fn rescan(&mut self) {
        if self.last_scan.is_some_and(|t| t.elapsed() < RESCAN) {
            return;
        }
        self.last_scan = Some(Instant::now());
        if let Ok(listing) = MidiInput::new(CLIENT) {
            let names: Vec<String> = listing.ports().iter().filter_map(|p| listing.port_name(p).ok()).collect();
            self.inputs.retain(|name, _| names.contains(name));
            for name in names {
                if !self.inputs.contains_key(&name) {
                    self.connect_input(&name);
                }
            }
        }
        if let Ok(listing) = MidiOutput::new(CLIENT) {
            let names: Vec<String> = listing.ports().iter().filter_map(|p| listing.port_name(p).ok()).collect();
            self.outputs.retain(|name, _| names.contains(name));
            for name in names {
                if !self.outputs.contains_key(&name) {
                    self.connect_output(&name);
                }
            }
        }
    }

    fn connect_input(&mut self, name: &str) {
        let Ok(mut input) = MidiInput::new(CLIENT) else { return };
        // Clock, active sensing and SysEx are of no use here.
        input.ignore(Ignore::All);
        let Some(port) = input.ports().into_iter().find(|p| input.port_name(p).ok().as_deref() == Some(name)) else { return };
        let (tx, ctx, port_name) = (self.tx.clone(), self.ctx.clone(), name.to_string());
        let callback = move |_: u64, bytes: &[u8], _: &mut ()| {
            let _ = tx.send(MidiMessage { port: port_name.clone(), bytes: bytes.to_vec() });
            ctx.request_repaint();
        };
        match input.connect(&port, "DAW 388 in", callback, ()) {
            Ok(connection) => {
                log::info!("MIDI input: {name}");
                self.failed.remove(name);
                self.inputs.insert(name.to_string(), connection);
            }
            Err(e) => {
                if self.failed.insert(name.to_string()) {
                    log::warn!("MIDI input {name}: {e}");
                }
            }
        }
    }

    fn connect_output(&mut self, name: &str) {
        let Ok(output) = MidiOutput::new(CLIENT) else { return };
        let Some(port) = output.ports().into_iter().find(|p| output.port_name(p).ok().as_deref() == Some(name)) else { return };
        match output.connect(&port, "DAW 388 out") {
            Ok(connection) => {
                self.outputs.insert(name.to_string(), connection);
            }
            Err(e) => {
                let key = format!("out:{name}");
                if self.failed.insert(key) {
                    log::warn!("MIDI output {name}: {e}");
                }
            }
        }
    }

    pub fn poll(&mut self) -> Vec<MidiMessage> {
        self.rescan();
        self.rx.try_iter().collect()
    }

    pub fn inputs(&self) -> Vec<String> {
        let mut names: Vec<String> = self.inputs.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn send(&mut self, port: &str, bytes: &[u8]) {
        if let Some(output) = self.outputs.get_mut(port)
            && let Err(e) = output.send(bytes)
        {
            log::warn!("MIDI output {port}: {e}");
        }
    }
}
