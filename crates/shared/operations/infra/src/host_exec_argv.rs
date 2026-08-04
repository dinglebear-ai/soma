#[cfg(any(feature = "process-driver", test))]
use crate::{HostExecCommand, InfraError, InfraResult};

#[cfg(any(feature = "process-driver", test))]
pub(crate) fn filesystem_operand_indices(
    command: HostExecCommand,
    args: &[String],
) -> InfraResult<Vec<usize>> {
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let (flags, value_flags): (&[&str], &[&str]) = match command {
        HostExecCommand::Cat => (
            &[
                "-A",
                "-b",
                "-E",
                "-n",
                "-s",
                "-T",
                "-v",
                "--number",
                "--number-nonblank",
                "--show-all",
                "--show-ends",
                "--show-tabs",
                "--show-nonprinting",
                "--squeeze-blank",
            ],
            &[],
        ),
        HostExecCommand::Head => (
            &["-q", "-v", "--quiet", "--silent", "--verbose"],
            &["-c", "--bytes", "-n", "--lines"],
        ),
        HostExecCommand::Tail => (
            &[
                "-f",
                "-F",
                "-q",
                "-v",
                "--follow",
                "--quiet",
                "--silent",
                "--verbose",
            ],
            &["-c", "--bytes", "-n", "--lines", "-s", "--sleep-interval"],
        ),
        HostExecCommand::Ls => (
            &[
                "-1",
                "-A",
                "-a",
                "-d",
                "-h",
                "-l",
                "-R",
                "--all",
                "--almost-all",
                "--directory",
                "--human-readable",
                "--recursive",
            ],
            &[],
        ),
        HostExecCommand::Tree => (&["-a", "-d", "-f", "-i", "--noreport"], &["-L"]),
        HostExecCommand::Stat => (
            &[
                "-f",
                "-L",
                "-t",
                "--dereference",
                "--file-system",
                "--terse",
            ],
            &["-c", "--format", "--printf"],
        ),
        HostExecCommand::File => (
            &["-b", "-L", "-z", "--brief", "--dereference", "--uncompress"],
            &[],
        ),
        HostExecCommand::Du => (
            &[
                "-a",
                "-h",
                "-s",
                "-x",
                "--all",
                "--human-readable",
                "--summarize",
                "--one-file-system",
            ],
            &["-d", "--max-depth"],
        ),
        HostExecCommand::Diff => (
            &[
                "-a",
                "-b",
                "-B",
                "-i",
                "-q",
                "-s",
                "-u",
                "-w",
                "--brief",
                "--ignore-all-space",
                "--ignore-blank-lines",
                "--ignore-case",
                "--report-identical-files",
                "--text",
                "--unified",
            ],
            &[],
        ),
        HostExecCommand::Wc => (
            &[
                "-c", "-l", "-m", "-w", "--bytes", "--chars", "--lines", "--words",
            ],
            &[],
        ),
        HostExecCommand::Uniq => (
            &[
                "-c",
                "-d",
                "-i",
                "-u",
                "--count",
                "--ignore-case",
                "--repeated",
                "--unique",
            ],
            &[
                "-f",
                "--skip-fields",
                "-s",
                "--skip-chars",
                "-w",
                "--check-chars",
            ],
        ),
        HostExecCommand::Grep | HostExecCommand::Rg => {
            return grep_like_operand_indices(command, &args);
        }
        HostExecCommand::Df
        | HostExecCommand::Pwd
        | HostExecCommand::Hostname
        | HostExecCommand::Uptime
        | HostExecCommand::Whoami => {
            if args.is_empty() {
                return Ok(Vec::new());
            }
            return Err(invalid(format!(
                "{} does not accept host-exec arguments",
                command.as_str()
            )));
        }
    };
    parse_path_operands(command, &args, flags, value_flags)
}

#[cfg(any(feature = "process-driver", test))]
fn parse_path_operands(
    command: HostExecCommand,
    args: &[&str],
    flags: &[&str],
    value_flags: &[&str],
) -> InfraResult<Vec<usize>> {
    let mut paths = Vec::new();
    let mut index = 0;
    let mut options = true;
    while index < args.len() {
        let argument = args[index];
        if options && argument == "--" {
            options = false;
        } else if options && argument.starts_with('-') {
            if argument.contains('=')
                || (!flags.contains(&argument) && !value_flags.contains(&argument))
            {
                return Err(invalid(format!(
                    "unsupported {} option: {argument}",
                    command.as_str()
                )));
            }
            if value_flags.contains(&argument) {
                index += 1;
                if index >= args.len() || args[index].starts_with('-') {
                    return Err(invalid(format!(
                        "{} option {argument} requires a value",
                        command.as_str()
                    )));
                }
            }
        } else {
            paths.push(index);
        }
        index += 1;
    }
    Ok(paths)
}

#[cfg(any(feature = "process-driver", test))]
fn grep_like_operand_indices(command: HostExecCommand, args: &[&str]) -> InfraResult<Vec<usize>> {
    let flags = [
        "-F",
        "-H",
        "-I",
        "-i",
        "-l",
        "-n",
        "-v",
        "-w",
        "-x",
        "--fixed-strings",
        "--files-with-matches",
        "--ignore-case",
        "--line-number",
        "--invert-match",
        "--word-regexp",
        "--line-regexp",
    ];
    let value_flags = [
        "-A",
        "-B",
        "-C",
        "-e",
        "-g",
        "-m",
        "--after-context",
        "--before-context",
        "--context",
        "--glob",
        "--max-count",
        "--regexp",
    ];
    let mut paths = Vec::new();
    let mut index = 0;
    let mut options = true;
    let mut has_explicit_pattern = false;
    let mut positional_pattern_seen = false;
    while index < args.len() {
        let argument = args[index];
        if options && argument == "--" {
            options = false;
        } else if options && argument.starts_with('-') {
            if argument.contains('=')
                || (!flags.contains(&argument) && !value_flags.contains(&argument))
            {
                return Err(invalid(format!(
                    "unsupported {} option: {argument}",
                    command.as_str()
                )));
            }
            if value_flags.contains(&argument) {
                index += 1;
                if index >= args.len() {
                    return Err(invalid(format!(
                        "{} option {argument} requires a value",
                        command.as_str()
                    )));
                }
                has_explicit_pattern |= matches!(argument, "-e" | "--regexp");
            }
        } else if !has_explicit_pattern && !positional_pattern_seen {
            positional_pattern_seen = true;
        } else {
            paths.push(index);
        }
        index += 1;
    }
    if !has_explicit_pattern && !positional_pattern_seen {
        return Err(invalid(format!("{} requires a pattern", command.as_str())));
    }
    Ok(paths)
}

#[cfg(any(feature = "process-driver", test))]
fn invalid(message: impl Into<String>) -> InfraError {
    InfraError::InvalidRequest {
        domain: "host-exec",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "host_exec_argv_tests.rs"]
mod tests;
