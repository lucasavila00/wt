/** State transition accepted by the set_mr command. */
export type ChangeRequestState = "ready" | "draft" | "open" | "closed";

/** Explicit Git hosting provider and repository. */
export type GitHostingTarget = {
  provider: "github" | "gitlab";
  repository: string;
};

/** Command sent to a Git hosting provider. */
export type GitHostingCommand =
  | { action: "show_mr"; mr: string }
  | { action: "show_mr_for_branch"; branch: string }
  | { action: "show_run"; run: string }
  | { action: "show_job"; job: string }
  | { action: "list_threads"; mr: string }
  | { action: "list_ci"; commit: string }
  | { action: "list_jobs"; run: string }
  | { action: "log_job"; job: string }
  | { action: "wait_mr"; mr: string; timeout_seconds?: number }
  | { action: "wait_run"; run: string; timeout_seconds?: number }
  | { action: "wait_job"; job: string; timeout_seconds?: number }
  | { action: "open_mr"; head: string; base: string; draft?: boolean }
  | { action: "set_mr"; mr: string; state: ChangeRequestState }
  | { action: "edit_mr"; mr: string; title?: string; body?: string }
  | { action: "comment_mr"; mr: string; body: string }
  | { action: "reply_thread"; mr: string; thread: string; body: string }
  | { action: "set_thread"; mr: string; thread: string; resolved: boolean }
  | { action: "retry_job"; job: string }
  | { action: "cancel_job"; job: string }
  | { action: "cancel_run"; run: string };

/** Feedback about wt-tools itself. */
export type WtToolsFeedbackCommand =
  | { action: "report_wt_tool_bug"; description: string }
  | { action: "report_wt_tool_issue"; description: string }
  | { action: "suggest_wt_tool_improvement"; description: string }
  | { action: "request_wt_tool_feature"; description: string };

/** One JSON command accepted by wt-tools. */
export type WtToolsCommand =
  | { target: GitHostingTarget; command: GitHostingCommand }
  | { command: WtToolsFeedbackCommand };
