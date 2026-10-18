//! Levenshtein distance algorithm for compiler Did-You-Mean suggestions.
//!
//! Provides closest match lookups for misspelled built-in methods and properties.

#[must_use]
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let a_len = a_bytes.len();
    let b_len = b_bytes.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, &a_byte) in a_bytes.iter().enumerate() {
        curr_row[0] = i + 1;
        for (j, &b_byte) in b_bytes.iter().enumerate() {
            let cost = usize::from(a_byte != b_byte);
            curr_row[j + 1] = (prev_row[j + 1] + 1)
                .min(curr_row[j] + 1)
                .min(prev_row[j] + cost);
        }
        prev_row.copy_from_slice(&curr_row);
    }

    curr_row[b_len]
}

#[must_use]
pub fn find_closest<'a>(target: &str, candidates: &[&'a str], max_distance: usize) -> Option<&'a str> {
    let mut best_candidate = None;
    let mut best_dist = max_distance + 1;

    for &candidate in candidates {
        let dist = levenshtein_distance(target, candidate);
        if dist < best_dist {
            best_dist = dist;
            best_candidate = Some(candidate);
        }
    }

    if best_dist <= max_distance {
        best_candidate
    } else {
        None
    }
}
