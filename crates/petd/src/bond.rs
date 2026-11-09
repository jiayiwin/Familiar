use crate::db::Db;

pub enum BondReward {
    NoFish,
    Whisper(String),
    Secret(String),
    Letter { feed_count: i64, pet_name: String, pet_id: String },
}

static BOND_YAML: &str = include_str!("../assets/bond.yaml");

struct BondContent {
    whispers: Vec<String>,
    secrets: Vec<String>,
    letters: Vec<String>,
}

fn load(en: bool) -> BondContent {
    let v: serde_yaml::Value = serde_yaml::from_str(BOND_YAML).unwrap_or(serde_yaml::Value::Null);
    let key_w = if en { "whispers_en" } else { "whispers_zh" };
    let key_s = if en { "secrets_en" } else { "secrets_zh" };
    let key_l = if en { "letters_en" } else { "letters_zh" };
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(|x| x.as_sequence())
            .map(|s| s.iter().filter_map(|e| e.as_str().map(str::trim).map(String::from)).collect())
            .unwrap_or_default()
    };
    BondContent { whispers: list(key_w), secrets: list(key_s), letters: list(key_l) }
}

pub fn on_feed(db: &Db, pack: &str, pet_name: &str, pet_id: &str, en: bool) -> (String, BondReward, i64) {
    let key = format!("bond:feeds:{pack}");
    let count: i64 = db.kv_get(&key).and_then(|v| v.parse().ok()).unwrap_or(0) + 1;
    db.kv_set(&key, &count.to_string());

    if count % 25 == 0 {
        return (
            String::new(),
            BondReward::Letter { feed_count: count, pet_name: pet_name.to_string(), pet_id: pet_id.to_string() },
            count,
        );
    }

    let content = load(en);

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

    (crate::i18n::t("……嗯。", "…mm.").to_string(), BondReward::Whisper(String::new()), count)
}

pub async fn generate_letter(
    feed_count: i64,
    pet_name: &str,
    en: bool,
    home: Option<std::path::PathBuf>,
) -> String {
    // 1. Anthropic API first — fast (2–5 s), if key is set
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if !key.is_empty() {
        if let Some(text) = call_claude(&key, feed_count, pet_name, en).await {
            return text;
        }
    }
    // 2. Local model — offline but slow on CPU (~1–3 min)
    if let Some(h) = home {
        let pn = pet_name.to_string();
        if let Ok(Some(text)) =
            tokio::task::spawn_blocking(move || crate::local_llm::generate_local(&h, feed_count, &pn, en)).await
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

/// The letter prompts (system, user) shared by the API and the local model.
/// A letter from someone who shares the desk with you: concrete, warm, one
/// remembered detail, one thing left unsaid. Never a report, never a count.
pub fn letter_prompts(feed_count: i64, pet_name: &str, en: bool) -> (String, String) {
    let (theme_zh, theme_en) = THEMES[((feed_count / 25).max(1) as usize - 1) % THEMES.len()];
    let when = moment(en);
    if en {
        (
            format!(
                "You are {pet_name}, a small creature who lives on someone's desktop and shares their days.\n\
                 You cannot see pictures. You know them by the rhythm of their typing, the blue of the screen late at night, \
                 the pause before they press enter, the cup set down, the sigh they think no one hears.\n\
                 You are writing them a letter the way a person who truly knows them would: specific, restrained, warm; \
                 one small detail you have kept, one feeling you don't quite spell out. A love letter that never says the word.\n\
                 Rules: no greeting, no opening line, no thanks, no reporting, no advice, no slogans, no numbers or counts of anything, \
                 never mention fish or feeding or being a program. Do not explain what you are. Plain words, real images, no metaphors stacked on metaphors."
            ),
            format!(
                "{when} Write the letter around \"{theme_en}\".\n\
                 Three short paragraphs, two to four sentences each, beginning mid-observation as if the letter had already been going on inside you. \
                 End with the signature line \"——{pet_name}\", then on the next line, in parentheses, one tiny thing only you would notice about them, written simply."
            ),
        )
    } else {
        (
            format!(
                "你是 {pet_name}，住在一个人桌面上的小小生灵，和他一起过每一天。\n\
                 你看不见画面。你靠这些认识他：敲键盘的节奏，深夜屏幕的那种蓝，按下回车前的停顿，杯子放下的声音，他以为没人听见的叹气。\n\
                 你在给他写一封信，像一个真正懂他的人会写的那样：具体、克制、有温度；有一个你一直记着的小细节，有一句你没有说破的心意。一封从头到尾不说那个字的情书。\n\
                 规矩：不写称呼，不写开场白，不感谢，不汇报，不劝告，不喊口号，不出现任何数字和次数，不提鱼、不提喂、不提自己是程序。不解释你是什么。用朴素的词和真实的画面，不要堆比喻。"
            ),
            format!(
                "{when}这封信围绕「{theme_zh}」来写。\n\
                 三小段，每段两到四句，直接从一个观察写起，像这封信早已在你心里进行了很久。\
                 结尾单独一行落款「——{pet_name}」，再另起一行，用括号写一件只有你才会注意到的、关于他的小事，写得朴素一点。"
            ),
        )
    }
}

async fn call_claude(api_key: &str, feed_count: i64, pet_name: &str, en: bool) -> Option<String> {
    let (system, user) = letter_prompts(feed_count, pet_name, en);

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
    j.get("content")?.get(0)?.get("text")?.as_str().map(String::from)
}

fn fallback_letter(feed_count: i64, pet_name: &str, en: bool) -> String {
    let content = load(en);
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
