use std::{fs, path::PathBuf};
use serde::{Deserialize,Serialize};
use crate::geometry::Placement;

#[derive(Debug,Default,Serialize,Deserialize)]
pub struct Preferences { pub placement:Option<Placement>, pub sprites:Option<PathBuf> }
fn path() -> PathBuf {
    std::env::var_os("OMP_PET_STATE").map(PathBuf::from).unwrap_or_else(|| {
        let home=std::env::var_os("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/Application Support/OMP Pet/preferences.json")
    })
}
pub fn load() -> Preferences {
    fs::read(path()).ok().and_then(|bytes|serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}
pub fn save(prefs:&Preferences) -> Result<(),String> {
    let path=path();
    fs::create_dir_all(path.parent().ok_or("Invalid preferences path")?).map_err(|e|e.to_string())?;
    let temporary=path.with_extension("json.tmp");
    fs::write(&temporary,serde_json::to_vec_pretty(prefs).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    fs::rename(temporary,path).map_err(|e|e.to_string())
}
