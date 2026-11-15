use anyhow::{Result, bail};
use index_core::*;
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
pub fn random_hex() -> Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("Randomness is unavailable: {e}"))?;
    Ok(hex::encode(bytes))
}
pub fn read_json(path: &Path) -> Result<Value> {
    if path.is_file() {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    let compressed = fs::File::open(path.with_extension("json.gz"))?;
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(compressed).read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
pub fn atomic_write(path: &Path, value: &Value) -> Result<()> {
    // Keep old development snapshots readable, and retain the production
    // compressed representation after refreshes without creating duplicate data.
    let compressed = !path.is_file()
        && (path.with_extension("json.gz").is_file()
            || path
                .file_name()
                .is_some_and(|n| n == "models.json" || n == "public-models.raw.json"));
    let stored = if compressed {
        path.with_extension("json.gz")
    } else {
        path.to_owned()
    };
    let path = stored.as_path();
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", random_hex()?));
    let result = (|| {
        if compressed {
            let file = fs::File::create(&temporary)?;
            let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::best());
            serde_json::to_writer(&mut encoder, value)?;
            encoder.finish()?.flush()?;
        } else {
            fs::write(&temporary, serde_json::to_vec(value)?)?;
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

pub struct Store {
    pub root: PathBuf,
    pub gate: Mutex<()>,
    pub stopping: Arc<AtomicBool>,
}
impl Store {
    pub fn new(root: PathBuf, stopping: Arc<AtomicBool>) -> Self {
        Self {
            root,
            gate: Mutex::new(()),
            stopping,
        }
    }
    pub fn snapshot(&self) -> Result<Value> {
        read_json(&self.root.join("public/models.json"))
    }
    pub fn raw(&self) -> Result<Value> {
        read_json(&self.root.join("data/public-models.raw.json"))
    }
    fn saved(&self) -> Option<Vec<String>> {
        let value = read_json(&self.root.join("data/model-selection.json")).ok()?;
        let array = value["selected"].as_array()?;
        array
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect()
    }
    pub fn selection(&self, incoming: Option<&Value>, initialize: bool) -> Result<Value> {
        let _guard = self.gate.lock().unwrap();
        let saved = self.saved();
        if incoming.is_none() && saved.is_none() {
            return Ok(json!({"selected":null}));
        }
        let candidates: Vec<String> = if incoming.is_none() || initialize && saved.is_some() {
            saved.clone().unwrap_or_default()
        } else {
            array(incoming.unwrap())
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        };
        let snapshot = self.snapshot()?;
        let ids: HashSet<_> = array(&snapshot["models"])
            .iter()
            .filter(|v| eligible(v))
            .map(|v| s(v, "id"))
            .collect();
        let mut seen = HashSet::new();
        let selected: Vec<_> = candidates
            .into_iter()
            .filter(|id| ids.contains(id.as_str()) && seen.insert(id.clone()))
            .collect();
        if incoming.is_some() || saved.as_ref() != Some(&selected) {
            if self.stopping.load(Ordering::Acquire) {
                bail!("The application is closing.");
            }
            atomic_write(
                &self.root.join("data/model-selection.json"),
                &json!({"selected":selected}),
            )?;
        }
        Ok(json!({"selected":selected}))
    }
    pub fn publish(&self, mut snapshot: Value, mut raw: Value) -> Result<()> {
        retain(&mut snapshot, &mut raw);
        let _guard = self.gate.lock().unwrap();
        if self.stopping.load(Ordering::Acquire) {
            bail!("The application is closing. The previous snapshot is preserved.");
        }
        atomic_write(&self.root.join("data/public-models.raw.json"), &raw)?;
        atomic_write(&self.root.join("public/models.json"), &snapshot)?;
        if let Some(saved) = self.saved() {
            let ids: HashSet<_> = array(&snapshot["models"])
                .iter()
                .filter(|m| eligible(m))
                .map(|m| s(m, "id"))
                .collect();
            let mut seen = HashSet::new();
            let selected: Vec<_> = saved
                .into_iter()
                .filter(|id| ids.contains(id.as_str()) && seen.insert(id.clone()))
                .collect();
            atomic_write(
                &self.root.join("data/model-selection.json"),
                &json!({"selected":selected}),
            )?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Remote {
    agent: ureq::Agent,
    pub stopping: Arc<AtomicBool>,
}
impl Remote {
    pub fn new(stopping: Arc<AtomicBool>) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(45)))
            .build();
        Self {
            agent: config.into(),
            stopping,
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.stopping.load(Ordering::Acquire) {
            bail!("The application is closing. The previous snapshot is preserved.");
        }
        Ok(())
    }
    pub fn bytes(&self, url: &str, maximum: usize, timeout: u64, retry: bool) -> Result<Vec<u8>> {
        let attempts = if retry { 3 } else { 1 };
        let mut last = String::new();
        for attempt in 0..attempts {
            self.check()?;
            let mut wait = 500_u64 * (1 << attempt);
            let result = self
                .agent
                .get(url)
                .header("Cache-Control", "no-cache")
                .config()
                .timeout_global(Some(Duration::from_secs(timeout)))
                .build()
                .call();
            match result {
                Ok(mut response) => {
                    let status = response.status().as_u16();
                    if response.status().is_success() {
                        if response
                            .headers()
                            .get("content-length")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<usize>().ok())
                            .is_some_and(|size| size > maximum)
                        {
                            bail!("Publisher metadata exceeds its size limit.");
                        }
                        let mut bytes = Vec::new();
                        response
                            .body_mut()
                            .as_reader()
                            .take(maximum as u64 + 1)
                            .read_to_end(&mut bytes)?;
                        if bytes.len() > maximum {
                            bail!("Publisher metadata exceeds its size limit.");
                        }
                        self.check()?;
                        return Ok(bytes);
                    } else {
                        last = format!("The public data source returned HTTP {status}");
                        if ![408, 425, 429].contains(&status) && status < 500 {
                            bail!("{last}");
                        }
                        if let Some(seconds) = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                        {
                            wait = (seconds * 1000).clamp(500, 10000);
                        }
                    }
                }
                Err(error) => last = error.to_string(),
            }
            if attempt + 1 < attempts {
                for _ in 0..wait.div_ceil(100) {
                    self.check()?;
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        bail!("{last}")
    }
    pub fn text(&self, url: &str) -> Result<String> {
        Ok(String::from_utf8(self.bytes(
            url,
            64 * 1024 * 1024,
            45,
            true,
        )?)?)
    }
    pub fn publisher(&self, url: &str, maximum: usize) -> Result<Vec<u8>> {
        self.bytes(url, maximum, 20, false)
    }
    pub fn object(&self, url: &str) -> Result<Value> {
        Ok(serde_json::from_slice(&self.publisher(url, 2_000_000)?)?)
    }
}

pub fn merge(a: &Value, b: &Value) -> Value {
    let mut out = a.as_object().cloned().unwrap_or_default();
    if let Some(values) = b.as_object() {
        for (k, v) in values {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}
pub fn nullish<'a>(values: &[&'a Value]) -> &'a Value {
    values
        .iter()
        .find(|v| !v.is_null())
        .copied()
        .unwrap_or(&Value::Null)
}
pub fn clean(value: &mut Value) {
    match value {
        Value::String(v) if v == "$undefined" => *value = Value::Null,
        Value::Array(a) => {
            for v in a {
                clean(v)
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                clean(v)
            }
        }
        _ => {}
    }
}
pub fn metadata(catalog: &Value, detail: &Value) -> Value {
    let release = nullish(&[&catalog["release"], &detail["release"]]);
    let effort = nullish(&[&detail["effort"], &catalog["effort"]]);
    json!({
    "release":if release["slug"].is_string()&&release["name"].is_string(){json!({"slug":release["slug"],"name":release["name"]})}else{Value::Null},
    "effort":if effort["slug"].is_string()&&effort["label"].is_string(){json!({"slug":effort["slug"],"label":effort["label"],"level":n(effort,"level")})}else{Value::Null}})
}

pub fn pricing_variants(models: &[Value], raw: &[Value]) -> Vec<Value> {
    let rates = json!({"nonCacheInput":0.1,"cacheRead":0.002,"cacheWrite":0.1,"output":0.2});
    fn close(a: Option<f64>, b: Option<f64>) -> bool {
        matches!((a,b),(Some(a),Some(b))if a>=0.&&b>=0.&&(a-b).abs()<=1e-8_f64.max(b.abs()*1e-9))
    }
    fn reprice(cost: &Value, source: &Value, rates: &Value) -> Value {
        let keys = ["nonCacheInput", "cacheRead", "cacheWrite", "output"];
        if cost.is_null()
            || keys.iter().any(|k| {
                n(cost, k).is_none_or(|v| v < 0.)
                    || n(cost, k) != Some(0.) && n(source, k).is_none_or(|v| v <= 0.)
            })
        {
            return Value::Null;
        }
        if !close(
            n(cost, "nonCacheInput")
                .zip(n(cost, "cacheRead"))
                .zip(n(cost, "cacheWrite"))
                .map(|((a, b), c)| a + b + c),
            n(cost, "input"),
        ) || !close(
            n(cost, "input").zip(n(cost, "output")).map(|(a, b)| a + b),
            n(cost, "total"),
        ) {
            return Value::Null;
        }
        let mut result = json!({});
        for key in keys {
            result[key] = json!(if n(cost, key) == Some(0.) {
                Some(0.)
            } else {
                n(cost, key)
                    .zip(n(source, key))
                    .zip(n(rates, key))
                    .map(|((c, s), r)| c / s * r)
            });
        }
        for key in ["reasoning", "answer"] {
            result[key] = json!(
                n(cost, key)
                    .filter(|v| *v >= 0.)
                    .zip(n(source, "output").filter(|v| *v > 0.))
                    .map(|(c, s)| c / s * n(rates, "output").unwrap())
            );
        }
        result["input"] = json!(
            n(&result, "nonCacheInput").unwrap()
                + n(&result, "cacheRead").unwrap()
                + n(&result, "cacheWrite").unwrap()
        );
        result["total"] = json!(n(&result, "input").unwrap() + n(&result, "output").unwrap());
        result
    }
    let mut output = Vec::new();
    for model in models
        .iter()
        .filter(|m| s(m, "slug") == "muse-spark-1-3-xhigh")
    {
        let Some(raw) = raw.iter().find(|r| s(r, "id") == s(model, "id")) else {
            continue;
        };
        if !eligible(model)
            || !eligible_source(raw)
            || n(model, "intelligence") != n(raw, "intelligenceIndex")
        {
            continue;
        }
        let source = json!({"nonCacheInput":raw["price1mInputTokens"],"cacheRead":raw["cacheHitPrice"],"cacheWrite":nullish(&[&raw["cacheWritePrice"],&raw["price1mInputTokens"]]),"output":raw["price1mOutputTokens"]});
        let cost = &raw["intelligenceIndexCost"];
        let breakdown = if close(n(cost, "total"), n(raw, "intelligenceIndexCostTotal")) {
            reprice(cost, &source, &rates)
        } else {
            Value::Null
        };
        let source_task = &raw["intelligenceIndexCostPerTask"]["cost"];
        let usage = if breakdown.is_null() {
            Value::Null
        } else {
            let mut usage = json!({});
            for key in ["nonCacheInput", "cacheRead", "cacheWrite", "output"] {
                usage[key] = json!(if n(cost, key) == Some(0.) {
                    0.
                } else {
                    n(cost, key).unwrap() / n(&source, key).unwrap() * 1e6
                });
            }
            usage
        };
        let mut variant = model.clone();
        variant["id"] = json!(format!("local:{}:contributor", s(model, "id")));
        variant["slug"] =
            json!(s(model, "slug").replacen("muse-spark-1-3", "muse-spark-1-3-contributor", 1));
        for key in ["name", "fullName"] {
            variant[key] =
                json!(s(model, key).replacen("Muse Spark 1.3", "Muse Spark 1.3 Contributor", 1));
        }
        variant["release"] =
            json!({"slug":"muse-spark-1-3-contributor","name":"Muse Spark 1.3 Contributor"});
        variant["cost"] = breakdown["total"].clone();
        variant["priceInputPerMillion"] = json!(0.1);
        variant["priceOutputPerMillion"] = json!(0.2);
        variant["pricing"] = json!({"modelId":"meta/muse-spark-1.3-contributor","sourceUrl":"https://openrouter.ai/meta/muse-spark-1.3-contributor","sourceApiUrl":"https://openrouter.ai/api/v1/models","verifiedAt":"2026-09-02","ratesPerMillion":rates,
            "sourceModelId":model["id"],"sourceModelName":model["name"],"method":"component-repricing","usageBasis":"reconstructed-from-published-costs","sourceRatesPerMillion":source,"sourceCost":cost,"sourceCostPerTask":source_task,"usage":usage,"breakdown":breakdown,"costPerTask":reprice(source_task,&source,&rates)});
        if eligible(&variant) {
            output.push(variant);
        }
    }
    output
}
