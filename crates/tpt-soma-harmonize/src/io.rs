use crate::{MappingTable, OntologySource, Result, ReviewQueue};
use std::io::{Read, Write};
use std::path::Path;

/// Load a review queue from a JSON file, or return an empty queue if absent.
pub fn load_queue(path: &Path) -> Result<ReviewQueue> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(ReviewQueue {
            pending: Vec::new(),
        })
    }
}

/// Persist a review queue as pretty JSON.
pub fn save_queue(path: &Path, queue: &ReviewQueue) -> Result<()> {
    let content = serde_json::to_string_pretty(queue)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Load a mapping table from a JSON file, or return an empty table if absent.
pub fn load_mapping_table(path: &Path) -> Result<MappingTable> {
    if path.exists() {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(MappingTable::new())
    }
}

/// Persist a mapping table as pretty JSON.
pub fn save_mapping_table(path: &Path, table: &MappingTable) -> Result<()> {
    let content = serde_json::to_string_pretty(table)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Write the pending review queue to a CSV with columns
/// `identifier,source,resolved_mapping` (resolved_mapping left blank).
pub fn export_queue_to_csv<W: Write>(queue: &ReviewQueue, writer: W) -> Result<()> {
    let mut writer = csv::Writer::from_writer(writer);
    writer.write_record(["identifier", "source", "resolved_mapping"])?;
    for unmapped in &queue.pending {
        writer.write_record([&unmapped.identifier, &unmapped.source, ""])?;
    }
    writer.flush()?;
    Ok(())
}

/// Import resolved mappings from a CSV reader, applying each non-empty
/// `resolved_mapping` to the mapping table and dropping the identifier from the
/// review queue. Returns the number of mappings applied.
///
/// The CSV should have columns: identifier,source,resolved_mapping
/// The source column is used to determine the OntologySource.
pub fn import_csv_mappings<R: Read>(
    reader: R,
    table: &mut MappingTable,
    queue: &mut ReviewQueue,
) -> Result<usize> {
    let mut reader = csv::Reader::from_reader(reader);
    let mut imported = 0;
    for record in reader.records() {
        let record = record?;
        let identifier = record.get(0).unwrap_or("").to_string();
        let source_str = record.get(1).unwrap_or("").to_string();
        let resolved = record.get(2).unwrap_or("").to_string();
        if !resolved.is_empty() {
            // Parse the source string to OntologySource
            let source = source_str
                .parse::<OntologySource>()
                .unwrap_or(OntologySource::Other);
            table.insert_simple(source, identifier.clone(), resolved.clone());
            queue.pending.retain(|u| u.identifier != identifier);
            imported += 1;
        }
    }
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Unmapped;
    use std::io::Cursor;

    fn sample_queue() -> ReviewQueue {
        ReviewQueue {
            pending: vec![
                Unmapped {
                    identifier: "1:999:G:A".to_string(),
                    source: "dbSNP".to_string(),
                },
                Unmapped {
                    identifier: "1:1000:C:T".to_string(),
                    source: "ClinVar".to_string(),
                },
            ],
        }
    }

    #[test]
    fn load_missing_queue_returns_empty() {
        let queue = load_queue(Path::new("definitely-not-present-queue.json")).unwrap();
        assert!(queue.pending.is_empty());
    }

    #[test]
    fn queue_round_trip_via_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.json");
        save_queue(&path, &sample_queue()).unwrap();
        let loaded = load_queue(&path).unwrap();
        assert_eq!(loaded.pending.len(), 2);
        assert_eq!(loaded.pending[0].identifier, "1:999:G:A");
        assert_eq!(loaded.pending[1].source, "ClinVar");
    }

    #[test]
    fn mapping_table_round_trip_via_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mapping.json");
        let mut table = MappingTable::new();
        table.insert_simple(OntologySource::DbSNP, "1:999:G:A", "rs999");
        save_mapping_table(&path, &table).unwrap();
        let loaded = load_mapping_table(&path).unwrap();
        assert_eq!(
            loaded.resolve(OntologySource::DbSNP, "1:999:G:A"),
            Some("rs999")
        );
    }

    #[test]
    fn export_import_csv_round_trip() {
        let mut buffer = Vec::new();
        export_queue_to_csv(&sample_queue(), &mut buffer).unwrap();
        let csv = String::from_utf8(buffer).unwrap();
        assert!(csv.contains("1:999:G:A,dbSNP,"));
        assert!(csv.starts_with("identifier,source,resolved_mapping"));

        let mut table = MappingTable::new();
        let mut queue = sample_queue();
        let imported = import_csv_mappings(Cursor::new(csv), &mut table, &mut queue).unwrap();
        // Neither row had a resolved mapping, so nothing is imported.
        assert_eq!(imported, 0);
        assert_eq!(queue.pending.len(), 2);

        // Now with resolved values.
        let csv_with_mappings =
            "identifier,source,resolved_mapping\n1:999:G:A,dbSNP,rs999\n1:1000:C:T,ClinVar,\n";
        let mut table = MappingTable::new();
        let mut queue = sample_queue();
        let imported =
            import_csv_mappings(Cursor::new(csv_with_mappings), &mut table, &mut queue).unwrap();
        assert_eq!(imported, 1);
        assert_eq!(
            table.resolve(OntologySource::DbSNP, "1:999:G:A"),
            Some("rs999")
        );
        assert_eq!(queue.pending.len(), 1);
        assert_eq!(queue.pending[0].identifier, "1:1000:C:T");
    }
}
