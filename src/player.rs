use rodio::{Decoder, DeviceSinkBuilder, Player as RodioPlayer, MixerDeviceSink, Source};
use anyhow::Result;
use std::fs::File;
use std::io::BufReader;
use std::time::Duration;

pub struct Player {
    rodio_player: RodioPlayer,
    _sink: MixerDeviceSink,
    current_duration: f64,
}

impl Player {
    pub fn new() -> Self {
        crate::log("Initializing Player with Rodio 0.22");
        let sink = DeviceSinkBuilder::open_default_sink()
            .expect("Failed to open default Rodio sink");
        let (rodio_player, output) = RodioPlayer::new();
        sink.mixer().add(output);
        
        Self {
            rodio_player,
            _sink: sink,
            current_duration: 0.0,
        }
    }

    pub fn play(&mut self, path: &str) -> Result<()> {
        crate::log(&format!("Player: Loading file: {}", path));
        
        let file = BufReader::new(File::open(path)?);
        let source = Decoder::new(file)?;
        
        self.current_duration = source.total_duration()
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
            
        self.rodio_player.stop(); // Clear previous
        self.rodio_player.append(source);
        self.rodio_player.play();
        
        crate::log("Player: Successfully started playback");
        Ok(())
    }

    pub fn pause(&self, state: bool) -> Result<()> {
        crate::log(&format!("Player: Setting pause to {}", state));
        if state {
            self.rodio_player.pause();
        } else {
            self.rodio_player.play();
        }
        Ok(())
    }

    pub fn get_paused(&self) -> Result<bool> {
        Ok(self.rodio_player.is_paused())
    }

    pub fn get_position(&self) -> Result<f64> {
        Ok(self.rodio_player.get_pos().as_secs_f64())
    }

    pub fn get_duration(&self) -> Result<f64> {
        Ok(self.current_duration)
    }

    pub fn is_empty(&self) -> bool {
        self.rodio_player.empty()
    }

    pub fn seek(&self, seconds: f64) -> Result<()> {
        let current_pos = self.rodio_player.get_pos();
        let new_pos = if seconds >= 0.0 {
            current_pos + Duration::from_secs_f64(seconds)
        } else {
            current_pos.saturating_sub(Duration::from_secs_f64(-seconds))
        };
        
        if let Err(e) = self.rodio_player.try_seek(new_pos) {
            crate::log(&format!("Player: Seek failed: {:?}", e));
        }
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        self.rodio_player.set_volume(volume as f32 / 100.0);
        Ok(())
    }

    pub fn get_volume(&self) -> Result<f64> {
        Ok(self.rodio_player.volume() as f64 * 100.0)
    }
}
