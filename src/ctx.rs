use std::env;
use std::path::PathBuf;

/// Environment paths, resolved once. A set but empty variable is used as is.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub home: PathBuf,
    pub openai_home: PathBuf,
    pub data_home: PathBuf,
    #[cfg(feature = "live")]
    pub cache_home: PathBuf,
}

impl Ctx {
    pub fn from_env() -> Ctx {
        let home = dirs::home_dir().unwrap_or_default();
        let or = |var: &str, rel: &str| env::var_os(var).map_or_else(|| home.join(rel), PathBuf::from);
        Ctx {
            openai_home: or("CODEX_HOME", ".codex"),
            data_home: or("XDG_DATA_HOME", ".local/share"),
            #[cfg(feature = "live")]
            cache_home: or("XDG_CACHE_HOME", ".cache"),
            home,
        }
    }
}
