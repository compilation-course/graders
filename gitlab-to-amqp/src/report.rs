use std::convert::TryInto;

use amqp_utils::AmqpResponse;
use gitlab::api::{self, State};
use hyper::Request;
use serde::Deserialize;

use crate::config::Configuration;
use crate::gitlab;

#[derive(Deserialize)]
pub struct Report {
    grade: usize,
    #[serde(rename = "max-grade")]
    max_grade: usize,
    explanation: Option<String>,
    groups: Option<Vec<Group>>,
}

#[derive(Deserialize)]
pub struct Group {
    grade: usize,
    #[serde(rename = "max-grade")]
    max_grade: usize,
    description: Option<String>,
    tests: Vec<Test>,
}

#[derive(Deserialize)]
pub struct Test {
    coefficient: usize,
    description: String,
    success: bool,
    signal: Option<u32>,
}

fn signal_to_explanation(signal: u32) -> String {
    match signal.try_into() {
        Ok(libc::SIGILL) => String::from("illegal instruction"),
        Ok(libc::SIGABRT) => String::from("abort, possibly because of a failed assertion"),
        Ok(libc::SIGFPE) => String::from("arithmetic exception"),
        Ok(libc::SIGKILL) => String::from(
            "program killed, possibly because of an infinite loop or memory exhaustion",
        ),
        Ok(libc::SIGBUS) => String::from("bus error"),
        Ok(libc::SIGSEGV) => String::from("segmentation fault"),
        _ => format!("crash (signal {signal})"),
    }
}

fn yaml_to_markdown(lab: &str, yaml: &str) -> eyre::Result<(String, usize, usize)> {
    let report: Report = serde_yaml::from_str(yaml)?;
    if let Some(explanation) = report.explanation {
        log::warn!("problem during handling of {lab}: {explanation}");
        return Ok((
            format!(
                r#"## Error

There has been an error during the test for {lab}:

```
{explanation}
```"#
            ),
            report.grade,
            report.max_grade,
        ));
    }
    let groups = report
        .groups
        .unwrap_or_default()
        .iter()
        .filter(|group| group.grade != group.max_grade)
        .map(|group| {
            let tests = if group.grade == 0 {
                String::new()
            } else {
                let mut explanation = "Failing tests:\n\n".to_owned();
                explanation.push_str(
                    &group
                        .tests
                        .iter()
                        .filter(|test| !test.success)
                        .map(|test| {
                            format!(
                                "- {}{}{}",
                                &test.description,
                                if test.coefficient == 1 {
                                    String::new()
                                } else {
                                    format!(" (coefficient {})", test.coefficient)
                                },
                                test.signal.map_or_else(String::new, |s| format!(
                                    " [{}]",
                                    signal_to_explanation(s)
                                ))
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                explanation
            };
            format!(
                "### {} ({})\n\n{}\n",
                group
                    .description
                    .clone()
                    .unwrap_or_else(|| "*Test group*".to_owned()),
                pass_fail(group.grade, group.max_grade),
                tests
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let diagnostic = format!(
        "## Failed tests report for {} ({})\n\n{}",
        lab,
        pass_fail(report.grade, report.max_grade),
        groups
    );
    Ok((diagnostic, report.grade, report.max_grade))
}

pub fn response_to_post(
    config: &Configuration,
    response: &AmqpResponse,
) -> eyre::Result<Vec<Request<String>>> {
    let (report, grade, max_grade) = yaml_to_markdown(&response.lab, &response.yaml_result)?;
    let (hook, zip) = gitlab::from_opaque(&response.opaque)?;
    match gitlab::remove_zip_file(config, &zip) {
        Ok(_) => log::trace!("removed zip file {zip}"),
        Err(e) => log::warn!("could not remove zip file {zip}: {e}"),
    }
    let state = if grade == max_grade {
        State::Success
    } else {
        State::Failed
    };
    let status = api::post_status(
        &config.gitlab,
        &hook,
        &state,
        hook.branch_name(),
        &response.lab,
        Some(&format!("grade: {grade}/{max_grade}")),
    );
    Ok(if state == State::Success {
        log::info!(
            "tests for {} are a success, generating status only",
            &response.job_name
        );
        vec![status]
    } else {
        log::info!(
            "tests for {} are a failure ({}/{}), generating status and comment",
            &response.job_name,
            grade,
            max_grade
        );
        let comment = api::post_comment(&config.gitlab, &hook, &report);
        vec![status, comment]
    })
}

fn pass_fail(grade: usize, max_grade: usize) -> String {
    if grade > max_grade {
        format!("{grade} passing out of {max_grade} [!]")
    } else if grade == max_grade {
        format!("all {max_grade} passing")
    } else if grade == 0 {
        format!("all {max_grade} failing")
    } else {
        format!("{} failing out of {}", max_grade - grade, max_grade)
    }
}
