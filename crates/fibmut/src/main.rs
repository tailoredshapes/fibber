//! `fibmut`: mutation review of fibber library source (spec/bootstrap.md
//! section 3, kept this time). See `cli::USAGE`.

mod casesel;
mod cli;
mod fsutil;
mod ops;
mod report;
mod runner;
mod sample;
mod session;
mod sexp;
#[cfg(test)]
mod testutil;

use std::path::Path;
use std::process::ExitCode;

use cli::{Args, Parsed};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match cli::parse(&args) {
        Ok(Parsed::Help) => {
            println!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        Ok(Parsed::Run(args)) => go(&args),
        Err(message) => {
            eprintln!("fibmut: {message}\n\n{}", cli::USAGE);
            ExitCode::from(2)
        }
    }
}

fn go(args: &Args) -> ExitCode {
    let base = match std::env::current_dir() {
        Ok(base) => base,
        Err(e) => return fail(&format!("no current directory: {e}")),
    };
    let result = if args.list {
        session::plan(&args.config, &base)
            .map(|plan| print!("{}", listing(&plan, &args.config.module)))
            .map(|()| None)
    } else {
        let mut say = |line: &str| eprintln!("{line}");
        session::run(&args.config, &base, &mut say).map(Some)
    };
    match result {
        Err(message) => fail(&message),
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(report)) => finish(args, &report),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("fibmut: {message}");
    ExitCode::from(2)
}

/// Prints the report, writes the survivors when asked, and picks the status.
fn finish(args: &Args, report: &report::Report) -> ExitCode {
    let text = report::render(report, args.verbose);
    print!("{text}");
    if let Some(dir) = &args.out {
        if let Err(e) = write_out(dir, report, &text) {
            return fail(&format!("cannot write {}: {e}", dir.display()));
        }
    }
    if args.fail_on_survivor && !report.survivors().is_empty() {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn write_out(dir: &Path, report: &report::Report, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("report.txt"), text)?;
    for d in report.survivors() {
        let name = format!("survivor-m{}-{}-line{}.diff", d.id, d.mutant.op, d.line);
        let body = format!(
            "{} line {}\n{}",
            report.module,
            d.line,
            report::diff(&report.src, &d.mutant)
        );
        std::fs::write(dir.join(name), body)?;
    }
    Ok(())
}

/// The chosen mutants, a line each, for `--list`.
fn listing(plan: &session::Plan, module: &Path) -> String {
    let mut out = format!(
        "{}: {} mutants, {} after the filters, {} chosen\n",
        module.display(),
        plan.all.len(),
        plan.filtered,
        plan.chosen.len()
    );
    for (id, m) in &plan.chosen {
        let was = plan.src[m.start..m.end].replace('\n', " ");
        let now = m.text.replace('\n', " ");
        out.push_str(&format!(
            "m{id} {} line {}: {was} -> {now}\n",
            m.op,
            m.line(&plan.src)
        ));
    }
    out
}
