use libmpv2::Mpv;
use anyhow::Result;

pub struct Player {
    mpv: Mpv,
}

impl Player {
    pub fn new(options: &[String]) -> Self {
        crate::log!("Initializing Player with libmpv");
        let mpv = Mpv::new().expect("Failed to initialize mpv");

        // Apply user mpv options from the config file (e.g. audio-device, profile)
        for option in options {
            match option.split_once('=') {
                Some((key, value)) => {
                    let key = key.trim().strip_prefix("--").unwrap_or(key.trim());
                    if let Err(e) = mpv.set_property(key, value.trim()) {
                        crate::log!("mpv: failed to set option {}={}: {}", key, value, e);
                    }
                }
                None => {
                    crate::log!("mpv: ignoring malformed option {:?} (expected key=value)", option);
                }
            }
        }

        // Suppress cover art display as requested (applied after user options so it always wins)
        mpv.set_property("vo", "null").expect("Failed to set vo=null");

        Self { mpv }
    }

    pub fn play(&mut self, path: &str) -> Result<()> {
        crate::log!("Player: Loading file: {}", path);
        
        // "replace" will stop current playback and play the new file
        self.mpv.command("loadfile", &[path, "replace"])
            .map_err(|e| anyhow::anyhow!("mpv loadfile error: {}", e))?;
        
        crate::log!("Player: Successfully started playback");
        Ok(())
    }

    pub fn pause(&self, state: bool) -> Result<()> {
        crate::log!("Player: Setting pause to {}", state);
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

    pub fn stop(&self) -> Result<()> {
        self.mpv.command("stop", &[])
            .map_err(|e| anyhow::anyhow!("mpv stop error: {}", e))?;
        Ok(())
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
