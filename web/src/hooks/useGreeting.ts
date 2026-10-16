export type TimeOfDay = "morning" | "afternoon" | "evening";

export interface Greeting {
  timeOfDay: TimeOfDay;
  name: string;
  greeting: string;
}

export function greetingFor(now: Date, nameOverride?: string): Greeting {
  const h = now.getHours();
  const timeOfDay: TimeOfDay =
    h < 12 ? "morning" : h < 18 ? "afternoon" : "evening";
  const name = (nameOverride ?? "").trim();
  return {
    timeOfDay,
    name,
    greeting: name ? `Good ${timeOfDay}, ${name}` : `Good ${timeOfDay}`,
  };
}

export function useGreeting(nameOverride?: string): Greeting {
  return greetingFor(new Date(), nameOverride);
}
