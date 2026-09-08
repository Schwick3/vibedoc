# Task service reference

<!-- vibedoc:source adapter="typescript" path="src/task-service.ts" symbol="TaskService.complete" -->
## `TaskService.complete`

`TaskService.complete` securely handles the task identifier and efficiently manages every part of the task completion operation for all callers in the application without requiring them to understand any implementation details.

#### Parameters

- `taskId` (`number`): The task key.
- `unknown` (`string`): An unknown value.

### Returns

`Task`

### Errors

- `NetworkError`: The repository request failed.

Read more [here](../README.md).

`TaskService.complete` returns `Task`.
`TaskService.complete` calls `sendNotification`.
