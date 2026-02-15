use crate::db::Database;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

/// Number of nearest neighbors to store per track.
const TOP_K: usize = 20;

pub struct SimilarityResult {
    pub tracks_processed: usize,
    pub pairs_stored: usize,
}

/// Compute pairwise cosine similarity between all analyzed tracks and store top-K neighbors.
pub fn compute_similarity(
    db: &Database,
    jobs: usize,
) -> Result<SimilarityResult, crate::db::DbError> {
    // Load all feature vectors
    let raw = db.get_feature_vectors()?;
    let n = raw.len();

    if n < 2 {
        return Ok(SimilarityResult {
            tracks_processed: n,
            pairs_stored: 0,
        });
    }

    let track_ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
    let dim = raw[0].1.len();

    // Z-score normalize each dimension across all tracks
    let vectors = normalize_features(&raw, dim);

    println!("Computing similarity for {} tracks ({}-dim vectors)...", n, dim);

    let pb = ProgressBar::new(n as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} tracks ({eta} remaining)")
            .unwrap()
            .progress_chars("=>-"),
    );

    // Build rayon pool
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()
        .unwrap();

    // For each track, find top-K most similar tracks by cosine similarity.
    // Cosine similarity → distance = 1.0 - similarity (0 = identical, 2 = opposite).
    let all_neighbors: Vec<Vec<(usize, f64)>> = pool.install(|| {
        (0..n)
            .into_par_iter()
            .map(|i| {
                let mut distances: Vec<(usize, f64)> = (0..n)
                    .filter(|&j| j != i)
                    .map(|j| {
                        let sim = cosine_similarity(&vectors[i], &vectors[j]);
                        let dist = 1.0 - sim;
                        (j, dist)
                    })
                    .collect();

                // Partial sort: only need top-K smallest distances
                let k = TOP_K.min(distances.len()) - 1;
                distances.select_nth_unstable_by(k, |a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });
                distances.truncate(TOP_K);
                distances.sort_by(|a, b| {
                    a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                });

                pb.inc(1);
                distances
            })
            .collect()
    });

    pb.finish_with_message("done");

    // Flatten into (track_id, similar_track_id, distance, rank) tuples
    let mut pairs: Vec<(i64, i64, f64, i32)> = Vec::with_capacity(n * TOP_K);
    for (i, neighbors) in all_neighbors.iter().enumerate() {
        for (rank, &(j, dist)) in neighbors.iter().enumerate() {
            pairs.push((track_ids[i], track_ids[j], dist, rank as i32 + 1));
        }
    }

    let pairs_count = pairs.len();
    println!("Storing {} similarity pairs...", pairs_count);
    db.store_similarities(&pairs)?;

    Ok(SimilarityResult {
        tracks_processed: n,
        pairs_stored: pairs_count,
    })
}

/// Z-score normalize each dimension: subtract mean, divide by std.
/// Returns a Vec of normalized vectors (same shape as input).
fn normalize_features(raw: &[(i64, Vec<f64>)], dim: usize) -> Vec<Vec<f64>> {
    let n = raw.len();

    // Compute mean and std for each dimension
    let mut means = vec![0.0_f64; dim];
    let mut vars = vec![0.0_f64; dim];

    for (_, vec) in raw {
        for (d, &val) in vec.iter().enumerate() {
            means[d] += val;
        }
    }
    for m in &mut means {
        *m /= n as f64;
    }

    for (_, vec) in raw {
        for (d, &val) in vec.iter().enumerate() {
            let diff = val - means[d];
            vars[d] += diff * diff;
        }
    }
    let stds: Vec<f64> = vars
        .iter()
        .map(|v| (v / n as f64).sqrt().max(1e-10))
        .collect();

    // Normalize
    raw.iter()
        .map(|(_, vec)| {
            vec.iter()
                .enumerate()
                .map(|(d, &val)| (val - means[d]) / stds[d])
                .collect()
        })
        .collect()
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-10 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_normalize_features() {
        let raw = vec![
            (1, vec![10.0, 100.0]),
            (2, vec![20.0, 200.0]),
            (3, vec![30.0, 300.0]),
        ];
        let normed = normalize_features(&raw, 2);

        // After z-score, mean should be ~0 and std ~1
        let mean_0: f64 = normed.iter().map(|v| v[0]).sum::<f64>() / 3.0;
        let mean_1: f64 = normed.iter().map(|v| v[1]).sum::<f64>() / 3.0;
        assert!(mean_0.abs() < 1e-10);
        assert!(mean_1.abs() < 1e-10);

        // Both dimensions should have same normalized values despite different scales
        assert!((normed[0][0] - normed[0][1]).abs() < 1e-10);
    }
}
