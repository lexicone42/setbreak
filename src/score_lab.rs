//! Score Lab: interactive formula experimentation without recompiling.
//!
//! Loads all numeric features from the database, binds them as variables
//! in an expression evaluator, and lets users test score formulas instantly.
//!
//! Example: `setbreak score-lab "rms_level / 0.18 * 30 + (lufs_integrated + 55) / 22 * 30"`

use crate::db::Database;
use crate::db::columns::ANALYSIS_SCHEMA;
use evalexpr::{
    ContextWithMutableVariables, DefaultNumericTypes, HashMapContext, Value, build_operator_tree,
};
use std::collections::HashMap;

/// A track with all its numeric features loaded as a name→value map.
pub(crate) struct FeatureRow {
    pub title: String,
    pub date: String,
    pub duration_min: f64,
    pub features: HashMap<String, f64>,
}

/// Result of evaluating a formula across the library.
pub struct LabResult {
    pub title: String,
    pub date: String,
    pub duration_min: f64,
    pub computed_score: f64,
}

/// List all available variable names for score-lab expressions.
pub fn list_variables() -> Vec<(&'static str, &'static str, &'static str)> {
    ANALYSIS_SCHEMA
        .iter()
        .filter(|c| c.sql_type == "REAL" || c.sql_type == "INT")
        .map(|c| (c.name, c.category, c.description))
        .collect()
}

/// Evaluate a formula against all analyzed tracks and return top results.
pub fn evaluate_formula(
    db: &Database,
    formula: &str,
    limit: usize,
    min_duration_secs: Option<f64>,
    live_only: bool,
) -> Result<Vec<LabResult>, String> {
    // Validate the expression parses before loading data
    let tree = build_operator_tree::<DefaultNumericTypes>(formula)
        .map_err(|e| format!("Parse error: {e}"))?;

    // Build the SQL query dynamically from ANALYSIS_SCHEMA
    let numeric_cols: Vec<&str> = ANALYSIS_SCHEMA
        .iter()
        .filter(|c| c.sql_type == "REAL" || c.sql_type == "INT")
        .map(|c| c.name)
        .collect();

    let col_selects: String = numeric_cols
        .iter()
        .map(|name| format!("COALESCE(a.{name}, 0)"))
        .collect::<Vec<_>>()
        .join(", ");

    let mut where_parts = vec![
        crate::db::columns::NOT_GARBAGE.to_string(),
        "a.energy_score IS NOT NULL".to_string(), // must be analyzed
    ];
    if live_only {
        where_parts.push(crate::db::columns::LIVE_ONLY.to_string());
    }
    if let Some(dur) = min_duration_secs {
        where_parts.push(format!("a.duration >= {dur}"));
    }
    let where_clause = where_parts.join(" AND ");

    let sql = format!(
        "SELECT a.track_id,
                COALESCE(t.parsed_title, t.title, '(untitled)'),
                COALESCE(t.parsed_date, t.date, '?'),
                COALESCE(t.file_path, ''),
                COALESCE(a.duration, 0) / 60.0,
                {col_selects}
         FROM analysis_results a
         JOIN tracks t ON t.id = a.track_id
         WHERE {where_clause}"
    );

    // Load all rows
    let rows = db
        .query_raw_lab(&sql, &numeric_cols)
        .map_err(|e| format!("Query error: {e}"))?;

    // Evaluate expression for each row
    let mut results: Vec<LabResult> = Vec::with_capacity(rows.len());

    for row in &rows {
        let mut context = HashMapContext::new();
        for (name, &val) in &row.features {
            context
                .set_value(name.clone(), Value::Float(val))
                .map_err(|e| format!("Variable bind error for {name}: {e}"))?;
        }

        // Also bind some convenience aliases
        if let Some(&dur) = row.features.get("duration") {
            context
                .set_value("duration_min".to_string(), Value::Float(dur / 60.0))
                .ok();
        }

        if let Ok(val) = tree.eval_with_context(&context) {
            // Try to extract a numeric value
            let score = val
                .as_float()
                .ok()
                .or_else(|| val.as_int().ok().map(|i: i64| i as f64));
            if let Some(s) = score {
                if s.is_finite() {
                    results.push(LabResult {
                        title: row.title.clone(),
                        date: row.date.clone(),
                        duration_min: row.duration_min,
                        computed_score: s,
                    });
                }
            }
        }
    }

    // Sort descending by computed score
    results.sort_by(|a, b| {
        b.computed_score
            .partial_cmp(&a.computed_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    Ok(results)
}

// ── Database query support ──────────────────────────────────────────────

impl Database {
    /// Load all numeric features for every analyzed track.
    /// Returns FeatureRow structs with a HashMap of column_name → f64.
    pub(crate) fn query_raw_lab(
        &self,
        sql: &str,
        col_names: &[&str],
    ) -> crate::db::Result<Vec<FeatureRow>> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows_out = Vec::new();

        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let _track_id: i64 = row.get(0)?;
            let title: String = row.get(1)?;
            let date: String = row.get(2)?;
            let _file_path: String = row.get(3)?;
            let duration_min: f64 = row.get(4)?;

            let mut features = HashMap::with_capacity(col_names.len());
            for (i, &name) in col_names.iter().enumerate() {
                let val: f64 = row.get(5 + i)?;
                features.insert(name.to_string(), val);
            }

            rows_out.push(FeatureRow {
                title,
                date,
                duration_min,
                features,
            });
        }

        Ok(rows_out)
    }
}
