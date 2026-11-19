//! Shape-based detection, not a guarantee of confidentiality. Findings never retain values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    PrivateKey,
    ProviderKey,
    BearerToken,
    Jwt,
    Assignment,
    UrlCredentials,
    EnvLine,
    HighEntropy,
}
impl SecretKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::PrivateKey => "private key",
            Self::ProviderKey => "provider key",
            Self::BearerToken => "bearer token",
            Self::Jwt => "JWT",
            Self::Assignment => "assignment",
            Self::UrlCredentials => "URL credentials",
            Self::EnvLine => "environment value",
            Self::HighEntropy => "token-like string",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: SecretKind,
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

fn token(c: char) -> bool {
    c.is_ascii_alphanumeric() || "+/_-=".contains(c)
}
fn value_end(line: &str, start: usize) -> usize {
    line[start..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || ",;\"'<>".contains(*c))
        .map_or(line.len(), |(i, _)| start + i)
}

pub fn find(text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut pem = false;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if line.contains("-----BEGIN ") && line.contains("PRIVATE KEY-----") {
            pem = true;
        }
        if pem {
            if !line.is_empty() {
                out.push(Finding {
                    kind: SecretKind::PrivateKey,
                    line: number,
                    start: 0,
                    end: line.len(),
                });
            }
            if line.contains("-----END ") && line.contains("PRIVATE KEY-----") {
                pem = false;
            }
            continue;
        }
        let mut add = |kind, start, end| {
            if end > start {
                out.push(Finding {
                    kind,
                    line: number,
                    start,
                    end,
                });
            }
        };
        for prefix in [
            "sk-ant-",
            "sk-",
            "ghp_",
            "github_pat_",
            "AKIA",
            "xoxb-",
            "xoxp-",
            "AIza",
        ] {
            for (start, _) in line.match_indices(prefix) {
                let end = line[start..]
                    .char_indices()
                    .find(|(_, c)| !token(*c))
                    .map_or(line.len(), |(n, _)| start + n);
                let minimum = if prefix == "AKIA" {
                    20
                } else {
                    prefix.len() + 12
                };
                if end - start >= minimum {
                    add(SecretKind::ProviderKey, start, end);
                }
            }
        }
        let lower = line.to_ascii_lowercase();
        for (start, _) in lower.match_indices("bearer ") {
            let at = start + 7;
            let end = value_end(line, at);
            if end > at {
                add(SecretKind::BearerToken, at, end);
            }
        }
        for key in ["password", "passwd", "secret", "token", "api_key", "apikey"] {
            for (start, _) in lower.match_indices(key) {
                if start > 0
                    && (lower.as_bytes()[start - 1].is_ascii_alphanumeric()
                        || lower.as_bytes()[start - 1] == b'_')
                {
                    continue;
                }
                let mut at = start + key.len();
                while at < line.len() && b" \t\"'".contains(&line.as_bytes()[at]) {
                    at += 1;
                }
                if at == line.len() || !b"=:".contains(&line.as_bytes()[at]) {
                    continue;
                }
                at += 1;
                while at < line.len() && b" \t\"'".contains(&line.as_bytes()[at]) {
                    at += 1;
                }
                let end = value_end(line, at);
                if line[at..end].chars().count() >= 6 {
                    add(SecretKind::Assignment, at, end);
                }
            }
        }
        for (start, _) in line.match_indices("://") {
            let at = start + 3;
            let end = line[at..]
                .char_indices()
                .find(|(_, c)| c.is_whitespace() || "/?#".contains(*c))
                .map_or(line.len(), |(n, _)| at + n);
            if let Some(mark) = line[at..end].rfind('@')
                && line[at..at + mark].contains(':')
            {
                add(SecretKind::UrlCredentials, at, at + mark);
            }
        }
        if let Some((key, val)) = line.split_once('=')
            && key.len() >= 3
            && key.as_bytes()[0].is_ascii_uppercase()
            && key
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            && val.len() >= 8
            && !val.chars().any(char::is_whitespace)
        {
            add(SecretKind::EnvLine, key.len() + 1, line.len());
        }
        // JWTs are three nonempty base64url segments; punctuation outside the token is ignored.
        let mut at = 0;
        for chunk in
            line.split_inclusive(|c: char| !(c.is_ascii_alphanumeric() || "-_.".contains(c)))
        {
            let candidate =
                chunk.trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || "-_".contains(c)));
            let parts: Vec<_> = candidate.split('.').collect();
            if parts.len() == 3
                && parts.iter().all(|p| {
                    p.len() >= 6
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                })
            {
                add(SecretKind::Jwt, at, at + candidate.len());
            }
            at += chunk.len();
        }
        let mut at = 0;
        for chunk in
            line.split_inclusive(|c: char| !(c.is_ascii_alphanumeric() || "+/_-".contains(c)))
        {
            let candidate = chunk
                .trim_end_matches(|c: char| !(c.is_ascii_alphanumeric() || "+/_-".contains(c)));
            let classes = [
                candidate.bytes().any(|b| b.is_ascii_uppercase()),
                candidate.bytes().any(|b| b.is_ascii_lowercase()),
                candidate.bytes().any(|b| b.is_ascii_digit()),
                candidate.bytes().any(|b| b"+/_-".contains(&b)),
            ];
            let uuid = candidate.len() == 36
                && candidate.char_indices().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == '-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                });
            let path = candidate.contains('/')
                && (candidate.starts_with('/')
                    || candidate.starts_with("src/")
                    || candidate.starts_with("docs/")
                    || chunk.contains('.')
                    || (at > 0 && line.as_bytes()[at - 1] == b':'));
            if candidate.len() >= 32
                && classes.iter().filter(|b| **b).count() >= 3
                && !uuid
                && !path
            {
                add(SecretKind::HighEntropy, at, at + candidate.len());
            }
            at += chunk.len();
        }
    }
    out.sort_by_key(|f| (f.line, f.start, std::cmp::Reverse(f.end)));
    let mut merged: Vec<Finding> = Vec::new();
    for f in out {
        if let Some(last) = merged.last_mut()
            && last.line == f.line
            && f.start < last.end
        {
            last.end = last.end.max(f.end);
            continue;
        }
        merged.push(f);
    }
    merged
}

pub fn mask(text: &str) -> String {
    let findings = find(text);
    let mut result = String::with_capacity(text.len());
    let mut findings = findings.iter().peekable();
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let mut offset = 0;
        while let Some(f) = findings.peek().filter(|f| f.line == i + 1) {
            result.push_str(&line[offset..f.start]);
            result.push_str(&format!("[masked: {}]", f.kind.label()));
            offset = f.end;
            findings.next();
        }
        result.push_str(&line[offset..]);
    }
    result
}
