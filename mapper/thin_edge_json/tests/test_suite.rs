use std::path::*;
use walkdir::{DirEntry, WalkDir};

#[test]
fn it_rejects_invalid_thin_edge_json() -> anyhow::Result<()> {
    for fixture in fixtures("tests/fixtures/invalid")?.iter() {
        let input = std::fs::read_to_string(fixture.path())?;
        println!("Fixture: {:?}", fixture.path());

        let res_stream_parser: anyhow::Result<_> = {
            let mut builder = thin_edge_json::builder::ThinEdgeJsonBuilder::new();
            thin_edge_json::parser::parse_str(&input, &mut builder)
                .map_err(Into::into)
                .and_then(|_| builder.done().map_err(Into::into))
        };

        assert!(res_stream_parser.is_err());
    }

    Ok(())
}

#[test]
fn it_transforms_valid_thin_edge_json() -> anyhow::Result<()> {
    let mut had_missing_test_fixtures = false;

    for fixture in fixtures("tests/fixtures/valid")?.iter() {
        let input = std::fs::read_to_string(fixture.path())?;

        let output_stream_parser = {
            let mut builder = thin_edge_json::serialize::ThinEdgeJsonSerializer::new();
            let res = thin_edge_json::parser::parse_str(&input, &mut builder);
            assert!(res.is_ok());
            builder.into_string()?
        };

        if let Ok(expected_output) =
            std::fs::read_to_string(fixture.path().with_extension("expected_output"))
        {
            assert_eq!(expected_output, output_stream_parser);
        } else {
            // we don't have a test fixture yet. Create one and abort.
            std::fs::write(
                fixture.path().with_extension("expected_output"),
                output_stream_parser,
            )?;
            had_missing_test_fixtures = true;
        }
    }

    assert!(!had_missing_test_fixtures, "Test fixtures were missing.");
    Ok(())
}

fn fixtures(subdir: &str) -> anyhow::Result<Vec<DirEntry>> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures: Result<Vec<DirEntry>, _> = WalkDir::new(Path::join(&base, subdir))
        .sort_by_file_name()
        .into_iter()
        .collect();

    Ok(fixtures?.into_iter().filter(is_fixture).collect())
}

fn is_fixture(e: &DirEntry) -> bool {
    match (e.file_type().is_file(), e.path().extension()) {
        (true, Some(ext)) if ext == "json" => true,
        _ => false,
    }
}
