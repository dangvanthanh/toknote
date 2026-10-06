use crate::ctx::Ctx;
use crate::model::LiveResult;

pub const LIVE: bool = cfg!(feature = "live");
/// The only version source is `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn fetch(ctx: &Ctx, now: i64) -> LiveResult {
    #[cfg(feature = "live")]
    return crate::live::fetch(ctx, now);
    #[cfg(not(feature = "live"))]
    {
        let _ = (ctx, now);
        LiveResult { errors: vec!["built without live feature".into()], ..Default::default() }
    }
}
