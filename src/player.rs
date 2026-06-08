use libmpv2::Mpv;
use anyhow::Result;

pub struct Player {
    mpv: Mpv,
}

impl Player {
    pub fn new() -> Self {
        crate::log("Initializing Player with libmpv");
        let mpv = Mpv::new().expect("Failed to initialize mpv");
        
        // Suppress cover art display as requested
        mpv.set_property("vo", "null").expect("Failed to set vo=null");
        
        Self { mpv }
    }

    pub fn play(&mut self, path: &str) -> Result<()> {
        crate::log(&format!("Player: Loading file: {}", path));
        
        // "replace" will stop current playback and play the new file
        self.mpv.command("loadfile", &[path, "replace"])
            .map_err(|e| anyhow::anyhow!("mpv loadfile error: {}", e))?;
        
        crate::log("Player: Successfully started playback");
        Ok(())
    }

    pub fn pause(&self, state: bool) -> Result<()> {
        crate::log(&format!("Player: Setting pause to {}", state));
        self.mpv.set_property("pause", state)
            .map_err(|e| anyhow::anyhow!("mpv set_property pause error: {}", e))?;
        Ok(())
    }

    pub fn get_paused(&self) -> Result<bool> {
        self.mpv.get_property::<bool>("pause")
            .map_err(|e| anyhow::anyhow!("mpv get_property pause error: {}", e))
    }

    pub fn get_position(&self) -> Result<f64> {
        // time-pos might be unavailable if nothing is playing
        Ok(self.mpv.get_property::<f64>("time-pos").unwrap_or(0.0))
    }

    pub fn get_duration(&self) -> Result<f64> {
        // duration might be unavailable if nothing is playing
        Ok(self.mpv.get_property::<f64>("duration").unwrap_or(0.0))
    }

    pub fn is_empty(&self) -> bool {
        // idle-active is true when mpv has nothing to play and is idling
        self.mpv.get_property::<bool>("idle-active").unwrap_or(true)
    }

    pub fn seek(&self, seconds: f64) -> Result<()> {
        let seek_val = format!("{}", seconds);
        self.mpv.command("seek", &[&seek_val, "relative"])
            .map_err(|e| anyhow::anyhow!("mpv seek error: {}", e))?;
        Ok(())
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        self.mpv.set_property("volume", volume)
            .map_err(|e| anyhow::anyhow!("mpv set_property volume error: {}", e))?;
        Ok(())
    }

    pub fn get_volume(&self) -> Result<f64> {
        self.mpv.get_property::<f64>("volume")
            .map_err(|e| anyhow::anyhow!("mpv get_property volume error: {}", e))
    }
}
