use super::*;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn cases_takes_one_directory() {
    assert_eq!(
        parse(&args(&["cases", "cases/other"])),
        Command::Cases {
            dir: "cases/other".to_string(),
            only: Vec::new()
        }
    );
}

#[test]
fn cases_only_takes_one_or_more_prefixes_after_the_directory() {
    assert_eq!(
        parse(&args(&["cases", "d", "--only", "240-", "241-"])),
        Command::Cases {
            dir: "d".to_string(),
            only: vec!["240-".to_string(), "241-".to_string()]
        }
    );
    assert_eq!(parse(&args(&["cases", "d", "--only"])), Command::Invalid);
    assert_eq!(parse(&args(&["cases", "--only", "1"])), Command::Invalid);
    assert_eq!(
        parse(&args(&["cases", "d", "-only", "1"])),
        Command::Invalid
    );
}

#[test]
fn cases_without_a_directory_uses_the_default() {
    assert_eq!(
        parse(&args(&["cases"])),
        Command::Cases {
            dir: "cases/ownership".to_string(),
            only: Vec::new()
        }
    );
}

#[test]
fn cases_with_extra_arguments_is_invalid() {
    assert_eq!(parse(&args(&["cases", "a", "b"])), Command::Invalid);
}

#[test]
fn explain_takes_one_file() {
    assert_eq!(
        parse(&args(&["explain", "a.fib"])),
        Command::Explain {
            file: "a.fib".to_string()
        }
    );
    assert_eq!(parse(&args(&["explain"])), Command::Invalid);
}

#[test]
fn run_takes_one_file() {
    assert_eq!(
        parse(&args(&["run", "a.fib"])),
        Command::Run {
            file: "a.fib".to_string(),
            args: Vec::new()
        }
    );
    assert_eq!(
        parse(&args(&["run", "a.fib", "--", "x"])),
        Command::Run {
            file: "a.fib".to_string(),
            args: vec!["x".to_string()]
        }
    );
    assert_eq!(parse(&args(&["run"])), Command::Invalid);
}

#[test]
fn read_takes_one_or_more_files() {
    assert_eq!(
        parse(&args(&["read", "a.fib", "b.fib"])),
        Command::Read {
            files: vec!["a.fib".to_string(), "b.fib".to_string()],
            print: false
        }
    );
    assert_eq!(parse(&args(&["read"])), Command::Invalid);
}

#[test]
fn read_print_is_a_flag_before_the_files() {
    assert_eq!(
        parse(&args(&["read", "--print", "a.fib", "b.fib"])),
        Command::Read {
            files: vec!["a.fib".to_string(), "b.fib".to_string()],
            print: true
        }
    );
    // The flag alone names no file; after a file it is a file name.
    assert_eq!(parse(&args(&["read", "--print"])), Command::Invalid);
    assert_eq!(
        parse(&args(&["read", "a.fib", "--print"])),
        Command::Read {
            files: vec!["a.fib".to_string(), "--print".to_string()],
            print: false
        }
    );
}

#[test]
fn expand_takes_its_options_and_files_from_the_shared_parser() {
    let opts = Options {
        context: true,
        ..Options::default()
    };
    assert_eq!(
        parse(&args(&["expand", "--context", "a.fib", "b.fib"])),
        Command::Expand {
            files: vec!["a.fib".to_string(), "b.fib".to_string()],
            opts
        }
    );
    assert_eq!(parse(&args(&["expand"])), Command::Invalid);
    assert_eq!(parse(&args(&["expand", "--context"])), Command::Invalid);
}

#[test]
fn types_takes_its_options_and_files_from_the_shared_parser() {
    let opts = fibref::types_dump::Options {
        stage: fibref::types_dump::Stage::Lower,
        tables: true,
        ..Default::default()
    };
    assert_eq!(
        parse(&args(&["types", "--stage", "lower", "--tables", "a.fib"])),
        Command::Types {
            files: vec!["a.fib".to_string()],
            opts
        }
    );
    assert_eq!(parse(&args(&["types"])), Command::Invalid);
    assert_eq!(
        parse(&args(&["types", "--stage", "x", "a.fib"])),
        Command::Invalid
    );
}

#[test]
fn own_takes_its_options_and_files_from_the_shared_parser() {
    let opts = fibref::own_dump::Options {
        library: true,
        sections: Some([fibref::own_dump::Section::Taken].into()),
        ..Default::default()
    };
    assert_eq!(
        parse(&args(&["own", "--library", "--sections", "taken", "a.fib"])),
        Command::Own {
            files: vec!["a.fib".to_string()],
            opts
        }
    );
    assert_eq!(parse(&args(&["own"])), Command::Invalid);
    assert_eq!(
        parse(&args(&["own", "--sections", "type", "a.fib"])),
        Command::Invalid
    );
}

#[test]
fn no_arguments_is_invalid() {
    assert_eq!(parse(&args(&[])), Command::Invalid);
}

#[test]
fn unknown_command_is_invalid() {
    assert_eq!(
        parse(&args(&["bogus", "cases/ownership"])),
        Command::Invalid
    );
}

#[test]
fn help_spellings_are_help() {
    for spelling in ["help", "--help", "-h"] {
        assert_eq!(parse(&args(&[spelling])), Command::Help, "{spelling}");
    }
}
