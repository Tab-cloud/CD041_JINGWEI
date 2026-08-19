use std::{env, fs, path::PathBuf};

use jingwei_kicad_schematic::parse_native_schematic;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args_os().skip(1);
    let input = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("usage: inspect_native <input.kicad_sch> [summary.json]")?;
    let output = arguments.next().map(PathBuf::from);
    if arguments.next().is_some() {
        return Err("usage: inspect_native <input.kicad_sch> [summary.json]".into());
    }

    let summary = parse_native_schematic(&fs::read(&input)?)?;
    let json = serde_json::to_vec_pretty(&summary)?;
    if let Some(output) = output {
        fs::write(output, &json)?;
    }
    println!("{}", String::from_utf8(json)?);
    Ok(())
}
