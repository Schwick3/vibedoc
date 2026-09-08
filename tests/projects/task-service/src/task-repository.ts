import type { Task } from "./task.js";

export interface TaskRepository {
  findById(taskId: string): Promise<Task | undefined>;
  save(task: Task): Promise<void>;
}
