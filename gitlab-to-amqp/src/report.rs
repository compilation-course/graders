use config::Configuration;
use errors::Result;
use gitlab;
use gitlab::api::{self, State};
use graders_utils::amqputils::AMQPResponse;
use hyper::Request;
use serde_yaml;

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

fn signal_to_explanation(signal: u32) -> &'static str {
    match signal {
        4 => "illegal instruction",
        6 => "abort, possibly because of a failed assertion",
        8 => "arithmetic exception",
        9 => "program killed",
        10 => "bus error",
        11 => "segmentation fault",
        _ => "crash",
    }
}

fn yaml_to_markdown(lab: &str, yaml: &str) -> Result<(String, usize, usize)> {
    let report: Report = serde_yaml::from_str(yaml)?;
    if let Some(explanation) = report.explanation {
        warn!("problem during handling of {}: {}", lab, explanation);
        return Ok((
            format!(
                r#"## Error

There has been an error during the test for {}:

```
{}
```"#,
                lab, explanation
            ),
            report.grade,
            report.max_grade,
        ));
    }
    let groups = report
        .groups
        .unwrap_or_else(|| vec![])
        .iter()
        .filter(|group| group.grade != group.max_grade)
        .map(|group| {
            let tests = if group.grade != 0 {
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
                                if test.coefficient != 1 {
                                    format!(" (coefficient {})", test.coefficient)
                                } else {
                                    "".to_owned()
                                },
                                test.signal
                                    .map(|s| format!(" [{}]", signal_to_explanation(s)))
                                    .unwrap_or_else(|| "".to_owned()),
                            )
                        }).collect::<Vec<_>>()
                        .join("\n"),
                );
                explanation
            } else {
                String::new()
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
        }).collect::<Vec<_>>()
        .join("\n");
    let diagnostic = format!(
        "## Failed tests report for {} ({})\n\n{}",
        lab,
        pass_fail(report.grade, report.max_grade),
        groups
    );
    Ok((diagnostic, report.grade, report.max_grade))
}

pub fn response_to_post(config: &Configuration, response: &AMQPResponse) -> Result<Vec<Request>> {
    let (report, grade, max_grade) = yaml_to_markdown(&response.lab, &response.yaml_result)?;
    let (hook, zip) = gitlab::from_opaque(&response.opaque)?;
    match gitlab::remove_zip_file(config, &zip) {
        Ok(_) => trace!("removed zip file {}", zip),
        Err(e) => warn!("could not remove zip file {}: {}", zip, e),
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
        Some(&format!("grade: {}/{}", grade, max_grade)),
    );
    Ok(if state == State::Success {
        info!(
            "tests for {} are a success, generating status only",
            &response.job_name
        );
        vec![status]
    } else {
        info!(
            "tests for {} are a failure ({}/{}), generating status and comment",
            &response.job_name, grade, max_grade
        );
        let comment = api::post_comment(&config.gitlab, &hook, &report);
        vec![status, comment]
    })
}

fn pass_fail(grade: usize, max_grade: usize) -> String {
    if grade > max_grade {
        format!("{} passing out of {} [!]", grade, max_grade)
    } else if grade == max_grade {
        format!("all {} passing", max_grade)
    } else if grade == 0 {
        format!("all {} failing", max_grade)
    } else {
        format!("{} failing out of {}", max_grade - grade, max_grade)
    }
}
