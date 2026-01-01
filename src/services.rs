use anyhow::{Context, Result, anyhow};
use azure_devops_rust_api::Credential;
use azure_devops_rust_api::core::ClientBuilder as CoreClientBuilder;
use azure_devops_rust_api::processes::ClientBuilder as ProcessesClientBuilder;
use azure_devops_rust_api::processes::models::FormLayout;
use azure_devops_rust_api::wit::ClientBuilder as WitClientBuilder;
use azure_devops_rust_api::wit::models::json_patch_operation::Op;
use azure_devops_rust_api::wit::models::{JsonPatchOperation, WorkItem as ADOWorkItem};
use azure_devops_rust_api::work::ClientBuilder as WorkClientBuilder;
use azure_identity::AzureCliCredential;

use crate::config::BoardConfig;
use crate::models::{WorkItem, clean_ado_text};

fn authenticate_with_cli_credential() -> Result<Credential> {
    let azure_cli_credential = AzureCliCredential::new(None)?;
    Ok(Credential::from_token_credential(azure_cli_credential))
}

fn get_credential() -> Result<Credential> {
    match std::env::var("ADO_TOKEN") {
        Ok(token) if !token.is_empty() => {
            println!("Authenticate using PAT provided via $ADO_TOKEN");
            Ok(Credential::from_pat(token))
        }
        _ => authenticate_with_cli_credential(),
    }
}

pub async fn resolve_iteration_id(
    organization: &str,
    project: &str,
    team: &str,
    iteration_path: &str,
) -> Result<String> {
    let credential = get_credential()?;
    let work_client = WorkClientBuilder::new(credential).build();

    // Fetch all iterations for the team and match by path or name
    let iterations = work_client
        .iterations_client()
        .list(organization, project, team)
        .await?
        .value;

    let matched = iterations
        .into_iter()
        .find(|i| match (&i.path, &i.name) {
            (Some(path), _) if path == iteration_path => true,
            (_, Some(name)) if name == iteration_path => true,
            _ => false,
        })
        .and_then(|i| i.id);

    matched.ok_or_else(|| {
        anyhow::anyhow!("Iteration not found for team '{team}' and path or name '{iteration_path}'")
    })
}

pub async fn get_iteration_ids(
    organization: &str,
    project: &str,
    team: &str,
    iteration_id: &str,
) -> Result<Vec<i32>> {
    let credential = get_credential()?;
    let work_client = WorkClientBuilder::new(credential).build();
    let iteration_work_items = work_client
        .iterations_client()
        .get_iteration_work_items(organization, project, iteration_id, team)
        .await?;
    let work_item_ids: Vec<i32> = iteration_work_items
        .work_item_relations
        .into_iter()
        .filter_map(|wi_link| wi_link.target)
        .filter_map(|wi| wi.id)
        .collect();
    Ok(work_item_ids)
}

pub async fn get_backlog_ids(organization: &str, project: &str, team: &str) -> Result<Vec<i32>> {
    let credential = get_credential()?;
    let work_client = WorkClientBuilder::new(credential).build();

    // Black magic string
    let backlog_level = "Microsoft.RequirementCategory";

    let backlog_result = work_client
        .backlogs_client()
        .get_backlog_level_work_items(organization, project, team, backlog_level)
        .await?;

    let work_item_ids: Vec<i32> = backlog_result
        .work_items
        .into_iter()
        .filter_map(|wi_link| wi_link.target)
        .filter_map(|wi| wi.id)
        .collect();

    Ok(work_item_ids)
}

pub async fn get_items(
    organization: &str,
    project: &str,
    work_item_ids: Vec<i32>,
) -> Result<Vec<WorkItem>> {
    let credential = get_credential()?;

    let ids: String = work_item_ids
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let wit_client = WitClientBuilder::new(credential).build();

    let full_items = wit_client
        .work_items_client()
        .list(organization, ids, project)
        .await?;

    let items = full_items.value.into_iter().map(WorkItem::from).collect();
    Ok(items)
}

pub async fn fetch_project_id(organization: &str, project_name: &str) -> Result<String> {
    let credential = get_credential()?;
    let core_client = CoreClientBuilder::new(credential).build();

    let project = core_client
        .projects_client()
        .get(organization, project_name)
        .await?
        .team_project_reference
        .id
        .ok_or_else(|| anyhow!("Project id missing in response"))?;

    Ok(project)
}

pub async fn fetch_process_template_type(organization: &str, project_id: &str) -> Result<String> {
    let credential = get_credential()?;
    let core_client = CoreClientBuilder::new(credential).build();

    let properties = core_client
        .projects_client()
        .get_project_properties(organization, project_id)
        .keys("System.ProcessTemplateType")
        .await?;

    let process_template_type = properties
        .value
        .into_iter()
        .find(|prop| prop.name.as_deref() == Some("System.ProcessTemplateType"))
        .and_then(|prop| prop.value)
        .and_then(|val| val.as_str().map(|s| s.to_string()))
        .ok_or_else(|| anyhow!("System.ProcessTemplateType not found"))?;

    Ok(process_template_type)
}

pub async fn fetch_process_work_item_types(
    organization: &str,
    process_id: &str,
) -> Result<Vec<(String, String)>> {
    let credential = get_credential()?;
    let processes_client = ProcessesClientBuilder::new(credential).build();

    let work_item_types = processes_client
        .work_item_types_client()
        .list(organization, process_id)
        .await?
        .value;

    let types = work_item_types
        .into_iter()
        .filter_map(|t| {
            let name = t.name?;
            let reference_name = t.reference_name?;
            Some((name, reference_name))
        })
        .collect();

    Ok(types)
}

pub async fn fetch_work_item_layout(
    organization: &str,
    process_id: &str,
    wit_ref_name: &str,
) -> Result<FormLayout> {
    let credential = get_credential()?;
    let processes_client = ProcessesClientBuilder::new(credential).build();

    processes_client
        .layout_client()
        .get(organization, process_id, wit_ref_name)
        .await
        .context("Failed to fetch work item layout")
}

pub async fn update_work_item_in_ado(
    board: &BoardConfig,
    item: &WorkItem,
    state: &crate::app::DetailEditState,
) -> Result<()> {
    let credential = get_credential()?;
    let wit_client = WitClientBuilder::new(credential).build();

    let mut operations = vec![JsonPatchOperation {
        from: None,
        op: Some(Op::Replace),
        path: Some("/fields/System.Title".to_string()),
        value: Some(serde_json::json!(state.title.clone())),
    }];

    for (_, reference, value) in &state.visible_fields {
        operations.push(JsonPatchOperation {
            from: None,
            op: Some(Op::Replace),
            path: Some(format!("/fields/{}", reference)),
            value: Some(serde_json::json!(value.clone())),
        });
    }

    wit_client
        .work_items_client()
        .update(
            &board.organization,
            operations,
            item.id as i32,
            &board.project,
        )
        .await
        .map(|_| ())
        .map_err(anyhow::Error::from)
}

impl From<ADOWorkItem> for WorkItem {
    fn from(item: ADOWorkItem) -> Self {
        let get_and_clean_field = |key: &str| -> String {
            item.fields
                .get(key)
                .and_then(|v| v.as_str())
                .map_or("".to_string(), clean_ado_text)
        };
        let assigned_to_name: String = item
            .fields
            .get("System.AssignedTo")
            .and_then(|assigned_to| assigned_to.as_object())
            .and_then(|assigned_to| assigned_to.get("displayName"))
            .and_then(|display_name| display_name.as_str())
            .map(|s| s.to_string())
            .unwrap_or("Unassigned".to_string());

        let fields = item
            .fields
            .as_object()
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|v| (key.clone(), clean_ado_text(v)))
                    })
                    .collect()
            })
            .unwrap_or_default();

        WorkItem {
            id: item.id as u32,
            title: get_and_clean_field("System.Title"),
            work_item_type: get_and_clean_field("System.WorkItemType"),
            description: get_and_clean_field("System.Description"),
            acceptance_criteria: get_and_clean_field("Microsoft.VSTS.Common.AcceptanceCriteria"),
            assigned_to: assigned_to_name,
            state: get_and_clean_field("System.State"),
            fields,
        }
    }
}
