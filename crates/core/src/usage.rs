//! Read-only usage aggregation. Read physical rollouts once; never replay inherited histories.
use crate::{Result, Settings, history, sessions::{self, Scanner}};
use anyhow::{Context, ensure};
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::{BTreeMap, BTreeSet, HashMap, HashSet}, fs::File, io::{BufRead, BufReader, Read}, path::PathBuf, time::SystemTime};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts { pub input: u64, pub output: u64, pub cached: Option<u64> }
impl Counts {
    fn read(v: &Value) -> Option<Self> {
        let input = v.get("input_tokens")?.as_u64()?;
        let output = v.get("output_tokens")?.as_u64()?;
        let cached = match v.get("cached_input_tokens").filter(|v| !v.is_null()) {
            Some(v) => Some(v.as_u64()?), None => None,
        };
        (input.checked_add(output)? <= 9_007_199_254_740_991 && cached.is_none_or(|n| n <= input))
            .then_some(Self {input, output, cached})
    }
    fn delta(self, prev: Self) -> Option<Self> {
        Some(Self {input:self.input.checked_sub(prev.input)?, output:self.output.checked_sub(prev.output)?,
            cached:match (self.cached, prev.cached) { (Some(a),Some(b)) => Some(a.checked_sub(b)?), _=>None }})
    }
    fn total(self) -> u64 { self.input + self.output }
    fn add(&mut self, other: Self) -> Result<()> {
        let input = self.input.checked_add(other.input).context("Token 计数溢出")?;
        let output = self.output.checked_add(other.output).context("Token 计数溢出")?;
        ensure!(input.checked_add(output).is_some_and(|n| n<=9_007_199_254_740_991), "Token 总量超过精确统计上限");
        self.input=input;self.output=output;
        self.cached = self.cached.zip(other.cached).map(|(a,b)| a+b);
        Ok(())
    }
    fn zero() -> Self { Self {cached:Some(0), ..Self::default()} }
}
#[derive(Clone)]
struct Point { time: Option<i64>, end: u64, model: String, turn: String, total: Option<Counts>, last: Option<Counts> }
#[derive(Clone, Default)]
struct Parsed { points: Vec<Point>, base: Option<(String,u64)>, invalid: usize, created: i128, forked: bool }
#[derive(Default)]
pub struct UsageCache { files: HashMap<PathBuf,(u64,SystemTime,Parsed)> }
#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
pub struct Query { pub bucket_starts: Vec<i64>, pub end: i64, pub model: Option<String> }
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Bucket { pub start: i64, #[serde(flatten)] pub counts: Counts }
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Share { pub id: String, pub name: String, #[serde(flatten)] pub counts: Counts }
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct Report {
    #[serde(flatten)] pub counts: Counts,
    pub buckets: Vec<Bucket>, pub models: Vec<Share>, pub sessions: Vec<Share>,
    pub available_models: Vec<String>, pub warnings: Vec<String>, pub scanned_files: usize,
}
fn parse(path: &std::path::Path) -> Result<Parsed> {
    sessions::no_links(path)?;
    let mut reader = BufReader::new(File::open(path)?);
    let created=std::fs::metadata(path)?.created().ok().and_then(|t|t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d|d.as_nanos() as i128).unwrap_or(i128::MAX);
    let mut out = Parsed {created,..Parsed::default()}; let mut model = String::new(); let mut turn = String::new(); let mut end = 0;
    loop {
        let mut bytes = vec![];
        let n = reader.by_ref().take(32*1024*1024+1).read_until(b'\n', &mut bytes)?;
        if n == 0 { break; }
        ensure!(n <= 32*1024*1024, "单条会话事件超过读取上限");
        end += n as u64;
        let line = std::str::from_utf8(&bytes)?.trim_start_matches('\u{feff}');
        let v: Value = match serde_json::from_str(line) { Ok(v)=>v, Err(_)=> {out.invalid+=1; continue;} };
        let p = &v["payload"];
        match v["type"].as_str() {
            Some("session_meta") => {
                out.forked = p.get("forked_from_id").is_some_and(|v| !v.is_null());
                if let Some(date)=p["timestamp"].as_str().or(v["timestamp"].as_str()).and_then(|s|DateTime::parse_from_rfc3339(s).ok()) {out.created=date.timestamp_nanos_opt().map(i128::from).unwrap_or(created); }
                if let Some(id) = p["history_base"]["thread_id"].as_str() {
                    out.base = Some((id.into(),p["history_base"]["end_byte_offset"].as_u64().context("分页历史缺少偏移")?));
                }
                if let Some(m) = p["model"].as_str() { model = m.to_owned(); }
            },
            Some("turn_context") => {
                // A context without model must not inherit the previous turn's model.
                model = p["model"].as_str().unwrap_or("").to_owned();
                turn = p["turn_id"].as_str().unwrap_or("").to_owned();
            },
            Some("event_msg") if p["type"] == "token_count" && p["info"].is_object() => {
                let total = Counts::read(&p["info"]["total_token_usage"]);
                let last = Counts::read(&p["info"]["last_token_usage"]);
                if total.is_none() && last.is_none() {out.invalid+=1; continue;}
                let time = v["timestamp"].as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok()).map(|t| t.timestamp());
                out.points.push(Point {time,end,model:model.clone(),turn:turn.clone(),total,last});
            },
            _=>{}
        }
    }
    Ok(out)
}
fn baseline<'a>(parsed: &'a Parsed, files: &HashMap<String,&'a Parsed>, depth: usize) -> Option<Counts> {
    if depth > 128 {return None;}
    let (id,cap) = parsed.base.as_ref()?;
    let base = files.get(id)?;
    let mut value=baseline(base,files,depth+1);
    let mut seen=HashSet::new();
    for p in base.points.iter().filter(|p|p.end<=*cap) {
        let key=(p.time,&p.turn,p.total.map(|c|(c.input,c.output,c.cached)),p.last.map(|c|(c.input,c.output,c.cached)));
        if !seen.insert(key) {continue;}
        if let Some(total)=p.total {value=Some(total)}
        else if let Some(last)=p.last {value.get_or_insert_with(Counts::zero).add(last).ok()?;}
    }
    value
}
impl UsageCache {
    pub fn report(&mut self, scanner: &mut Scanner, settings: &Settings, query: Query) -> Result<Report> {
        ensure!(!query.bucket_starts.is_empty() && query.bucket_starts.len() <= 32, "统计时间分组无效");
        let start = query.bucket_starts[0];
        ensure!(query.bucket_starts.windows(2).all(|p| p[0]<p[1]) && *query.bucket_starts.last().unwrap()<query.end
            && query.end.checked_sub(start).is_some_and(|n| n <= 32*86400), "统计日期范围无效");
        ensure!(query.model.as_ref().is_none_or(|s| s.len()<=256), "模型筛选过长");
        let scan = scanner.scan(settings,&CancellationToken::new())?;
        let mut warnings = scan.warnings;
        let mut files = vec![]; let mut seen_paths = HashSet::new();
        for session in &scan.sessions {
            for path in std::iter::once(&session.path).chain(&session.related_paths) {
                let path = sessions::owned(path, settings)?;
                if !seen_paths.insert(path.to_string_lossy().to_lowercase()) {continue;}
                let result = (|| -> Result<_> {
                    let meta = std::fs::metadata(&path)?; let stamp = meta.modified()?;
                    let cached = self.files.get(&path).filter(|(n,t,_)| *n==meta.len() && *t==stamp);
                    // ponytail: reparse changed files; use incremental offsets only if measured scans become slow.
                    let parsed = if let Some((_,_,p)) = cached {p.clone()} else {
                        let p = parse(&path)?;
                        let after = std::fs::metadata(&path)?;
                        ensure!(after.len()==meta.len() && after.modified()?==stamp, "文件正在写入，请刷新");
                        self.files.insert(path.clone(),(meta.len(),stamp,p.clone())); p
                    };
                    Ok(parsed)
                })();
                match result {
                    Ok(parsed) => files.push((path,session.id.clone(),session.title.clone(),parsed)),
                    Err(_) => warnings.push(format!("有一个会话文件暂时无法统计（{}），请刷新重试。",path.file_name().unwrap_or_default().to_string_lossy()))
                }
            }
        }
        self.files.retain(|p,_| seen_paths.contains(&p.to_string_lossy().to_lowercase()));
        // Attribute copied fork history to the original/earlier physical history first.
        files.sort_by(|a,b| (a.3.forked,a.3.created,&a.0).cmp(&(b.3.forked,b.3.created,&b.0)));
        let by_id: HashMap<_,_> = files.iter().filter_map(|(path,_,_,p)|history::rollout_id(path).map(|id|(id,p))).collect();
        let mut report = Report {counts:Counts::zero(),buckets:query.bucket_starts.iter().map(|&start|Bucket {start,counts:Counts::zero()}).collect(),
            models:vec![],sessions:vec![],available_models:vec![],warnings,scanned_files:files.len()};
        let mut models: BTreeMap<String,Counts> = BTreeMap::new();
        let mut rows: BTreeMap<String,(String,Counts)> = BTreeMap::new();
        let mut available = BTreeSet::new(); let mut fingerprints = HashSet::new(); let mut skipped = 0;
        for (_,id,title,parsed) in &files {
            let mut previous = baseline(parsed,&by_id,0);
            let inherited_unknown = parsed.base.is_some() && previous.is_none();
            skipped += parsed.invalid;
            let mut local_events = HashSet::new();
            for point in &parsed.points {
                let signature = (point.time,&point.turn,&point.model,point.total.map(|c|(c.input,c.output,c.cached)),point.last.map(|c|(c.input,c.output,c.cached)));
                if !local_events.insert(signature) {continue;}
                let counts = if let Some(total) = point.total {
                    let old = previous.replace(total);
                    if old == Some(total) {continue;} // Repeated quota updates are not new usage.
                    match old {
                        Some(prev) => total.delta(prev).or(point.last),
                        None if inherited_unknown => point.last,
                        None => point.last.or(Some(total))
                    }
                } else {
                    if let (Some(prev),Some(last)) = (previous.as_mut(),point.last) {prev.add(last)?;}
                    point.last
                };
                let Some(counts) = counts else {skipped+=1;continue;};
                let Some(time) = point.time else {skipped+=1;continue;};
                if counts.cached.is_some_and(|n| n>counts.input) {skipped+=1;continue;}
                // Forks may contain identical copied events. Stable turn IDs disambiguate real requests.
                let identity = if point.turn.is_empty() {id.as_str()} else {&point.turn};
                let fingerprint = (identity,time,&point.model,point.total.map(|c|(c.input,c.output,c.cached)),point.last.map(|c|(c.input,c.output,c.cached)));
                if !fingerprints.insert(fingerprint) {continue;}
                if time < start || time >= query.end || counts.total()==0 {continue;}
                let model = if point.model.is_empty() {"未知模型"} else {&point.model};
                available.insert(model.to_owned());
                if query.model.as_ref().is_some_and(|m| m!=model) {continue;}
                let bucket = query.bucket_starts.partition_point(|&b| b<=time)-1;
                report.counts.add(counts)?; report.buckets[bucket].counts.add(counts)?;
                models.entry(model.to_owned()).or_insert_with(Counts::zero).add(counts)?;
                rows.entry(id.clone()).or_insert_with(||(title.clone(),Counts::zero())).1.add(counts)?;
            }
        }
        if skipped>0 {report.warnings.push(format!("{skipped} 条记录缺少有效用量、时间或累计基准，未纳入统计。"));}
        report.available_models = available.into_iter().collect();
        report.models = models.into_iter().map(|(id,counts)|Share{name:id.clone(),id,counts}).collect();
        report.sessions = rows.into_iter().map(|(id,(name,counts))|Share{id,name,counts}).collect();
        report.models.sort_by_key(|v|std::cmp::Reverse(v.counts.total()));
        report.sessions.sort_by_key(|v|std::cmp::Reverse(v.counts.total()));
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*; use serde_json::json; use std::fs;
    fn counts(i:u64,o:u64,c:u64)->Value {json!({"input_tokens":i,"output_tokens":o,"cached_input_tokens":c})}
    fn event(time:&str,total:Value,last:Value)->Value {json!({"timestamp":time,"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":total,"last_token_usage":last}}})}
    fn fixture() -> (tempfile::TempDir,Settings) {
        let dir=tempfile::tempdir().unwrap(); let mut settings=Settings::default();
        settings.codex_home=dir.path().to_path_buf(); fs::create_dir(dir.path().join("sessions")).unwrap(); (dir,settings)
    }
    fn write(path:&std::path::Path,events:&[Value]) {
        fs::write(path,events.iter().map(|e|format!("{e}\n")).collect::<String>()).unwrap();
    }
    fn query(model:Option<&str>)->Query { Query {bucket_starts:vec![0,86400],end:172800,model:model.map(str::to_owned)} }
    #[test]
    fn aggregates_deltas_reset_models_missing_fields_and_cache_refresh() {
        let (dir,settings)=fixture(); let path=dir.path().join("sessions/a.jsonl");
        let mut events=vec![
            json!({"type":"session_meta","payload":{"id":"one","cwd":dir.path(),"model_provider":"openai"}}),
            json!({"type":"turn_context","payload":{"model":"A","turn_id":"first"}}),
            event("1970-01-01T01:00:00Z",counts(100,20,50),counts(100,20,50)),
            event("1970-01-01T01:01:00Z",counts(100,20,50),counts(100,20,50)),
            event("1970-01-02T01:00:00Z",counts(180,40,90),counts(80,20,40)),
            json!({"type":"turn_context","payload":{"model":"B","turn_id":"second"}}),
            event("1970-01-02T02:00:00Z",counts(20,10,10),counts(20,10,10)),
            json!({"type":"turn_context","payload":{}}),
            event("1970-01-02T03:00:00Z",Value::Null,json!({"input_tokens":10,"output_tokens":5})),
        ];
        write(&path,&events);
        let mut cache=UsageCache::default(); let mut scanner=Scanner::default();
        let report=cache.report(&mut scanner,&settings,query(None)).unwrap();
        assert_eq!(report.counts,Counts {input:210,output:55,cached:None});
        assert_eq!(report.buckets[0].counts.total(),120); assert_eq!(report.buckets[1].counts.total(),145);
        assert_eq!(report.models.len(),3); assert_eq!(report.sessions.len(),1);
        let filtered=cache.report(&mut scanner,&settings,query(Some("B"))).unwrap();
        assert_eq!(filtered.counts.total(),30); assert_eq!(filtered.available_models.len(),3);
        events.push(event("1970-01-02T04:00:00Z",Value::Null,counts(10,5,0)));write(&path,&events);
        assert_eq!(cache.report(&mut scanner,&settings,query(None)).unwrap().counts.total(),280);
        assert!(cache.report(&mut scanner,&settings,Query{bucket_starts:vec![10,9],end:11,model:None}).is_err());
        assert!(Counts::read(&json!({"input_tokens":-1,"output_tokens":1})).is_none());
    }
    #[test]
    fn physical_pages_and_forked_events_are_not_double_counted() {
        let (dir,settings)=fixture();
        let base_id=uuid::Uuid::new_v4().to_string(); let child_id=uuid::Uuid::new_v4().to_string();
        let base=dir.path().join(format!("sessions/rollout_{base_id}.jsonl"));
        let first=vec![json!({"type":"session_meta","payload":{"id":"thread","session_id":"stable","history_mode":"paginated","cwd":dir.path()}}),
            json!({"type":"turn_context","payload":{"model":"A","turn_id":"shared-turn"}}),
            event("1970-01-01T01:00:00Z",counts(100,20,50),counts(100,20,50))];
        write(&base,&first);
        let cap=fs::metadata(&base).unwrap().len();
        let child=dir.path().join(format!("sessions/rollout_{child_id}.jsonl"));
        write(&child,&[
            json!({"type":"session_meta","payload":{"id":"thread","session_id":"stable","history_mode":"paginated","cwd":dir.path(),"history_base":{"thread_id":base_id,"end_byte_offset":cap}}}),
            json!({"type":"turn_context","payload":{"model":"B","turn_id":"new-turn"}}),
            event("1970-01-02T01:00:00Z",counts(180,40,90),Value::Null)]);
        let mut copied=first;copied[0]["payload"]["id"]=json!("fork");copied[0]["payload"]["forked_from_id"]=json!("thread");
        write(&dir.path().join("sessions/fork.jsonl"),&copied);
        let report=UsageCache::default().report(&mut Scanner::default(),&settings,query(None)).unwrap();
        assert_eq!(report.counts,Counts {input:180,output:40,cached:Some(90)});
        assert_eq!(report.scanned_files,3);
        assert_eq!(report.sessions.len(),1);assert_eq!(report.sessions[0].id,"thread");
    }
    #[test]
    fn last_only_events_advance_the_cumulative_baseline_and_partial_lines_are_reported() {
        let (dir,settings)=fixture();let path=dir.path().join("sessions/a.jsonl");
        let delta=event("1970-01-01T02:00:00Z",Value::Null,counts(20,5,10));
        write(&path,&[
            json!({"type":"session_meta","payload":{"id":"one","cwd":dir.path()}}),
            event("1970-01-01T01:00:00Z",counts(100,20,50),counts(100,20,50)),
            delta.clone(),delta,
            event("1970-01-02T01:00:00Z",counts(150,35,70),counts(30,10,10)),
        ]);
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(("{}\n".repeat(40)+"{bad").as_bytes()).unwrap();
        let report=UsageCache::default().report(&mut Scanner::default(),&settings,query(None)).unwrap();
        assert_eq!(report.counts,Counts{input:150,output:35,cached:Some(70)});
        assert_eq!(report.warnings.len(),1);
    }

}
