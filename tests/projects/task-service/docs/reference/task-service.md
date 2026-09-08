# Task service reference

<!-- vibedoc:source adapter="typescript" path="src/task-service.ts" symbol="TaskService.complete" -->
## `TaskService.complete`

`TaskService.complete` records the completion time and saves the updated task.

### Parameters

- `taskId` (`string`): The task ID.
- `completedAt` (`Date`): The completion time.

### Returns

`Promise<Task>`

### Errors

- `TaskNotFoundError`: No task has the specified task ID.

<!-- vibedoc:source adapter="typescript" path="src/task-service.ts" symbol="createTask" -->
## `createTask`

`createTask` creates an open task.

### Parameters

- `id` (`string`): The task ID.
- `title` (`string`): The task title.
- `labels` (`string[]`): The labels assigned to the task.

### Returns

`Task`

### Errors

- `InvalidTaskTitleError`: The task title is empty.
