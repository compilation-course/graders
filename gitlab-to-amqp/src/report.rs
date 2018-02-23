use errors::Result;
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

pub fn yaml_to_markdown(step: &str, yaml: &str) -> Result<String> {
    let report: Report = serde_yaml::from_str(yaml)?;
    if let Some(explanation) = report.explanation {
        return Ok(format!(
            r#"## There has been an error during the test
```
{}
```"#,
            explanation
        ));
    }
    let groups = report
        .groups
        .unwrap_or(vec![])
        .iter()
        .map(|group| {
            let tests = group
                .tests
                .iter()
                .map(|test| {
                    format!(
                        "- {} (coefficient {}): {}",
                        &test.description,
                        test.coefficient,
                        if test.success {
                            "success"
                        } else {
                            "**failure**"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
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
    let report = format!("## Report for {} ({})\n\n{}", step, grade, groups);
    Ok(report)
}
