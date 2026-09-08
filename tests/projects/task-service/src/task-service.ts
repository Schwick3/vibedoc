import type { TaskRepository } from "./task-repository.js";
import {
  InvalidTaskTitleError,
  type Task,
  TaskNotFoundError,
} from "./task.js";

export class TaskService {
  constructor(private readonly repository: TaskRepository) {}

  async complete(taskId: string, completedAt: Date): Promise<Task> {
    const task = await this.repository.findById(taskId);
    if (!task) {
      throw new TaskNotFoundError(taskId);
    }

    const completed: Task = {
      ...task,
      status: "completed",
      completedAt,
    };
    await this.repository.save(completed);
    return completed;
  }
}

export function createTask(
  id: string,
  title: string,
  labels: string[] = [],
): Task {
  if (title.trim().length === 0) {
    throw new InvalidTaskTitleError(title);
  }
  return { id, title, labels, status: "open" };
}
