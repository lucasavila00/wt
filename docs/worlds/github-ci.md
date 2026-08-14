# GitHub CI world foundation

This checkout contains the typed registry records, capacity admission, config
validation, libvirt backend, and single-job lifecycle library. It does not yet
ship the `wt-runner` executable, service installer, or runner image builder.

A `github-ci` world runs one GitHub Actions job in a fresh KVM guest. It is not
created, listed, entered, restarted, or removed with the `wt` client.

The intended `wt-runner` service owns this lifecycle:

1. Reserve shared CPU, RAM, and disk capacity.
2. Create a guest from the runner image.
3. Supply a short-lived GitHub JIT configuration.
4. Run one official Actions runner process.
5. Destroy the guest and disk and release capacity.

Cleanup runs after success, failure, cancellation, or timeout. Startup
reconciliation removes recorded runner guests before accepting work. A failed
cleanup keeps its capacity reservation.

Runner guests have no interactive SSH, devcontainer, Byobu session, or agent
Git grant. GitHub App credentials remain with the runner service; the guest sees
only its job JIT configuration and job-provided credentials.

GitHub remains authoritative for workflow status, logs, artifacts, and
cancellation. WT retains only operator diagnostics outside the guest.
