import { Link } from "react-router";
import { useGreeting } from "../../hooks/useGreeting";
import { useCloudStatus } from "../../lib/api/cloud";
import { Pill } from "../Pill";
import { formatCount } from "./pagination";

interface Props {
  count: number;
}

export function Greeting({ count }: Props) {
  const { greeting } = useGreeting();
  const cloud = useCloudStatus().data;
  const notPushed = cloud?.connected ? cloud.not_pushed : 0;
  const pushing = cloud?.connected ? cloud.push : null;
  const plural = count === 1 ? "meeting" : "meetings";
  return (
    <header className="mb-8">
      <h1 className="heading-greeting">{greeting}</h1>
      <p className="mt-1 flex flex-wrap items-center gap-1.5 text-[13px] text-mut">
        <span>
          {formatCount(count)} {plural} · all on this Mac
        </span>
        {(notPushed > 0 || pushing) && (
          <>
            <span aria-hidden="true">·</span>
            <Link to="/settings/cloud" className="rounded-pill focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue">
              <Pill tone="honey" className="px-2.5! py-[3px]! text-[10.5px]! font-bold!">
                {pushing && pushing.total > 0
                  ? `Pushing ${formatCount(pushing.sent)} of ${formatCount(pushing.total)}`
                  : pushing
                    ? "Pushing"
                    : `${formatCount(notPushed)} not pushed`}
              </Pill>
            </Link>
          </>
        )}
      </p>
    </header>
  );
}
