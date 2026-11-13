//! The five pets. Each is a whole skin of the product (its colours, its room,
//! its letter paper, its lines) and plays one instrument with its own body.
//! Which pet stands for which agent is the user's choice (`companions:` in
//! config.yaml); a pet that has not arrived yet cannot be chosen.

use crate::config::Config;

pub struct Fellow {
    pub id: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
    /// What it plays: the windows pick the samples and the playing pose by this.
    pub instrument: &'static str,
    /// Cut, rigged and able to move. The others are drawn but still on their way.
    pub here: bool,
}

impl Fellow {
    pub fn name(&self) -> &'static str {
        crate::i18n::t(self.zh, self.en)
    }
}

pub const CAST: [Fellow; 5] = [
    Fellow {
        id: "zimeng",
        zh: "紫梦",
        en: "Zimeng",
        instrument: "piano",
        here: true,
    },
    Fellow {
        id: "mashu",
        zh: "麻薯",
        en: "Mashu",
        instrument: "cello",
        here: true,
    },
    Fellow {
        id: "paopao",
        zh: "泡泡",
        en: "Paopao",
        instrument: "flute",
        here: true,
    },
    Fellow {
        id: "gugu",
        zh: "菇菇",
        en: "Gugu",
        instrument: "xylophone",
        here: true,
    },
    Fellow {
        id: "rongrong",
        zh: "茸茸",
        en: "Rongrong",
        instrument: "harp",
        here: true,
    },
];

pub fn by_id(id: &str) -> Option<&'static Fellow> {
    CAST.iter().find(|f| f.id == id)
}

/// The pet called `name`, in either language (a letter is signed with it).
pub fn by_name(name: &str) -> Option<&'static Fellow> {
    CAST.iter().find(|f| f.zh == name || f.en == name)
}

/// The agents that can have a pet of their own, in the order of the cast.
pub const AGENTS: [&str; 5] = ["claude-code", "codex", "gemini", "cursor", "opencode"];

/// The pet an agent gets when the user has not chosen: 紫梦 with Claude Code,
/// 麻薯 with Codex, 泡泡 with Gemini CLI, 菇菇 with Cursor, 茸茸 with OpenCode,
/// and 紫梦 with anything else.
fn default_for(source: &str) -> &'static Fellow {
    &CAST[AGENTS.iter().position(|a| *a == source).unwrap_or(0)]
}

/// The pet that stands for `source`.
pub fn of(cfg: &Config, source: &str) -> &'static Fellow {
    cfg.companions
        .get(source)
        .and_then(|id| by_id(id))
        .filter(|f| f.here)
        .unwrap_or_else(|| default_for(source))
}

/// The pet whose skin the windows that belong to no pet wear (tray, gallery,
/// settings, report): the one with Claude Code.
pub fn main(cfg: &Config) -> &'static Fellow {
    of(cfg, "claude-code")
}

/// Everything a window needs to draw the cast and the pairing.
pub fn json(cfg: &Config) -> serde_json::Value {
    serde_json::json!({
        "main": main(cfg).id,
        "companions": AGENTS.iter().map(|a| (a.to_string(), serde_json::json!(of(cfg, a).id))).collect::<serde_json::Map<_, _>>(),
        "cast": CAST.iter().map(|f| serde_json::json!({ "id": f.id, "name": f.name(), "instrument": f.instrument, "here": f.here })).collect::<Vec<_>>(),
    })
}

/// Give `pet` to `source`. No two agents share a pet: whoever had it takes the
/// one `source` gives up, so the pets on the desktop are always different ones
/// to play off each other.
pub fn pair(cfg: &Config, source: &str, pet: &str) -> Result<Vec<(String, String)>, String> {
    let f = by_id(pet).ok_or_else(|| crate::tr!("没有这只宠物：{pet}", "No such pet: {pet}"))?;
    if !f.here {
        return Err(crate::tr!(
            "{} 还没来。",
            "{} has not arrived yet.",
            f.name()
        ));
    }
    let mut out = vec![(source.to_string(), f.id.to_string())];
    if let Some(o) = AGENTS
        .iter()
        .find(|a| **a != source && of(cfg, a).id == f.id)
    {
        out.push((o.to_string(), of(cfg, source).id.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_pair_and_swap() {
        let mut cfg = Config::default();
        assert_eq!(
            (
                of(&cfg, "claude-code").id,
                of(&cfg, "codex").id,
                of(&cfg, "gemini").id
            ),
            ("zimeng", "mashu", "paopao")
        );
        assert_eq!(
            (
                of(&cfg, "cursor").id,
                of(&cfg, "opencode").id,
                of(&cfg, "something-else").id
            ),
            ("gugu", "rongrong", "zimeng")
        );
        // Taking the other agent's pet swaps the two.
        let moves = pair(&cfg, "claude-code", "mashu").unwrap();
        assert_eq!(
            moves,
            vec![
                ("claude-code".to_string(), "mashu".to_string()),
                ("codex".to_string(), "zimeng".to_string())
            ]
        );
        for (s, p) in moves {
            cfg.companions.insert(s, p);
        }
        assert_eq!(
            (
                of(&cfg, "claude-code").id,
                of(&cfg, "codex").id,
                main(&cfg).id
            ),
            ("mashu", "zimeng", "mashu")
        );
        // Taking a third agent's pet swaps with that one, and leaves the others alone.
        let moves = pair(&cfg, "codex", "paopao").unwrap();
        assert_eq!(
            moves,
            vec![
                ("codex".to_string(), "paopao".to_string()),
                ("gemini".to_string(), "zimeng".to_string())
            ]
        );
        // An unknown pet cannot be chosen, and one in the file falls back.
        assert!(pair(&cfg, "codex", "nobody").is_err());
        cfg.companions.insert("codex".into(), "nobody".into());
        assert_eq!(of(&cfg, "codex").id, "mashu");
        // Every agent has a different pet by default.
        let mut ids: Vec<_> = AGENTS
            .iter()
            .map(|a| of(&Config::default(), a).id)
            .collect();
        ids.dedup();
        assert_eq!(ids.len(), 5);
    }
}
