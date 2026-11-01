//! Everything the CLI prints, in English and Persian.
//!
//! Machine-readable output (JSON) is always English. Human-readable output
//! follows `--lang`, then `SEAM_LANG`, then `LANG` and `LC_ALL` when they
//! name Persian. Sentences written by the analysis come with their Persian
//! renderings; anything without one is shown in English.

use std::collections::BTreeMap;

use seam_adapters::Support;
use seam_core::{
    Classification, ConfidenceLevel, FindingKind, Lifecycle, MappingStatus, Role, Severity, Side,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Lang {
    En,
    Fa,
}

impl Lang {
    /// `--lang` wins; otherwise the environment decides.
    pub fn resolve(flag: Option<Lang>) -> Lang {
        if let Some(lang) = flag {
            return lang;
        }
        for var in ["SEAM_LANG", "LC_ALL", "LANG"] {
            if let Ok(value) = std::env::var(var) {
                let value = value.to_ascii_lowercase();
                if value.starts_with("fa") {
                    return Lang::Fa;
                }
                if var == "SEAM_LANG" && value.starts_with("en") {
                    return Lang::En;
                }
            }
        }
        Lang::En
    }
}

pub struct Text {
    pub lang: Lang,
    translations: BTreeMap<String, String>,
}

impl Text {
    pub fn new(lang: Lang, translations: BTreeMap<String, String>) -> Text {
        Text { lang, translations }
    }

    pub fn fa(&self) -> bool {
        self.lang == Lang::Fa
    }

    fn pick(&self, en: &'static str, fa: &'static str) -> &'static str {
        if self.fa() { fa } else { en }
    }

    /// A sentence from the analysis, in the chosen language when a rendering exists.
    pub fn sentence<'a>(&'a self, english: &'a str) -> &'a str {
        if self.fa()
            && let Some(persian) = self.translations.get(english)
        {
            return persian;
        }
        english
    }

    pub fn class(&self, c: Classification) -> String {
        if !self.fa() {
            return c.to_string();
        }
        let name = match c {
            Classification::Verified => "تأییدشده",
            Classification::Parsed => "تجزیه‌شده",
            Classification::Inferred => "استنتاج‌شده",
            Classification::Correlated => "هم‌بسته",
            Classification::Unknown => "نامعلوم",
        };
        format!("{name} ({c})")
    }

    pub fn class_word(&self, c: &str) -> String {
        match c {
            "VERIFIED" => self.class(Classification::Verified),
            "PARSED" => self.class(Classification::Parsed),
            "INFERRED" => self.class(Classification::Inferred),
            "CORRELATED" => self.class(Classification::Correlated),
            "UNKNOWN" => self.class(Classification::Unknown),
            other => other.to_owned(),
        }
    }

    pub fn state(&self, s: Lifecycle) -> String {
        if !self.fa() {
            return s.to_string();
        }
        match s {
            Lifecycle::Discovered => "تازه‌یافته",
            Lifecycle::Reconfirmed => "دوباره تأییدشده",
            Lifecycle::Changed => "تغییرکرده",
            Lifecycle::PotentiallyIncompatible => "احتمالاً ناسازگار",
            Lifecycle::Unresolved => "حل‌نشده",
            Lifecycle::Removed => "حذف‌شده",
        }
        .to_owned()
    }

    pub fn state_word(&self, s: &str) -> String {
        let parsed: Option<Lifecycle> =
            serde_json::from_value(serde_json::Value::String(s.to_owned())).ok();
        parsed.map_or_else(|| s.to_owned(), |l| self.state(l))
    }

    pub fn confidence(&self, c: ConfidenceLevel) -> &'static str {
        match c {
            ConfidenceLevel::Strong => self.pick("strong", "قوی"),
            ConfidenceLevel::Moderate => self.pick("moderate", "متوسط"),
            ConfidenceLevel::Weak => self.pick("weak", "ضعیف"),
        }
    }

    pub fn confidence_word(&self, c: &str) -> String {
        match c {
            "strong" => self.confidence(ConfidenceLevel::Strong).to_owned(),
            "moderate" => self.confidence(ConfidenceLevel::Moderate).to_owned(),
            "weak" => self.confidence(ConfidenceLevel::Weak).to_owned(),
            other => other.to_owned(),
        }
    }

    pub fn severity(&self, s: Severity) -> &'static str {
        match s {
            Severity::Breaking => self.pick("breaking", "شکننده"),
            Severity::Risk => self.pick("risk", "ریسک"),
            Severity::Info => self.pick("info", "اطلاع"),
        }
    }

    pub fn finding_kind(&self, k: FindingKind) -> &'static str {
        match k {
            FindingKind::MissingField => self.pick("missing field", "فیلد گم‌شده"),
            FindingKind::TypeMismatch => self.pick("type mismatch", "نوع ناسازگار"),
            FindingKind::NullableMismatch => self.pick("nullable mismatch", "ناسازگاری null"),
            FindingKind::ContractDrift => self.pick("contract drift", "انحراف از قرارداد"),
            FindingKind::NoProvider => self.pick("no provider", "بدون ارائه‌دهنده"),
            FindingKind::Untested => self.pick("untested", "بدون آزمون"),
        }
    }

    pub fn mapping(&self, m: MappingStatus) -> &'static str {
        match m {
            MappingStatus::Aligned => self.pick("aligned", "هم‌راستا"),
            MappingStatus::Missing => self.pick("missing", "گم‌شده"),
            MappingStatus::OptionalUnsent => {
                self.pick("optional, not sent", "اختیاری، فرستاده نمی‌شود")
            }
            MappingStatus::Unread => self.pick("not read", "خوانده نمی‌شود"),
            MappingStatus::TypeMismatch => self.pick("type mismatch", "نوع ناسازگار"),
            MappingStatus::NullableMismatch => self.pick("nullable mismatch", "ناسازگاری null"),
            MappingStatus::ContractDrift => self.pick("contract drift", "انحراف از قرارداد"),
            MappingStatus::Indeterminate => self.pick("undecided", "نامعین"),
        }
    }

    pub fn role(&self, r: Role) -> &'static str {
        match r {
            Role::ConsumerCall => self.pick("consumer call", "فراخوانی مصرف‌کننده"),
            Role::ProducerRoute => self.pick("producer route", "مسیر تولیدکننده"),
            Role::ContractOperation => self.pick("contract operation", "عملیات قرارداد"),
            Role::ConsumerType => self.pick("consumer type", "نوع مصرف‌کننده"),
            Role::ProducerModel => self.pick("producer model", "مدل تولیدکننده"),
            Role::ContractSchema => self.pick("contract schema", "طرح قرارداد"),
            Role::Resolution => self.pick("resolution", "گام اتصال"),
            Role::Assumption => self.pick("assumption", "فرض"),
            Role::Test => self.pick("test", "آزمون"),
        }
    }

    pub fn side(&self, s: Side) -> &'static str {
        match s {
            Side::Consumer => self.pick("consumer", "مصرف‌کننده"),
            Side::Producer => self.pick("producer", "تولیدکننده"),
        }
    }

    pub fn support(&self, s: Support) -> &'static str {
        match s {
            Support::Read => self.pick("read", "خوانده می‌شود"),
            Support::Resolved => self.pick("resolved", "پیگیری می‌شود"),
            Support::NotRead => self.pick("not handled", "پشتیبانی نمی‌شود"),
        }
    }

    // Labels ----------------------------------------------------------------

    pub fn l_class(&self) -> &'static str {
        self.pick("Class", "رده")
    }
    pub fn l_confidence(&self) -> &'static str {
        self.pick("Confidence", "اطمینان")
    }
    pub fn l_state(&self) -> &'static str {
        self.pick("State", "وضعیت")
    }
    pub fn l_contract(&self) -> &'static str {
        self.pick("Contract", "قرارداد")
    }
    pub fn l_why(&self) -> &'static str {
        self.pick("Why", "چرا")
    }
    pub fn l_findings(&self) -> &'static str {
        self.pick("Findings", "یافته‌ها")
    }
    pub fn l_response_fields(&self) -> &'static str {
        self.pick("Response fields", "فیلدهای پاسخ")
    }
    pub fn l_request_fields(&self) -> &'static str {
        self.pick("Request body fields", "فیلدهای بدنهٔ درخواست")
    }
    pub fn l_assumptions(&self) -> &'static str {
        self.pick("Assumptions", "فرض‌ها")
    }
    pub fn l_tests(&self) -> &'static str {
        self.pick("Tests", "آزمون‌ها")
    }
    pub fn l_evidence(&self) -> &'static str {
        self.pick("Evidence", "شواهد")
    }
    pub fn l_none(&self) -> &'static str {
        self.pick("none", "هیچ")
    }
    pub fn l_field(&self) -> &'static str {
        self.pick("field", "فیلد")
    }
    pub fn l_consumer(&self) -> &'static str {
        self.pick("consumer", "مصرف‌کننده")
    }
    pub fn l_producer(&self) -> &'static str {
        self.pick("producer", "تولیدکننده")
    }
    pub fn l_status(&self) -> &'static str {
        self.pick("status", "وضعیت")
    }
    pub fn l_no_producer(&self) -> &'static str {
        self.pick("no producer found", "تولیدکننده‌ای پیدا نشد")
    }
    pub fn l_seam(&self) -> &'static str {
        self.pick("Seam", "درز")
    }
    pub fn l_stale(&self) -> &'static str {
        self.pick("stale", "کهنه")
    }
    pub fn l_routes_without_consumers(&self) -> &'static str {
        self.pick(
            "Routes no analysed client calls",
            "مسیرهایی که هیچ کلاینت تحلیل‌شده‌ای صدا نمی‌زند",
        )
    }
    pub fn l_skipped(&self) -> &'static str {
        self.pick("Skipped files", "فایل‌های کنارگذاشته")
    }
    pub fn l_diagnostics(&self) -> &'static str {
        self.pick("Diagnostics", "هشدارهای خواندن")
    }

    // Sentences -------------------------------------------------------------

    pub fn scanned(&self, root: &str, ms: u64) -> String {
        match self.lang {
            Lang::En => format!("scanned {root} in {ms} ms"),
            Lang::Fa => format!("{root} در {ms} میلی‌ثانیه اسکن شد"),
        }
    }

    pub fn counts(&self, files: usize, components: usize, routes: usize, seams: usize) -> String {
        match self.lang {
            Lang::En => format!(
                "{files} files · {components} components · {routes} routes · {seams} {}",
                if seams == 1 { "Seam" } else { "Seams" }
            ),
            Lang::Fa => format!("{files} فایل · {components} مؤلفه · {routes} مسیر · {seams} درز"),
        }
    }

    pub fn stored(&self, path: &str) -> String {
        match self.lang {
            Lang::En => format!("Recorded in {path}."),
            Lang::Fa => format!("در {path} ثبت شد."),
        }
    }

    pub fn not_stored(&self) -> &'static str {
        self.pick("Not recorded (--no-store).", "ثبت نشد (--no-store).")
    }

    pub fn removed_since(&self, n: usize) -> String {
        match self.lang {
            Lang::En => format!(
                "{n} Seam{} from the previous scan {} gone.",
                if n == 1 { "" } else { "s" },
                if n == 1 { "is" } else { "are" }
            ),
            Lang::Fa => format!("{n} درز از اسکن قبلی دیگر وجود ندارد."),
        }
    }

    pub fn by_class(&self, parts: &[(String, usize)]) -> String {
        let list = parts
            .iter()
            .map(|(c, n)| format!("{n} {c}"))
            .collect::<Vec<_>>();
        match self.lang {
            Lang::En => format!("Classes: {}", list.join(", ")),
            Lang::Fa => format!("رده‌ها: {}", list.join("، ")),
        }
    }

    pub fn by_severity(&self, breaking: usize, risk: usize, info: usize) -> String {
        match self.lang {
            Lang::En => format!("Findings: {breaking} breaking, {risk} risk, {info} info"),
            Lang::Fa => format!("یافته‌ها: {breaking} شکننده، {risk} ریسک، {info} اطلاع"),
        }
    }

    pub fn explain_hint(&self, id: &str) -> String {
        match self.lang {
            Lang::En => format!("See the evidence behind a Seam with: seam explain {id}"),
            Lang::Fa => format!("برای دیدن شواهد یک درز: seam explain {id}"),
        }
    }

    pub fn no_seams(&self) -> &'static str {
        self.pick(
            "No Seams found. SEAM reads TypeScript and JavaScript clients, FastAPI services, OpenAPI and Docker Compose; run `seam languages` for the details.",
            "هیچ درزی پیدا نشد. SEAM کلاینت‌های TypeScript و JavaScript، سرویس‌های FastAPI، OpenAPI و Docker Compose را می‌خواند؛ برای جزئیات `seam languages` را اجرا کنید.",
        )
    }

    pub fn no_snapshot(&self, store: &str) -> String {
        match self.lang {
            Lang::En => format!("There is no recorded scan in {store}. Run `seam scan` first."),
            Lang::Fa => format!("در {store} هیچ اسکن ثبت‌شده‌ای نیست. اول `seam scan` را اجرا کنید."),
        }
    }

    pub fn last_scan(&self, at: &str, root: &str) -> String {
        match self.lang {
            Lang::En => format!("Ledger of the scan at {at} of {root}"),
            Lang::Fa => format!("دفتر درزها، از اسکن {at} روی {root}"),
        }
    }

    pub fn not_found(&self, id: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "No Seam with the id {id} is recorded. `seam seams` lists the recorded ones."
            ),
            Lang::Fa => format!(
                "درزی با شناسهٔ {id} ثبت نشده است. `seam seams` درزهای ثبت‌شده را فهرست می‌کند."
            ),
        }
    }

    pub fn ambiguous(&self, id: &str, n: usize) -> String {
        match self.lang {
            Lang::En => format!("{n} Seams start with {id}; type more of the id."),
            Lang::Fa => format!("{n} درز با {id} شروع می‌شوند؛ بخش بیشتری از شناسه را بنویسید."),
        }
    }

    pub fn stale_warning(&self, n: usize) -> String {
        match self.lang {
            Lang::En => format!(
                "{n} cited file{} changed since the scan; this Seam is stale until the next `seam scan`.",
                if n == 1 { "" } else { "s" }
            ),
            Lang::Fa => format!(
                "{n} فایلِ استنادشده پس از اسکن تغییر کرده است؛ این درز تا اسکن بعدی کهنه به حساب می‌آید."
            ),
        }
    }

    pub fn stale_class(&self, class: Classification) -> String {
        match self.lang {
            Lang::En => format!("STALE (was {class})"),
            Lang::Fa => format!("کهنه (پیش‌تر {})", self.class(class)),
        }
    }

    pub fn observed(&self, at: &str) -> String {
        match self.lang {
            Lang::En => format!("Observed in the scan at {at}."),
            Lang::Fa => format!("دیده‌شده در اسکن {at}."),
        }
    }

    pub fn cancelled(&self) -> &'static str {
        self.pick(
            "The scan was cancelled; nothing was recorded.",
            "اسکن لغو شد؛ چیزی ثبت نشد.",
        )
    }

    pub fn failed_on(&self, n: usize, level: &str) -> String {
        match self.lang {
            Lang::En => format!(
                "{n} finding{} at or above `{level}`.",
                if n == 1 { "" } else { "s" }
            ),
            Lang::Fa => format!("{n} یافته در سطح `{level}` یا بالاتر."),
        }
    }

    pub fn error_prefix(&self) -> &'static str {
        self.pick("error", "خطا")
    }

    pub fn languages_intro(&self) -> &'static str {
        self.pick(
            "What each adapter reads. \"read\" comes from one file; \"resolved\" is followed across files; \"not handled\" turns into an assumption or UNKNOWN.",
            "آنچه هر مبدل می‌خواند. «خوانده می‌شود» از خود فایل می‌آید؛ «پیگیری می‌شود» در میان فایل‌ها دنبال می‌شود؛ «پشتیبانی نمی‌شود» به فرض یا نامعلوم می‌انجامد.",
        )
    }

    pub fn report_title(&self, name: &str) -> String {
        match self.lang {
            Lang::En => format!("SEAM report: {name}"),
            Lang::Fa => format!("گزارش SEAM: {name}"),
        }
    }

    pub fn report_generated(&self, version: &str, at: &str, root: &str) -> String {
        match self.lang {
            Lang::En => format!("Generated by SEAM {version} at {at} from `{root}`."),
            Lang::Fa => format!("ساخته‌شده با SEAM {version} در {at} از `{root}`."),
        }
    }
}
