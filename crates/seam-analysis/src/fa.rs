//! Persian renderings of the sentences `seam-core` writes into findings.
//!
//! The core states each finding in English from a small set of fixed
//! sentence forms. This module recognises those forms and writes the same
//! statement in Persian; a sentence it does not recognise stays English.

fn role(word: &str) -> Option<&'static str> {
    match word {
        "consumer" => Some("مصرف‌کننده"),
        "producer" => Some("تولیدکننده"),
        "contract" => Some("قرارداد"),
        _ => None,
    }
}

fn payload(word: &str) -> Option<&'static str> {
    match word {
        "response" => Some("پاسخ"),
        "request" => Some("درخواست"),
        _ => None,
    }
}

/// The Persian form of a finding message from `seam_core::compat`.
pub fn finding(message: &str) -> Option<String> {
    let rest = message.strip_prefix("The ")?;
    let (head, after) = rest.split_once('`')?;
    let (field, tail) = after.split_once('`')?;
    let mut words = head.split_whitespace();
    let subject = words.next()?;
    let verb = words.collect::<Vec<_>>().join(" ");
    let who = role(subject)?;

    // The {reader} reads|requires `f`; the {writer}'s {payload} has no field `f`.
    if let Some(rest) = tail.strip_prefix("; the ")
        && let Some((writer, rest)) = rest.split_once("'s ")
        && let Some((load, _)) = rest.split_once(" has no field")
    {
        let action = match verb.as_str() {
            "reads" => "می‌خواند",
            "requires" => "لازم دارد",
            _ => return None,
        };
        return Some(format!(
            "{who} `{field}` را {action}؛ {} {} فیلدی به نام `{field}` ندارد.",
            payload(load)?,
            role(writer)?
        ));
    }
    // The {reader} reads `f` as A; the {writer} sends B.
    if verb == "reads"
        && let Some(rest) = tail.strip_prefix(" as ")
        && let Some((read_as, rest)) = rest.split_once("; the ")
        && let Some((writer, sent)) = rest.split_once(" sends ")
    {
        let sent = sent.strip_suffix('.').unwrap_or(sent);
        return Some(format!(
            "{who} `{field}` را از نوع {read_as} می‌خواند؛ {} {sent} می‌فرستد.",
            role(writer)?
        ));
    }
    // The {writer} may send null for `f`; the {reader}'s type does not allow null.
    if verb == "may send null for"
        && let Some(rest) = tail.strip_prefix("; the ")
        && let Some((reader, _)) = rest.split_once("'s type")
    {
        return Some(format!(
            "{who} ممکن است برای `{field}` مقدار null بفرستد؛ نوع {} مقدار null را نمی‌پذیرد.",
            role(reader)?
        ));
    }
    match (subject, verb.as_str(), tail) {
        ("producer", "sends", "; the contract does not declare it.") => Some(format!(
            "تولیدکننده `{field}` را می‌فرستد؛ قرارداد آن را تعریف نکرده است."
        )),
        ("producer", "reads", "; the contract does not declare it.") => Some(format!(
            "تولیدکننده `{field}` را می‌خواند؛ قرارداد آن را تعریف نکرده است."
        )),
        ("contract", "declares", "; the producer does not send it.") => Some(format!(
            "قرارداد `{field}` را تعریف کرده است؛ تولیدکننده آن را نمی‌فرستد."
        )),
        ("contract", "declares", "; the producer does not read it.") => Some(format!(
            "قرارداد `{field}` را تعریف کرده است؛ تولیدکننده آن را نمی‌خواند."
        )),
        ("producer", "allows null for", "; the contract does not.") => Some(format!(
            "تولیدکننده برای `{field}` مقدار null را مجاز می‌داند؛ قرارداد نه."
        )),
        ("contract", "types", _) => {
            let rest = tail.strip_prefix(" as ")?;
            let (declared, rest) = rest.split_once("; the producer has ")?;
            let has = rest.strip_suffix('.').unwrap_or(rest);
            Some(format!(
                "قرارداد نوع `{field}` را {declared} تعریف کرده است؛ تولیدکننده {has} دارد."
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_core_sentence_form_has_a_persian_form() {
        let cases = [
            "The consumer reads `name`; the producer's response has no field `name`.",
            "The producer requires `amount`; the consumer's request has no field `amount`.",
            "The consumer reads `id` as string; the producer sends integer.",
            "The producer may send null for `ledger_entry_id`; the consumer's type does not allow null.",
            "The producer sends `x`; the contract does not declare it.",
            "The contract declares `x`; the producer does not send it.",
            "The contract types `x` as string; the producer has integer.",
            "The producer allows null for `x`; the contract does not.",
            "The contract may send null for `x`; the consumer's type does not allow null.",
        ];
        for case in cases {
            let fa = finding(case).unwrap_or_else(|| panic!("no Persian form for: {case}"));
            assert!(fa.contains('`'), "{fa}");
            assert!(!fa.contains("The "), "{fa}");
        }
        assert_eq!(
            finding("The consumer reads `name`; the producer's response has no field `name`.")
                .as_deref(),
            Some("مصرف‌کننده `name` را می‌خواند؛ پاسخ تولیدکننده فیلدی به نام `name` ندارد.")
        );
    }

    #[test]
    fn unknown_sentences_stay_english() {
        assert_eq!(finding("Something else entirely."), None);
    }
}
