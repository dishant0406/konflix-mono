import { Button } from "@/components/button";
import { StateCard } from "@/components/states";

export default function NotFound() {
  return (
    <StateCard
      eyebrow="404"
      title="Nothing here"
      detail="That page does not exist. The title may have been removed, or the link may be wrong."
      className="min-h-[70vh]"
    >
      <Button href="/">Back to home</Button>
    </StateCard>
  );
}
