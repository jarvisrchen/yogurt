import { useGreeting } from "../../hooks/useGreeting";

interface Props {
  count: number;
}

export function Greeting({ count }: Props) {
  const { greeting } = useGreeting();
  const plural = count === 1 ? "meeting" : "meetings";
  return (
    <header className="mb-8">
      <h1 className="heading-greeting">{greeting}</h1>
      <p className="mt-1 text-[13px] text-mut">
        {count} {plural} · all on this Mac
      </p>
    </header>
  );
}
