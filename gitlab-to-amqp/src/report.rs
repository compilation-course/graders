use config::Configuration;
use errors::Result;
use gitlab::api::{self, State};
use graders_utils::amqputils::AMQPResponse;
use hyper::Request;
use serde_json;
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
}

fn yaml_to_markdown(step: &str, yaml: &str) -> Result<(String, usize, usize)> {
    let report: Report = serde_yaml::from_str(yaml)?;
    if let Some(explanation) = report.explanation {
        warn!("problem during handling of {}: {}", step, explanation);
        return Ok((
            format!(
                r#"## Error

There has been an error during the test for {}:

```
{}
```"#,
                step, explanation
            ),
            report.grade,
            report.max_grade,
        ));
    }
    let groups = report
        .groups
        .unwrap_or(vec![])
        .iter()
        .filter(|group| group.grade != group.max_grade)
        .map(|group| {
            let tests = if group.grade != 0 {
                let mut explanation = "Failed tests:\n\n".to_owned();
                explanation.push_str(&group
                    .tests
                    .iter()
                    .filter(|test| !test.success)
                    .map(|test| {
                        format!(
                            "- {}{}",
                            &test.description,
                            if test.coefficient != 1 {
                                format!(" (coefficient {})", test.coefficient)
                            } else {
                                "".to_owned()
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"));
                explanation
            } else {
                String::new()
            };
            let mut grade = format!("{}/{}", group.grade, group.max_grade);
            if group.grade != group.max_grade {
                grade = format!("**{}**", grade);
            }
            format!(
                "### {} ({})\n\n{}\n",
                group
                    .description
                    .clone()
                    .unwrap_or("*Test group*".to_owned()),
                grade,
                tests
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut grade = format!("{}/{}", report.grade, report.max_grade);
    if report.grade != report.max_grade {
        grade = format!("**{}**", grade);
    }
    let diagnostic = format!(
        "## Failed tests reports for {} ({})\n\n{}",
        step, grade, groups
    );
    Ok((diagnostic, report.grade, report.max_grade))
}

pub fn response_to_post(config: &Configuration, response: &AMQPResponse) -> Result<Vec<Request>> {
    let (report, grade, max_grade) = yaml_to_markdown(&response.step, &response.yaml_result)?;
    let hook = serde_json::from_str(&response.opaque)?;
    let state = if grade == max_grade {
        State::Success
    } else {
        State::Failed
    };
    let status = api::post_status(
        &config.gitlab,
        &hook,
        &state,
        Some(&hook.ref_),
        &response.step,
        Some(&format!("grade: {}/{}", grade, max_grade)),
    );
    Ok(if state == State::Success {
        trace!(
            "tests for {} are a success, generating status only",
            &response.step
        );
        vec![status]
    } else {
        trace!(
            "tests for {} are a failure, generating status and comment",
            &response.step
        );
        let comment = api::post_comment(&config.gitlab, &hook, &report);
        vec![status, comment]
    })
}
