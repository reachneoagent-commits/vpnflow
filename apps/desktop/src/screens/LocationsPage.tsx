import { ChevronDown } from "lucide-react";
import { useState } from "react";
import { Locations } from "./Locations";
import { Profiles } from "./Profiles";

/** One page: add servers at the top, pick and manage them below, raw WireGuard profiles at the bottom (open by default). */
export function LocationsPage() {
  const [open, setOpen] = useState(true);
  return (
    <div className="flex flex-col gap-8">
      <Locations />
      <section aria-label="Advanced WireGuard">
        <button
          type="button"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
          className="flex w-full items-center justify-between gap-3 rounded-xl bg-raised px-5 py-4 text-left"
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
