export interface Task {
  id: string;
  title: string;
  labels: string[];
  status: "open" | "completed";
  completedAt?: Date;
}

export class TaskNotFoundError extends Error {}

export class InvalidTaskTitleError extends Error {}
