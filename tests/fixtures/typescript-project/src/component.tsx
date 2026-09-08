export interface GreetingProps {
  name: string;
}

export function Greeting({ name }: GreetingProps) {
  return <div>Hello {name}</div>;
}

