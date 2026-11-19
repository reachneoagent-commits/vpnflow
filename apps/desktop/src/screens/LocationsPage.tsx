import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { Locations } from "./Locations";
import { Profiles } from "./Profiles";

/** One page: add servers at the top, pick and manage them below, raw WireGuard profiles at the bottom. */
export function LocationsPage({ showAdvanced = false }: { showAdvanced?: boolean }) {
  const [open, setOpen] = useState(showAdvanced);
  return (
    <div className="flex flex-col gap-8">
      <Locations />
      <section aria-label="Advanced WireGuard" className="border-t border-line pt-5">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center justify-between gap-3 text-left"
        >
          <span>
            <span className="block text-base font-bold">Advanced WireGuard</span>
            <span className="block text-xs text-muted">
              Every imported profile in one plain list: edit, export, duplicate or delete several at once.
            </span>
          </span>
          <ChevronDown size={18} aria-hidden className={open ? "rotate-180" : ""} />
        </button>
        {open && (
          <div className="mt-4">
            <Profiles embedded />
          </div>
        )}
      </section>
    </div>
  );
}
