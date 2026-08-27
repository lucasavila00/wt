use super::CONTEXT_REQUEST_TIMEOUT;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use wt_client::config::ClientConfig;
use wt_client::inventory::ContextWorld;
use wt_control_protocol::{
    ApiRequest, GitActivity, GitActivityKind, GitActivityQuery, Operation, Response, WorldId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RepositoryInteraction {
    pub(super) target: String,
    pub(super) wrote: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WorldGitActivity {
    pub(super) context: String,
    pub(super) world_id: WorldId,
    pub(super) repositories: Vec<RepositoryInteraction>,
}

pub(super) fn load(
    config: &ClientConfig,
    worlds: &[ContextWorld],
    cancelled: &AtomicBool,
) -> Vec<WorldGitActivity> {
    worlds
        .iter()
        .filter_map(|world| {
            let context = config.context(&world.context)?;
            let request = ApiRequest::new(Operation::ListGitActivity {
                query: GitActivityQuery::World {
                    world_id: world.world.world_id,
                    before_id: None,
                },
            });
            let response = wt_client::transport::call_with_timeout_until(
                context,
                &request,
                CONTEXT_REQUEST_TIMEOUT,
                cancelled,
            )
            .ok()?;
            let Response::GitActivity { activity } = response else {
                return None;
            };
            Some(WorldGitActivity {
                context: world.context.clone(),
                world_id: world.world.world_id,
                repositories: recent_repositories(activity),
            })
        })
        .collect()
}

fn recent_repositories(activity: Vec<GitActivity>) -> Vec<RepositoryInteraction> {
    let mut repositories = BTreeMap::new();
    for (position, entry) in activity.into_iter().enumerate() {
        let target = format!("{}/{}", entry.provider_host, entry.repository);
        let wrote = entry.kind == GitActivityKind::BranchUpdate
            || entry.git_service.as_deref() == Some("git-receive-pack");
        let summary = repositories
            .entry(target.clone())
            .or_insert((position, false));
        summary.1 |= wrote;
    }
    let mut repositories = repositories
        .into_iter()
        .map(|(target, (position, wrote))| (position, RepositoryInteraction { target, wrote }))
        .collect::<Vec<_>>();
    repositories.sort_by_key(|(position, repository)| (!repository.wrote, *position));
    repositories
        .into_iter()
        .take(3)
        .map(|(_, repository)| repository)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wt_control_protocol::WorldName;

    fn activity(
        id: u64,
        repository: &str,
        kind: GitActivityKind,
        git_service: Option<&str>,
    ) -> GitActivity {
        GitActivity {
            id,
            world_id: WorldId::new(),
            world_name: WorldName::parse("world").unwrap(),
            recorded_at_unix_ms: id,
            kind,
            provider_host: "github.com".into(),
            repository: repository.into(),
            git_service: git_service.map(str::to_owned),
            branch: None,
            previous_oid: None,
            new_oid: None,
        }
    }

    #[test]
    fn keeps_three_recent_repositories_with_writes_first() {
        let repositories = recent_repositories(vec![
            activity(
                6,
                "read-newest",
                GitActivityKind::Service,
                Some("git-upload-pack"),
            ),
            activity(
                5,
                "write-oldest",
                GitActivityKind::Service,
                Some("git-receive-pack"),
            ),
            activity(4, "write-newest", GitActivityKind::BranchUpdate, None),
            activity(
                3,
                "read-second",
                GitActivityKind::Service,
                Some("git-upload-pack"),
            ),
            activity(
                2,
                "read-third",
                GitActivityKind::Service,
                Some("git-upload-pack"),
            ),
        ]);

        insta::assert_debug_snapshot!(repositories, @r###"
        [
            RepositoryInteraction {
                target: "github.com/write-oldest",
                wrote: true,
            },
            RepositoryInteraction {
                target: "github.com/write-newest",
                wrote: true,
            },
            RepositoryInteraction {
                target: "github.com/read-newest",
                wrote: false,
            },
        ]
        "###);
    }
}
