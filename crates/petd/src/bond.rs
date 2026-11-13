use crate::db::Db;

pub enum BondReward {
    NoFish,
    Whisper(String),
    Secret(String),
    Letter { feed_count: i64 },
}

/// The feeds so far. Counted per pet pack before there was one pet; the
/// highest of those counts carries over, so nobody starts again from nothing.
fn feeds(db: &Db) -> i64 {
    const KEY: &str = "bond:feeds:zimeng";
    if let Some(n) = db.kv_get(KEY).and_then(|v| v.parse().ok()) {
        return n;
    }
    db.with(|c| {
        c.query_row(
            "SELECT COALESCE(MAX(CAST(value AS INTEGER)), 0) FROM kv WHERE key LIKE 'bond:feeds:%'",
            [],
            |r| r.get(0),
        )
    })
    .unwrap_or(0)
}

static BOND_YAML: &str = include_str!("../assets/bond.yaml");

struct BondContent {
    whispers: Vec<String>,
    secrets: Vec<String>,
    letters: Vec<String>,
}

/// What `who` says when fed. A pet with lines of its own (`<id>_whispers_zh`,
/// …) uses them; the others speak as 紫梦, whose lines carry no prefix.
fn load(en: bool, who: &str) -> BondContent {
    let v: serde_yaml::Value = serde_yaml::from_str(BOND_YAML).unwrap_or(serde_yaml::Value::Null);
    let lang = if en { "en" } else { "zh" };
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|x| x.as_sequence())
            .map(|s| {
                s.iter()
                    .filter_map(|e| e.as_str().map(str::trim).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let own = |kind: &str| {
        let mine = list(&format!("{who}_{kind}_{lang}"));
        if mine.is_empty() {
            list(&format!("{kind}_{lang}"))
        } else {
            mine
        }
    };
    BondContent {
        whispers: own("whispers"),
        secrets: own("secrets"),
        letters: own("letters"),
    }
}

/// One fish fed to `who` (`zimeng`, `mashu`, …): what it says, what it earned
/// you, and the count so far. The count is one, whoever eats.
pub fn on_feed(db: &Db, en: bool, who: &str) -> (String, BondReward, i64) {
    let count = feeds(db) + 1;
    db.kv_set("bond:feeds:zimeng", &count.to_string());

    if count % 25 == 0 {
        return (
            String::new(),
            BondReward::Letter { feed_count: count },
            count,
        );
    }

    let content = load(en, who);

    if count % 5 == 0 && !content.secrets.is_empty() {
        let idx = ((count / 5) as usize - 1) % content.secrets.len();
        let text = content.secrets.get(idx).cloned().unwrap_or_default();
        return (text.clone(), BondReward::Secret(text), count);
    }

    if !content.whispers.is_empty() {
        let idx = (count as usize - 1) % content.whispers.len();
        let text = content.whispers.get(idx).cloned().unwrap_or_default();
        return (text.clone(), BondReward::Whisper(text), count);
    }

    (
        crate::i18n::t("……嗯。", "…mm.").to_string(),
        BondReward::Whisper(String::new()),
        count,
    )
}

pub async fn generate_letter(
    feed_count: i64,
    pet_name: &str,
    en: bool,
    home: Option<std::path::PathBuf>,
    moments: Vec<crate::db::BondMoment>,
) -> String {
    // 1. Anthropic API first — fast (2–5 s), if key is set
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if !key.is_empty() {
        if let Some(text) = call_claude(&key, feed_count, pet_name, en, &moments).await {
            return text;
        }
    }
    // 2. Local model — offline but slow on CPU (~1–3 min)
    if let Some(h) = home {
        let pn = pet_name.to_string();
        let m2 = moments.clone();
        if let Ok(Some(text)) = tokio::task::spawn_blocking(move || {
            crate::local_llm::generate_local(&h, feed_count, &pn, en, &m2)
        })
        .await
        {
            return text;
        }
    }
    // 3. Pre-written YAML fallback
    fallback_letter(feed_count, pet_name, en)
}

/// What a letter circles around. Each milestone takes the next one, so two
/// letters in a row never sound alike.
const THEMES: &[(&str, &str)] = &[
    ("光", "light"),
    ("声音", "sound"),
    ("夜", "night"),
    ("手", "hands"),
    ("窗", "the window"),
    ("等待", "waiting"),
    ("季节", "seasons"),
    ("名字", "a name"),
    ("离开和回来", "leaving and coming back"),
    ("休息", "rest"),
];

/// Time of day and season in words, so the letter can be about today.
fn moment(en: bool) -> String {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let h = now.hour();
    let (tod_zh, tod_en) = match h {
        5..=8 => ("清晨", "early morning"),
        9..=11 => ("上午", "morning"),
        12..=13 => ("中午", "midday"),
        14..=17 => ("下午", "afternoon"),
        18..=20 => ("傍晚", "evening"),
        21..=23 => ("深夜", "late at night"),
        _ => ("凌晨", "the small hours"),
    };
    let (sea_zh, sea_en) = match now.month() {
        3..=5 => ("春天", "spring"),
        6..=8 => ("夏天", "summer"),
        9..=11 => ("秋天", "autumn"),
        _ => ("冬天", "winter"),
    };
    let (wd_zh, wd_en) = match now.weekday().num_days_from_monday() {
        5 | 6 => ("周末", "a weekend"),
        4 => ("周五", "a Friday"),
        0 => ("周一", "a Monday"),
        _ => ("工作日", "a weekday"),
    };
    if en {
        format!("It is {tod_en}, {wd_en}, {sea_en}.")
    } else {
        format!("现在是{sea_zh}的一个{wd_zh}，{tod_zh}。")
    }
}

fn moments_context(moments: &[crate::db::BondMoment], en: bool) -> String {
    if moments.is_empty() {
        return String::new();
    }
    let now_ms = crate::now_ms();
    let lines: Vec<String> = moments
        .iter()
        .take(3)
        .filter_map(|m| {
            let age_h = now_ms.saturating_sub(m.ts) / 3_600_000;
            let age = if age_h < 24 {
                if en {
                    format!("{age_h}h ago")
                } else {
                    format!("{age_h}小时前")
                }
            } else {
                let d = age_h / 24;
                if en {
                    format!("{d}d ago")
                } else {
                    format!("{d}天前")
                }
            };
            Some(match m.kind.as_str() {
                "late_night" => {
                    if en {
                        format!("({age}: still at the desk past midnight)")
                    } else {
                        format!("（{age}：深夜还在工作）")
                    }
                }
                "error_burst" => {
                    if en {
                        format!("({age}: several errors in a row)")
                    } else {
                        format!("（{age}：连续遇到了问题）")
                    }
                }
                "stone" => {
                    if en {
                        format!("({age}: got completely stuck for a while)")
                    } else {
                        format!("（{age}：卡住了很久）")
                    }
                }
                "radio_on" => {
                    let station = serde_json::from_str::<serde_json::Value>(&m.detail)
                        .ok()
                        .and_then(|v| {
                            v.get("station")
                                .and_then(|s| s.as_str())
                                .map(|s| s.to_owned())
                        })
                        .unwrap_or_default();
                    if station.is_empty() {
                        return None;
                    }
                    if en {
                        format!("({age}: the radio was on — {station})")
                    } else {
                        format!("（{age}：在放{station}）")
                    }
                }
                _ => return None,
            })
        })
        .collect();
    if lines.is_empty() {
        return String::new();
    }
    if en {
        format!("\nSome things I remember: {}", lines.join("; "))
    } else {
        format!("\n我记得的一些事：{}", lines.join("；"))
    }
}

/// The letter prompts (system, user) shared by the API and the local model.
/// A letter from someone who shares the desk with you: concrete, warm, one
/// remembered detail, one thing left unsaid. Never a report, never a count.
pub fn letter_prompts(
    feed_count: i64,
    pet_name: &str,
    en: bool,
    moments: &[crate::db::BondMoment],
) -> (String, String) {
    let (theme_zh, theme_en) = THEMES[((feed_count / 25).max(1) as usize - 1) % THEMES.len()];
    // Whose letter it is: its temper and its instrument shape how it writes.
    let nature = crate::voice::nature(
        crate::cast::by_name(pet_name)
            .map(|f| f.id)
            .unwrap_or("zimeng"),
        en,
    );
    let when = moment(en);
    let ctx = moments_context(moments, en);
    if en {
        (
            format!(
                "You are {pet_name}, a small creature who lives on someone's desktop and shares their days.\n\
                 You cannot see pictures. You know them by the rhythm of their typing, the blue of the screen late at night, \
                 the pause before they press enter, the cup set down, the sigh they think no one hears.\n\
                 {nature}\n\
                 You are writing them a letter the way a person who truly knows them would: specific, restrained, warm; \
                 one small detail you have kept, one feeling you don't quite spell out. A love letter that never says the word.\n\
                 Rules: no greeting, no opening line, no thanks, no reporting, no advice, no slogans, no numbers or counts of anything, \
                 never mention sweets or feeding or being a program. Do not explain what you are. Plain words, real images, no metaphors stacked on metaphors."
            ),
            format!(
                "{when}{ctx}\nWrite the letter around \"{theme_en}\".\n\
                 Three short paragraphs, two to four sentences each, beginning mid-observation as if the letter had already been going on inside you. \
                 If the \"Some things I remember\" list above has anything in it, let one of those moments surface in the letter — not explained, just present. \
                 End with the signature line \"——{pet_name}\", then on the next line, in parentheses, one tiny thing only you would notice about them, written simply."
            ),
        )
    } else {
        (
            format!(
                "你是 {pet_name}，住在一个人桌面上的小小生灵，和他一起过每一天。\n\
                 你看不见画面。你靠这些认识他：敲键盘的节奏，深夜屏幕的那种蓝，按下回车前的停顿，杯子放下的声音，他以为没人听见的叹气。\n\
                 {nature}\n\
                 你在给他写一封信，像一个真正懂他的人会写的那样：具体、克制、有温度；有一个你一直记着的小细节，有一句你没有说破的心意。一封从头到尾不说那个字的情书。\n\
                 规矩：不写称呼，不写开场白，不感谢，不汇报，不劝告，不喊口号，不出现任何数字和次数，不提糖果、不提喂、不提自己是程序。不解释你是什么。用朴素的词和真实的画面，不要堆比喻。"
            ),
            format!(
                "{when}{ctx}\n这封信围绕「{theme_zh}」来写。\n\
                 三小段，每段两到四句，直接从一个观察写起，像这封信早已在你心里进行了很久。\
                 如果上面「我记得的一些事」里有内容，让其中一件事出现在信里——不用解释，只是在那里。\
                 结尾单独一行落款「——{pet_name}」，再另起一行，用括号写一件只有你才会注意到的、关于他的小事，写得朴素一点。"
            ),
        )
    }
}

async fn call_claude(
    api_key: &str,
    feed_count: i64,
    pet_name: &str,
    en: bool,
    moments: &[crate::db::BondMoment],
) -> Option<String> {
    let (system, user) = letter_prompts(feed_count, pet_name, en, moments);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .ok()?;

    let res = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&serde_json::json!({
            "model": "claude-sonnet-5-5",
            "max_tokens": 700,
            "system": system,
            "messages": [{ "role": "user", "content": user }]
        }))
        .send()
        .await
        .ok()?;

    if !res.status().is_success() {
        return None;
    }
    let j: serde_json::Value = res.json().await.ok()?;
    j.get("content")?
        .get(0)?
        .get("text")?
        .as_str()
        .map(String::from)
}

fn fallback_letter(feed_count: i64, pet_name: &str, en: bool) -> String {
    let content = load(
        en,
        crate::cast::by_name(pet_name)
            .map(|f| f.id)
            .unwrap_or("zimeng"),
    );
    if content.letters.is_empty() {
        return if en {
            format!("You're still here. So am I.\n\n——{pet_name}")
        } else {
            format!("你还在。我也在。\n\n——{pet_name}")
        };
    }
    let idx = ((feed_count / 25) as usize).saturating_sub(1) % content.letters.len();
    content
        .letters
        .get(idx)
        .cloned()
        .unwrap_or_default()
        .replace("{count}", &feed_count.to_string())
        .replace("{pet_name}", pet_name)
}
