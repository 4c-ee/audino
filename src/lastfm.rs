use crate::config::Config;
use anyhow::Result;
use md5::{Md5, Digest};

const LASTFM_API_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";

pub struct LastFMClient {
    api_key: String,
    api_secret: String,
    session_key: Option<String>,
    http: reqwest::blocking::Client,
}

impl LastFMClient {
    pub fn from_config(config: &Config) -> Option<Self> {
        let api_key = config.get("lastfm", "api_key")?;
        let api_secret = config.get("lastfm", "api_secret")?;
        let session_key = config.get("lastfm", "session_key");

        Some(Self {
            api_key,
            api_secret,
            session_key,
            http: reqwest::blocking::Client::new(),
        })
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty() && !self.api_secret.is_empty()
    }

    pub fn is_authenticated(&self) -> bool {
        self.session_key.is_some()
    }

    pub fn session_key(&self) -> Option<String> {
        self.session_key.clone()
    }

    pub fn get_auth_url(&self) -> String {
        format!(
            "https://www.last.fm/api/auth/?api_key={}",
            urlencoding::encode(&self.api_key)
        )
    }

    pub fn auth_with_token(&mut self, token: &str) -> Result<()> {
        let mut params = std::collections::HashMap::new();
        params.insert("method", "auth.getSession");
        params.insert("api_key", &self.api_key);
        params.insert("token", token);

        let sign = self.sign_params(&params);
        params.insert("api_sig", &sign);
        params.insert("format", "json");

        let response = self.http.post(LASTFM_API_ROOT).form(&params).send()?;
        let json: serde_json::Value = response.json()?;

        if let Some(session) = json.get("session") {
            if let Some(key) = session.get("key").and_then(|k| k.as_str()) {
                self.session_key = Some(key.to_string());
                return Ok(());
            }
        }

        Err(anyhow::anyhow!("Failed to authenticate with Last.FM"))
    }

    pub fn scrobble(
        &self,
        artist: &str,
        track: &str,
        album: Option<&str>,
        timestamp: u64,
    ) -> Result<()> {
        let session_key = self
            .session_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not authenticated with Last.FM"))?;

        let timestamp_str = timestamp.to_string();
        let mut params = std::collections::HashMap::new();
        params.insert("method", "track.scrobble");
        params.insert("api_key", &self.api_key);
        params.insert("sk", session_key);
        params.insert("artist", artist);
        params.insert("track", track);
        params.insert("timestamp", &timestamp_str);

        if let Some(album) = album {
            params.insert("album", album);
        }

        let sign = self.sign_params(&params);
        params.insert("api_sig", &sign);
        params.insert("format", "json");

        let response = self.http.post(LASTFM_API_ROOT).form(&params).send()?;
        let status = response.status();
        let text = response.text()?;

        crate::log(&format!("Last.FM scrobble response: {}", text));

        if status.is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Last.FM scrobble failed"))
        }
    }

    pub fn update_now_playing(&self, artist: &str, track: &str, album: Option<&str>) -> Result<()> {
        let session_key = self
            .session_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Not authenticated with Last.FM"))?;

        let mut params = std::collections::HashMap::new();
        params.insert("method", "track.updateNowPlaying");
        params.insert("api_key", &self.api_key);
        params.insert("sk", session_key);
        params.insert("artist", artist);
        params.insert("track", track);

        if let Some(album) = album {
            params.insert("album", album);
        }

        let sign = self.sign_params(&params);
        params.insert("api_sig", &sign);
        params.insert("format", "json");

        let _response = self.http.post(LASTFM_API_ROOT).form(&params).send()?;
        Ok(())
    }

    fn sign_params(&self, params: &std::collections::HashMap<&str, &str>) -> String {
        let mut keys: Vec<_> = params.iter().collect();
        keys.sort_by(|a, b| a.0.cmp(b.0));

        let mut sign = String::new();
        for (key, value) in keys {
            sign.push_str(key);
            sign.push_str(value);
        }
        sign.push_str(&self.api_secret);

        let hash = Md5::digest(sign.as_bytes());
        format!("{:x}", hash)
    }
}
