pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut curr = e.source();
    while let Some(cause) = curr {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        curr = cause.source();
    }
    Ok(())
}
