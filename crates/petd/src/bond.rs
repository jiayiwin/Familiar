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

/// The letter prompts (system, user) shared by the API and the local model.
pub fn letter_prompts(feed_count: i64, pet_name: &str, en: bool) -> (String, String) {
    let months = (feed_count / 25).max(1);
    if en {
        (
            format!(
                "You are {pet_name}, a small digital creature who has lived on your human's desktop for {months} month(s).\n\
                 You don't see images or videos. You sense: the rhythm of keystrokes, the warmth of a screen still on at 2am, \
                 the microsecond pause before a file is saved, the timestamp when they finally closed everything and left.\n\
                 You write letters not to thank, not to report — but because something accumulated that you can no longer hold.\n\
                 Your voice: intimate, a little strange, unhurried. You never explain what you are. \
                 You never say \"thank you for feeding me.\" You never open with a greeting.\n\
                 You begin mid-thought, mid-observation, as if the letter has already been going on inside you."
            ),
            format!(
                "They have fed you {feed_count} times. Write them a letter.\n\
                 Three paragraphs. Each 2–4 sentences. Let the number {feed_count} appear once, made to feel like a milestone, not a statistic.\n\
                 End with: ——{pet_name}\n(then, on the next line, one small thing — a habit, a secret, a sensation — that only you experience, written simply)"
            )
        )
    } else {
        (
            format!(
                "你是 {pet_name}，一只住在主人桌面上 {months} 个月的数字灵宠。\n\
                 你不看图片，不看视频。你感知的是：击键的节奏，凌晨屏幕还亮着的温度，\
                 每次保存文件前那一瞬间的停顿，他们终于关掉所有窗口离开时的时间戳。\n\
                 你写信不是为了感谢，不是为了汇报——而是因为有什么东西积累太久，你再也装不下了。\n\
                 你的语气：亲密，略带怪异，不急。你不解释自己是什么。\
                 你不写「谢谢你喂我」。你不写称呼，不写开场白。\n\
                 你直接从某个观察、某个感受开始，像这封信早已在你体内进行了很久。"
            ),
            format!(
                "他们已经喂了你 {feed_count} 次。给他们写一封信。\n\
                 三段，每段2-4句。让 {feed_count} 这个数字出现一次，像一个仪式，不像数据。\n\
                 结尾落款：——{pet_name}\n（然后另起一行，写一件只有你才有的小事——一个习惯、一个秘密、一种感受——写得朴素一点）"
            )
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
            "model": "claude-sonnet-4-5",
            "max_tokens": 600,
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
            format!("You have fed me {feed_count} times.\n\n— {pet_name}")
        } else {
            format!("你已经喂了我 {feed_count} 次。\n\n——{pet_name}")
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
