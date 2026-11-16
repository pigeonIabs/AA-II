use crate::{data::*, enrichment};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Result, anyhow, bail};
use flate2::read::GzDecoder;
use index_core::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::Path,
    thread,
    time::Duration,
};

const ORIGIN: &str = "https://artificialanalysis.ai";
fn page(html: &str) -> Vec<Value> {
    let pattern = Regex::new(r"self\s*\.\s*__next_f\s*\.\s*push\s*\(").unwrap();
    let mut stream = String::new();
    for capture in pattern.find_iter(html) {
        let mut decoder =
            serde_json::Deserializer::from_str(&html[capture.end()..]).into_iter::<Value>();
        if let Some(Ok(chunk)) = decoder.next() {
            if let Some(text) = chunk[1].as_str() {
                stream.push_str(text);
            }
        }
    }
    stream
        .lines()
        .filter_map(|line| serde_json::from_str(&line[line.find(':').map_or(0, |i| i + 1)..]).ok())
        .collect()
}
fn find_arrays<'a>(node: &'a Value, key: &str, out: &mut Vec<&'a Value>) {
    match node {
        Value::Object(map) => {
            if node[key].is_array() && !array(&node[key]).is_empty() {
                out.push(&node[key]);
            }
            for value in map.values() {
                find_arrays(value, key, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                find_arrays(value, key, out);
            }
        }
        _ => {}
    }
}
fn page_models(remote: &Remote, records: &[Value]) -> Result<Vec<Value>> {
    // Discover by schema, not component names, CSS, nesting, or a manifest's position.
    fn walk(node: &Value, manifests: &mut Vec<Value>, collections: &mut Vec<Vec<Value>>) {
        match node {
            Value::Object(map) => {
                if s(node, "path").starts_with("/data/") && node.get("key").is_some() {
                    if !manifests.contains(node) {
                        manifests.push(node.clone());
                    }
                }
                for child in map.values() {
                    walk(child, manifests, collections);
                }
            }
            Value::Array(values) => {
                if !values.is_empty()
                    && values
                        .iter()
                        .all(|m| m["id"].is_string() && m.get("intelligenceIndex").is_some())
                {
                    collections.push(values.clone());
                }
                for child in values {
                    walk(child, manifests, collections);
                }
            }
            _ => {}
        }
    }
    let mut manifests = Vec::new();
    let mut collections = Vec::new();
    for record in records {
        walk(record, &mut manifests, &mut collections);
    }
    let mut last_error = None;
    for manifest in manifests {
        let decoded = (|| -> Result<Value> {
            let url = url::Url::parse(ORIGIN)?.join(s(&manifest, "path"))?;
            if url.origin().ascii_serialization() != ORIGIN
                || !url.path().starts_with("/data/")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                bail!(
                    "The public model data location changed. The previous snapshot is preserved."
                );
            }
            if !s(&manifest, "key").is_empty() {
                let key = hex::decode(s(&manifest, "key"))
                    .map_err(|_| anyhow!("The public model data key changed."))?;
                if key.len() != 32 {
                    bail!("The public model data key changed.");
                }
                let iv = Sha256::digest(&key);
                let encrypted = remote.bytes(url.as_str(), 64 * 1024 * 1024, 45, true)?;
                let cipher = Aes256Gcm::new_from_slice(&key)
                    .map_err(|_| anyhow!("The public manifest key is invalid."))?;
                let compressed=cipher.decrypt(Nonce::from_slice(&iv[..12]),encrypted.as_slice()).map_err(|_|anyhow!("The public model manifest could not be authenticated. The previous snapshot is preserved."))?;
                let mut decoded = Vec::new();
                GzDecoder::new(compressed.as_slice())
                    .take(64 * 1024 * 1024 + 1)
                    .read_to_end(&mut decoded)?;
                if decoded.len() > 64 * 1024 * 1024 {
                    bail!("The public model manifest exceeds its size limit.");
                }
                Ok(serde_json::from_slice::<Value>(&decoded)?)
            } else {
                Ok(serde_json::from_str::<Value>(&remote.text(url.as_str())?)?)
            }
        })();
        match decoded {
            Ok(mut payload) => {
                clean(&mut payload);
                walk(&payload, &mut Vec::new(), &mut collections);
            }
            Err(error) => last_error = Some(error),
        }
    }
    collections.sort_by_key(|rows| std::cmp::Reverse(rows.len()));
    if let Some(mut rows) = collections.into_iter().find(|rows| rows.len() >= 100) {
        for row in &mut rows {
            clean(row);
        }
        return Ok(rows);
    }
    if let Some(error) = last_error {
        return Err(error);
    }
    bail!(
        "Automatic source recovery could not find a complete scored model catalog. The previous snapshot is preserved."
    )
}

fn find_detail<'a>(node: &'a Value, id: &str) -> Option<&'a Value> {
    match node {
        Value::Object(map) => {
            let current = (s(node, "id") == id
                && node["slug"].is_string()
                && node.get("intelligenceIndex").is_some()
                && node["creator"].is_object())
            .then_some(node);
            // Detail records can omit costs or token counts. Prefer the richest matching
            // record over navigation summaries without requiring optional measurements.
            current
                .into_iter()
                .chain(map.values().filter_map(|v| find_detail(v, id)))
                .max_by_key(|v| v.as_object().map_or(0, |m| m.len()))
        }
        Value::Array(values) => values
            .iter()
            .filter_map(|v| find_detail(v, id))
            .max_by_key(|v| v.as_object().map_or(0, |m| m.len())),
        _ => None,
    }
}

fn detail(remote: &Remote, slug: &str, id: &str) -> Result<Value> {
    for attempt in 0..3 {
        let url = url::Url::parse(ORIGIN)?.join(&format!("/models/{slug}"))?;
        let records = page(&remote.text(url.as_str())?);
        if let Some(found) = records.iter().find_map(|v| find_detail(v, id)) {
            let mut value = found.clone();
            clean(&mut value);
            return Ok(value);
        }
        if attempt < 2 {
            remote.check()?;
            thread::sleep(Duration::from_millis(500 * (1 << attempt)));
        }
    }
    bail!(
        "The public details for {slug} are temporarily unavailable. The previous snapshot is preserved."
    )
}
fn changed(source: &Value, detail: &Value) -> bool {
    !detail.is_null()
        && (source.get("intelligenceIndex") != detail.get("intelligenceIndex")
            || source.get("intelligenceIndexIsEstimated")
                != detail.get("intelligenceIndexIsEstimated")
            || source["id"].is_string() && source["id"] != detail["id"])
}
// Preserve the 500-answer-token metric when AA publishes its components instead
// of a precomputed total. First-answer latency already includes reasoning time.
fn response_time(model: &Value) -> Option<(f64, &'static str)> {
    for path in [
        vec!["endToEndTime"],
        vec!["medianEndToEndResponseTimeSeconds"],
        vec!["endToEndResponseTime", "total"],
        vec![
            "performanceByPromptType",
            "long",
            "medianEndToEndResponseTime",
        ],
    ] {
        if let Some(value) = num(model, &path).filter(|v| *v >= 0.) {
            return Some((value, "published-total"));
        }
    }
    for measurements in [model, &model["headline"]] {
        let speed = n(measurements, "outputSpeed").filter(|v| *v > 0.);
        let latency = n(measurements, "timeToFirstAnswerToken")
            .or_else(|| {
                (!b(model, "isReasoning"))
                    .then(|| n(measurements, "timeToFirstToken"))
                    .flatten()
            })
            .filter(|v| *v >= 0.);
        if let (Some(speed), Some(latency)) = (speed, latency) {
            let seconds = latency + 500. / speed;
            if seconds.is_finite() {
                return Some((seconds, "first-answer-latency-plus-500-tokens"));
            }
        }
    }
    None
}
fn join(source: &Value, detail: &Value) -> Option<Value> {
    if !eligible_source(source) || !detail.is_null() && !eligible_source(detail) {
        return None;
    }
    let different = changed(source, detail);
    let mut model = if different {
        detail.clone()
    } else {
        merge(detail, source)
    };
    for field in ["Id", "Slug", "Name", "Color", "Logo", "Country"] {
        let key = format!("modelCreator{field}");
        let lower = field[0..1].to_lowercase() + &field[1..];
        model[&key] = if different {
            nullish(&[&detail["creator"][&lower], &source[&key]]).clone()
        } else {
            nullish(&[&source[&key], &detail["creator"][&lower]]).clone()
        };
    }
    model["slug"] = nullish(&[&detail["slug"], &source["slug"]]).clone();
    model["id"] = nullish(&[&model["id"], &detail["id"]]).clone();
    model["name"] = nullish(&[&model["name"], &detail["name"]]).clone();
    model["intelligenceIndexCostTotal"] = json!(
        nullish(&[
            &model["intelligenceIndexCostTotal"],
            &detail["intelligenceIndexCost"]["total"]
        ])
        .as_f64()
        .filter(|v| v.is_finite())
    );
    if model["intelligenceIndexTokenCounts"].is_null() {
        model["intelligenceIndexTokenCounts"] = json!({"inputTokens":num(detail,&["canonicalIntelligenceIndexTokenCount","input"]),"outputTokens":num(detail,&["canonicalIntelligenceIndexTokenCount","output"]),"reasoningTokens":num(detail,&["canonicalIntelligenceIndexTokenCount","reasoning"]),"answerTokens":num(detail,&["canonicalIntelligenceIndexTokenCount","answer"])});
    }
    let timing = response_time(&model);
    model["medianEndToEndResponseTimeSeconds"] = json!(timing.map(|v| v.0));
    model["responseTimeBasis"] = json!(timing.map(|v| v.1));
    model["totalParameters"] = json!(
        nullish(&[&model["totalParameters"], &detail["parameters"]])
            .as_f64()
            .filter(|v| v.is_finite())
    );
    model["inferenceParametersActiveBillions"] = json!(
        nullish(&[
            &model["inferenceParametersActiveBillions"],
            &detail["inferenceParametersActiveBillions"]
        ])
        .as_f64()
        .filter(|v| v.is_finite())
    );
    model["modelWeightsSourceUrl"] = nullish(&[
        &model["modelWeightsSourceUrl"],
        &detail["modelWeightsSourceUrl"],
    ])
    .clone();
    Some(model)
}
pub fn size_metadata(source: &Value) -> Value {
    let weights = url::Url::parse(s(source, "modelWeightsSourceUrl"))
        .ok()
        .filter(|u| u.scheme() == "https" && u.username().is_empty() && u.password().is_none())
        .map(|u| u.to_string());
    json!({"parameters":nullish(&[&source["totalParameters"],&source["parameters"]]).as_f64().filter(|v|v.is_finite()&&*v>0.),"activeParameters":n(source,"inferenceParametersActiveBillions").filter(|v|*v>0.),"weightsUrl":weights})
}
fn model(raw: &Value, catalog: &Value, detail: &Value) -> Value {
    let base = nullish(&[
        &detail["shortName"],
        &catalog["name"],
        &raw["shortName"],
        &raw["name"],
    ])
    .as_str()
    .unwrap_or(s(raw, "slug"));
    let effort = nullish(&[&detail["effort"]["label"], &catalog["effort"]["label"]])
        .as_str()
        .unwrap_or("");
    let name = if !effort.is_empty()
        && !base
            .to_lowercase()
            .contains(&format!("({})", effort.to_lowercase()))
        && !base.contains('(')
    {
        format!("{base} ({effort})")
    } else {
        base.to_string()
    };
    let logo = Path::new(raw["modelCreatorLogo"].as_str().unwrap_or("unknown.svg"))
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("unknown.svg");
    let mut result = merge(
        &metadata(catalog, detail),
        &json!({"id":raw["id"],"slug":raw["slug"],"name":name,"fullName":raw["name"],"provider":if s(raw,"modelCreatorSlug")=="zai"{json!("zAI")}else{raw["modelCreatorName"].clone()},"providerSlug":raw["modelCreatorSlug"],"color":raw["modelCreatorColor"],"logo":format!("/logos/{logo}"),"country":raw["modelCreatorCountry"],"intelligence":n(raw,"intelligenceIndex"),"estimated":false,"sourceUrl":format!("{ORIGIN}/models/{}",s(raw,"slug"))}),
    );
    for (output, input) in [
        ("cost", "intelligenceIndexCostTotal"),
        ("time", "medianEndToEndResponseTimeSeconds"),
        ("evaluationTimePerTask", "intelligenceIndexTimePerTask"),
        ("priceInputPerMillion", "price1mInputTokens"),
        ("priceOutputPerMillion", "price1mOutputTokens"),
        ("context", "contextWindowTokens"),
    ] {
        result[output] = json!(n(raw, input));
    }
    for (output, input) in [
        ("tokens", "outputTokens"),
        ("reasoningTokens", "reasoningTokens"),
        ("answerTokens", "answerTokens"),
    ] {
        result[output] = json!(num(raw, &["intelligenceIndexTokenCounts", input]));
    }
    for (output, input) in [
        ("openWeights", "isOpenWeights"),
        ("reasoning", "isReasoning"),
        ("deprecated", "deprecated"),
        ("image", "inputModalityImage"),
        ("video", "inputModalityVideo"),
        ("speech", "inputModalitySpeech"),
    ] {
        result[output] = json!(b(raw, input));
    }
    for (output, input) in [
        ("license", "licenseName"),
        ("licenseUrl", "licenseUrl"),
        ("releaseDate", "releaseDate"),
        ("size", "sizeClass"),
    ] {
        result[output] = raw[input].clone();
    }
    result["responseTimeBasis"] = raw["responseTimeBasis"].clone();
    result["licenseCategory"] = detail
        .get("openSourceCategorization")
        .filter(|v| !v.is_null())
        .cloned()
        .unwrap_or_else(|| {
            json!(if b(raw, "isOpenWeights") {
                "open_weights"
            } else {
                "proprietary"
            })
        });
    merge(&result, &size_metadata(raw))
}
fn validate(models: &[Value]) -> Result<()> {
    let mut ids = HashSet::new();
    for model in models {
        if s(model, "id").trim().is_empty() {
            bail!(
                "The public model details require valid model IDs. The previous snapshot is preserved."
            );
        }
        if !ids.insert(s(model, "id")) {
            bail!(
                "The public data contains duplicate model IDs. The previous snapshot is preserved."
            );
        }
    }
    Ok(())
}
pub fn sync(store: &Store, remote: &Remote) -> Result<Value> {
    fs::create_dir_all(store.root.join("data"))?;
    fs::create_dir_all(store.root.join("public/logos"))?;
    let pages = thread::scope(|scope| {
        let a = scope.spawn(|| remote.text(&format!("{ORIGIN}/leaderboards/models")));
        let b = scope.spawn(|| remote.text(&format!("{ORIGIN}/models")));
        Ok::<_, anyhow::Error>((
            a.join().map_err(|_| anyhow!("Source download failed"))?,
            b.join().map_err(|_| anyhow!("Source download failed"))?,
        ))
    })?;
    if pages.0.is_err() && pages.1.is_err() {
        bail!("Both public model sources are unavailable. The previous snapshot is preserved.");
    }
    let pages = (pages.0.unwrap_or_default(), pages.1.unwrap_or_default());
    let leaderboard = page(&pages.0);
    let listing = page(&pages.1);
    let (source, details) = thread::scope(|scope| {
        let a = scope.spawn(|| page_models(remote, &leaderboard));
        let b = scope.spawn(|| page_models(remote, &listing));
        (
            a.join()
                .unwrap_or_else(|_| Err(anyhow!("Source manifest failed"))),
            b.join()
                .unwrap_or_else(|_| Err(anyhow!("Source manifest failed"))),
        )
    });
    // Either complete public catalog can recover the other. Never use a partial chart subset.
    let (source, details) = match (source, details) {
        (Ok(source), Ok(details)) => (source, details),
        (Ok(source), Err(_)) => (source.clone(), source),
        (Err(_), Ok(details)) => (details.clone(), details),
        (Err(a), Err(b)) => bail!(
            "Automatic source recovery failed ({a}; {b}). The previous snapshot is preserved."
        ),
    };
    let details: HashMap<_, _> = details
        .into_iter()
        .map(|v| (s(&v, "id").to_string(), v))
        .collect();
    let candidates: Vec<_> = source
        .into_iter()
        .filter(eligible_source)
        .map(|mut source| {
            // A release slug identifies a family, not a reasoning variant. Join stable IDs first.
            if let Some(detail) = details.get(s(&source, "id")) {
                if !detail["slug"].is_null() {
                    source["slug"] = detail["slug"].clone();
                }
            }
            source
        })
        .collect();
    let mut raw = Vec::new();
    for batch in candidates.chunks(6) {
        let results = thread::scope(|scope| {
            batch
                .iter()
                .map(|source| {
                    let details = &details;
                    scope.spawn(move || -> Result<Option<Value>> {
                        let mut d = details
                            .get(s(source, "id"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        let needs = !source["id"].is_string()
                            || source.get("intelligenceIndexCostTotal").is_none()
                            || source.get("intelligenceIndexTokenCounts").is_none()
                            || s(source, "slug") == "muse-spark-1-3-xhigh"
                            || b(source, "isOpenWeights")
                                && (source.get("inferenceParametersActiveBillions").is_none()
                                    || source.get("modelWeightsSourceUrl").is_none());
                        let size = b(source, "isOpenWeights")
                            && !d.is_null()
                            && ["inferenceParametersActiveBillions", "modelWeightsSourceUrl"]
                                .iter()
                                .any(|key| d.get(*key).is_none() && source.get(*key).is_none());
                        if needs
                            && (d.is_null()
                                || d.get("intelligenceIndexCost").is_none()
                                || d.get("canonicalIntelligenceIndexTokenCount").is_none())
                            || size
                            || changed(source, &d)
                        {
                            let slug = if s(source, "slug").is_empty() {
                                text(source, &["release", "slug"])
                            } else { s(source, "slug") };
                            if slug.is_empty() { bail!("A scored model has no public detail address. The previous snapshot is preserved."); }
                            d = detail(remote, slug, s(source, "id"))?;
                        }
                        Ok(join(source, &d))
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|h| h.join().map_err(|_| anyhow!("Model details failed"))?)
                .collect::<Result<Vec<_>>>()
        })?;
        raw.extend(results.into_iter().flatten().filter(eligible_source));
    }
    if raw.len() < 10 {
        bail!(
            "The public page does not contain enough scored models. The saved snapshot is preserved."
        );
    }
    validate(&raw)?;
    let mut initial = Vec::new();
    for record in &listing {
        find_arrays(record, "initialModels", &mut initial);
    }
    let initial = initial.into_iter().find(|v| {
        array(v)
            .iter()
            .any(|m| m["id"].is_string() && m.get("intelligenceIndex").is_some())
    });
    let by_id: HashMap<_, _> = raw.iter().map(|m| (s(m, "id"), m)).collect();
    let defaults: Vec<Value> = initial
        .map(|v| {
            array(v)
                .iter()
                .filter(|m| eligible_source(m))
                .filter_map(|m| by_id.get(s(m, "id")).map(|v| (*v).clone()))
                .collect()
        })
        .unwrap_or_default();
    let catalog: Vec<_> = raw
        .iter()
        .map(|m| {
            merge(
                &json!({"id":m["id"],"slug":m["slug"],"name":m["name"],"shortName":m["shortName"]}),
                &metadata(&Value::Null, m),
            )
        })
        .collect();
    let catalog_by_slug: HashMap<_, _> = catalog.iter().map(|m| (s(m, "slug"), m)).collect();
    let default_by_slug: HashMap<_, _> = defaults.iter().map(|m| (s(m, "slug"), m)).collect();
    let models: Vec<_> = raw
        .iter()
        .map(|m| {
            model(
                m,
                catalog_by_slug
                    .get(s(m, "slug"))
                    .copied()
                    .unwrap_or(&Value::Null),
                default_by_slug.get(s(m, "slug")).copied().unwrap_or(m),
            )
        })
        .filter(eligible)
        .collect();
    let previous = store.snapshot().unwrap_or_else(|_| json!({"models":[]}));
    let mut models = enrichment::enrich(remote, models, array(&previous["models"]))?;
    let variants = pricing_variants(&models, &raw);
    models.extend(variants.iter().cloned());
    validate(&models)?;
    let complete: Vec<_> = models
        .iter()
        .filter(|m| {
            n(m, "intelligence").is_some() && n(m, "cost").is_some() && n(m, "tokens").is_some()
        })
        .collect();
    if complete.len() < 10 {
        bail!("The public data is missing comparison fields. The previous snapshot is preserved.");
    }
    if !models.iter().any(|m| n(m, "time").is_some()) {
        bail!(
            "The public source did not provide usable response times after automatic recovery. The previous snapshot is preserved."
        );
    }
    let logos: HashSet<_> = raw
        .iter()
        .filter_map(|m| m["modelCreatorLogo"].as_str())
        .collect();
    let logos: Vec<_> = logos.into_iter().collect();
    let mut missing = 0;
    for batch in logos.chunks(8) {
        missing += thread::scope(|scope| {
            batch
                .iter()
                .map(|name| {
                    scope.spawn(move || -> bool {
                        let Some(base) = Path::new(name).file_name() else {
                            return false;
                        };
                        let path = store.root.join("public/logos").join(base);
                        if path.is_file() {
                            return true;
                        }
                        let result = (|| -> Result<()> {
                            let url =
                                url::Url::parse(&format!("{ORIGIN}/img/logos/"))?.join(name)?;
                            fs::write(path, remote.bytes(url.as_str(), 2_000_000, 15, false)?)?;
                            Ok(())
                        })();
                        result.is_ok()
                    })
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|handle| handle.join().unwrap_or(false))
                .filter(|success| !*success)
                .count()
        });
    }
    let version_pattern = Regex::new(r"Intelligence Index v(\d+\.\d+(?:\.\d+)?)").unwrap();
    let version = version_pattern
        .captures(&pages.1)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| s(&previous, "indexVersion").to_string());
    let mut default_ids: Vec<_> = defaults.iter().map(|m| s(m, "id").to_string()).collect();
    let selected: HashSet<_> = default_ids.iter().cloned().collect();
    default_ids.extend(
        variants
            .iter()
            .filter(|m| selected.contains(text(m, &["pricing", "sourceModelId"])))
            .map(|m| s(m, "id").to_string()),
    );
    let mut seen = HashSet::new();
    default_ids.retain(|id| seen.insert(id.clone()));
    let snapshot = json!({"updatedAt":now(),"indexVersion":version,"dataPolicy":POLICY,"sourceUrl":format!("{ORIGIN}/leaderboards/models"),"chartSourceUrl":format!("{ORIGIN}/models"),"sourceDescription":"Full precision Artificial Analysis results with separately calculated contributor pricing variants","responseTimeDescription":"Seconds for a 500-token answer, using a published total or published first-answer latency plus 500 divided by output tokens per second. First-answer latency includes reasoning time.","defaultModelIds":default_ids,"normalization":{"cost":complete.iter().filter_map(|m|n(m,"cost")).fold(0_f64,f64::max),"tokens":complete.iter().filter_map(|m|n(m,"tokens")).fold(0_f64,f64::max)},"models":models});
    let parameter_metadata = parameter_metadata(&models);
    let export = json!({"updatedAt":snapshot["updatedAt"],"dataPolicy":POLICY,"models":raw,"defaults":defaults,"catalog":catalog,"parameterMetadata":parameter_metadata,"pricingVariants":variants});
    let result = json!({"modelCount":models.len(),"completeCount":complete.len(),"updatedAt":snapshot["updatedAt"],"missingLogos":missing});
    store.publish(snapshot, export)?;
    Ok(result)
}
pub fn parameter_metadata(models: &[Value]) -> Vec<Value> {
    models.iter().filter(|m|!m["parameterSources"].is_null()).map(|m|json!({"id":m["id"],"parameters":m["parameters"],"activeParameters":m["activeParameters"],"parameterSources":m["parameterSources"],"parameterRevision":m["parameterRevision"]})).collect()
}
pub fn enrich_saved(store: &Store, remote: &Remote) -> Result<Value> {
    let mut snapshot = store.snapshot()?;
    let mut raw = store.raw()?;
    let previous: Vec<_> = array(&snapshot["models"])
        .iter()
        .filter(|m| eligible(m))
        .cloned()
        .collect();
    let source: HashMap<_, _> = array(&raw["models"])
        .iter()
        .map(|m| (s(m, "id"), m))
        .collect();
    let baseline: Vec<_> = previous
        .iter()
        .map(|m| {
            let mut model = m.clone();
            for key in [
                "parameterSources",
                "parameterRevision",
                "parameterEnrichmentVersion",
            ] {
                model.as_object_mut().unwrap().remove(key);
            }
            let id = at(m, &["pricing", "sourceModelId"])
                .as_str()
                .unwrap_or(s(m, "id"));
            merge(
                &model,
                &size_metadata(source.get(id).copied().unwrap_or(&Value::Null)),
            )
        })
        .collect();
    let models = enrichment::enrich(remote, baseline, &previous)?;
    snapshot["models"] = json!(models);
    raw["parameterMetadata"] = json!(parameter_metadata(&models));
    let by_id: HashMap<_, _> = models.iter().map(|m| (s(m, "id"), m)).collect();
    for variant in raw["pricingVariants"].as_array_mut().into_iter().flatten() {
        if let Some(model) = by_id.get(s(variant, "id")) {
            for key in [
                "parameters",
                "activeParameters",
                "parameterSources",
                "parameterRevision",
                "parameterEnrichmentVersion",
                "weightsUrl",
            ] {
                variant[key] = model[key].clone();
            }
        }
    }
    let result = json!({"modelCount":models.len(),"updatedAt":snapshot["updatedAt"]});
    store.publish(snapshot, raw)?;
    Ok(result)
}
