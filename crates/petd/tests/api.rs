//! The daemon end to end, on a home of its own: start it, send an agent's
//! turn the way an adapter would, and read back what the windows read.

use serde_json::{json, Value};
use std::time::Duration;

struct Api {
    base: String,
    token: String,
    http: reqwest::Client,
}

impl Api {
    async fn get(&self, path: &str) -> (u16, Value) {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .header("x-rillbuddy-token", &self.token)
            .send()
            .await
            .unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }

    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .header("x-rillbuddy-token", &self.token)
            .json(&body)
            .send()
            .await
            .unwrap();
        let code = r.status().as_u16();
        (code, r.json().await.unwrap_or(Value::Null))
    }
}

fn event(kind: &str, ts: u64, detail: Value) -> Value {
    json!({ "v": 1, "source": "claude-code", "session": "s1", "project": "demo", "event": kind, "detail": detail, "pid": 0, "ts": ts })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_goes_in_and_the_windows_can_read_the_day() {
    let home = tempfile::tempdir().unwrap();
    let petd = petd::start(petd::Options {
        home: Some(home.path().to_path_buf()),
        port: Some(0),
    })
    .await
    .expect("the daemon starts");
    let api = Api {
        base: format!("http://127.0.0.1:{}", petd.port),
        token: petd.token.clone(),
        http: reqwest::Client::new(),
    };

    // Without the token nothing answers.
    let bare = api
        .http
        .get(format!("{}/v1/pets", api.base))
        .send()
        .await
        .unwrap();
    assert_eq!(bare.status().as_u16(), 401);

    // One pet per agent, and nothing of the packs it used to wear.
    let (code, pets) = api.get("/v1/pets").await;
    assert_eq!(code, 200);
    let pet = pets["pets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "claude-code")
        .expect("the Claude Code pet always exists")
        .clone();
    assert_eq!(pet["marker"], "Claude");
    assert!(pet["size"].is_number(), "{pet}");
    for gone in [
        "pack", "pack_rev", "hue", "target", "minis", "hat", "wear", "stage",
    ] {
        assert!(pet.get(gone).is_none(), "{gone} should be gone: {pet}");
    }

    // A turn of three minutes that ends well.
    let t0 = petd::now_ms() - 200_000;
    for (kind, at, detail) in [
        ("session.start", 0, json!({})),
        ("turn.start", 1_000, json!({})),
        (
            "tool.start",
            2_000,
            json!({ "tool": "Edit", "file": "a.rs" }),
        ),
        ("tool.end", 3_000, json!({ "tool": "Edit", "ok": true })),
        ("turn.done", 181_000, json!({ "result": "ok" })),
    ] {
        let (code, _) = api.post("/v1/event", event(kind, t0 + at, detail)).await;
        assert!(code == 200 || code == 204, "{kind}: {code}");
    }

    // The background loop records it; the report is what the card draws.
    let mut report = Value::Null;
    for _ in 0..50 {
        report = api.get("/v1/report").await.1;
        if report["turns"].as_u64() == Some(1) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert_eq!(report["turns"], 1, "{report}");
    assert_eq!(report["clean"], 1, "{report}");
    assert!(report["work_ms"].as_u64().unwrap() >= 170_000, "{report}");
    assert_eq!(report["by_camp"]["claude-code"]["turns"], 1, "{report}");
    for gone in [
        "coins_earned",
        "balance",
        "whips",
        "whipped",
        "games",
        "games_by",
        "duty",
    ] {
        assert!(
            report.get(gone).is_none(),
            "{gone} should be gone: {report}"
        );
    }
    for kept in [
        "catches",
        "bugs",
        "bosses",
        "radio_s",
        "kegel_sets",
        "letters",
        "peak_agents",
    ] {
        assert!(
            report.get(kept).is_some(),
            "{kept} should be there: {report}"
        );
    }

    // The pet knows the session and its project.
    let (_, pets) = api.get("/v1/pets").await;
    let pet = pets["pets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "claude-code")
        .unwrap()
        .clone();
    assert_eq!(pet["sessions"][0]["project"], "demo", "{pet}");

    // The room and the gallery.
    let (code, today) = api.get("/v1/today").await;
    assert_eq!(code, 200);
    assert!(
        today["desk_ms"].is_u64()
            && today["breaks"].is_u64()
            && today["fish_left"].is_i64()
            && today["noticed"].is_array(),
        "{today}"
    );
    let (code, mem) = api.get("/v1/memories").await;
    assert_eq!(code, 200);
    assert!(
        mem["arrived"].as_u64().unwrap() > 0 && mem["items"].is_array(),
        "{mem}"
    );
    let (_, again) = api.get("/v1/memories").await;
    assert_eq!(
        again["arrived"], mem["arrived"],
        "the day it arrived does not move"
    );
    let (_, stats) = api.get("/v1/stats").await;
    assert!(!stats["fish"].as_array().unwrap().is_empty(), "{stats}");
    assert!(
        stats.get("wallet").is_none() && stats.get("daily_cap").is_none(),
        "{stats}"
    );
    let (_, world) = api.get("/v1/world").await;
    assert!(
        world["bugs"].is_array() && world["focus"].is_boolean() && world["sounds"].is_boolean(),
        "{world}"
    );
    assert!(
        world.get("wallet").is_none() && world.get("tug").is_none(),
        "{world}"
    );
    let (code, bond) = api.get("/v1/bond").await;
    assert_eq!(code, 200);
    assert!(bond["familiarity"].is_number(), "{bond}");

    // Settings: the kegel reps live here now.
    let (_, settings) = api.get("/v1/settings").await;
    assert_eq!(settings["kegel_reps"], 10, "{settings}");
    for gone in ["mode", "unlock", "stage", "arena"] {
        assert!(
            settings.get(gone).is_none(),
            "{gone} should be gone: {settings}"
        );
    }
    let (_, set) = api
        .post("/v1/settings", json!({ "key": "kegel_reps", "value": 15 }))
        .await;
    assert_eq!(set["ok"], true, "{set}");
    assert_eq!(api.get("/v1/settings").await.1["kegel_reps"], 15);
    let (_, bad) = api
        .post("/v1/settings", json!({ "key": "kegel_reps", "value": 99 }))
        .await;
    assert_eq!(bad["ok"], false, "{bad}");
    let (_, off) = api
        .post("/v1/settings", json!({ "key": "sounds", "value": false }))
        .await;
    assert_eq!(off["ok"], true, "{off}");
    assert_eq!(api.get("/v1/settings").await.1["sounds"], false);

    // What was deleted answers nothing.
    for path in [
        "/v1/shop",
        "/v1/packs",
        "/v1/progress",
        "/v1/games",
        "/v1/spring",
        "/v1/config",
        "/v1/reminders",
    ] {
        assert_eq!(api.get(path).await.0, 404, "{path}");
    }
}
