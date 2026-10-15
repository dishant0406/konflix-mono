import { ATTRIBUTION } from "@/constants/site";

/** One quiet line at the foot of the page: the attribution the providers' licences ask for. */
export default function Footer() {
  return (
    <footer className="gutter pb-10 pt-6">
      <p className="max-w-3xl text-xs leading-5 text-faint">{ATTRIBUTION}</p>
    </footer>
  );
}
